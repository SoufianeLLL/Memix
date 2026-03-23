// Manages the lifecycle of skeleton entry embeddings using a write-through cache.
//
// Read path:  disk file (.bin) → Redis hash → recompute
// Write path: compute → write to disk file AND Redis hash simultaneously
//
// The disk file uses a simple format: a 4-byte entry count header, followed by
// fixed-size records. Each record is:
//   - 128 bytes: entry ID (null-padded UTF-8 string)
//   - 1536 bytes: 384 × f32 embedding vector (384 × 4 bytes)
// Total per record: 1664 bytes.
//
// This gives O(1) random access by entry index and makes the file mmap-able
// if we ever want to go faster. A project with 1,000 indexed files produces
// a file of approximately 1.6 MB — trivially small.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

const EMBEDDING_DIM: usize = 384;
const ENTRY_ID_BYTES: usize = 128;
const RECORD_SIZE: usize = ENTRY_ID_BYTES + EMBEDDING_DIM * 4;
const REDIS_EMBEDDING_KEY_PREFIX: &str = "embeddings:";

/// A loaded embedding record: entry ID and its 384-float vector.
#[derive(Debug, Clone)]
pub struct EmbeddingRecord {
    pub entry_id: String,
    pub vector: Vec<f32>,
}

/// The embedding store manages persistence and retrieval of vectors.
/// It is cheap to clone (Arc-wrapped internally) and safe to share
/// across async tasks.
#[derive(Clone)]
pub struct EmbeddingStore {
    inner: Arc<EmbeddingStoreInner>,
}

struct EmbeddingStoreInner {
    /// In-memory index: entry_id → position in the embedding matrix
    index: RwLock<HashMap<String, usize>>,
    /// The flat embedding matrix — all vectors concatenated row-by-row
    matrix: RwLock<Vec<Vec<f32>>>,
    /// Reverse index: position → entry_id
    id_by_position: RwLock<Vec<String>>,
    /// Path to the .bin file on disk
    bin_path: PathBuf,
    /// Redis key that stores the same data (for multi-IDE sharing)
    redis_key: String,
    /// Dirty flag: if true, the in-memory state has unsaved changes
    dirty: std::sync::atomic::AtomicBool,
}

impl EmbeddingStore {
    /// Load from disk first, then Redis if disk is missing or stale.
    /// This is the startup path — call once per daemon session.
    pub async fn load(
        project_id: &str,
        data_dir: &Path,
        redis_client: Option<&redis::Client>,
    ) -> Result<Self> {
        let bin_path = data_dir.join("skeleton_embeddings.bin");
        let redis_key = format!("{}{}", REDIS_EMBEDDING_KEY_PREFIX, project_id);

        let mut index = HashMap::new();
        let mut matrix: Vec<Vec<f32>> = Vec::new();
        let mut id_by_position: Vec<String> = Vec::new();

        // Step 1: Try to load from disk (fastest path)
        let loaded_from_disk = Self::load_from_disk(&bin_path, &mut index, &mut matrix, &mut id_by_position);

        // Step 2: If disk was empty or failed, try Redis (covers multi-IDE case where
        // another instance already computed the embeddings)
        if !loaded_from_disk {
            if let Some(client) = redis_client {
                let _ = Self::load_from_redis(
                    client,
                    &redis_key,
                    &mut index,
                    &mut matrix,
                    &mut id_by_position,
                ).await;
                // If we loaded from Redis, sync to disk immediately for next time
                if !index.is_empty() {
                    let _ = Self::persist_to_disk_internal(
                        &bin_path,
                        &id_by_position,
                        &matrix,
                    );
                    tracing::info!(
                        "EmbeddingStore: loaded {} vectors from Redis, synced to disk",
                        index.len()
                    );
                }
            }
        } else {
            tracing::info!("EmbeddingStore: loaded {} vectors from disk", index.len());
        }

        Ok(Self {
            inner: Arc::new(EmbeddingStoreInner {
                index: RwLock::new(index),
                matrix: RwLock::new(matrix),
                id_by_position: RwLock::new(id_by_position),
                bin_path,
                redis_key,
                dirty: std::sync::atomic::AtomicBool::new(false),
            }),
        })
    }

    pub fn empty(data_dir: &Path, project_id: &str) -> Self {
        let bin_path = data_dir.join("skeleton_embeddings.bin");
        let redis_key = format!("{}{}", REDIS_EMBEDDING_KEY_PREFIX, project_id);
        Self {
            inner: Arc::new(EmbeddingStoreInner {
                index: RwLock::new(HashMap::new()),
                matrix: RwLock::new(Vec::new()),
                id_by_position: RwLock::new(Vec::new()),
                bin_path,
                redis_key,
                dirty: std::sync::atomic::AtomicBool::new(false),
            }),
        }
    }

    /// Loads embedding data from disk (and Redis fallback) into an existing store.
    /// Used for deferred loading after the daemon socket is already bound.
    pub async fn load_into(
        store: &EmbeddingStore,
        project_id: &str,
        data_dir: &std::path::Path,
        redis_client: Option<&redis::Client>,
    ) -> anyhow::Result<()> {
        let bin_path = data_dir.join("skeleton_embeddings.bin");
        let mut index = std::collections::HashMap::new();
        let mut matrix: Vec<Vec<f32>> = Vec::new();
        let mut ids: Vec<String> = Vec::new();

        let loaded_from_disk = Self::load_from_disk(&bin_path, &mut index, &mut matrix, &mut ids);

        if !loaded_from_disk {
            if let Some(client) = redis_client {
                let redis_key = format!("embeddings:{}", project_id);
                let _ = Self::load_from_redis(client, &redis_key, &mut index, &mut matrix, &mut ids).await;
            }
        }

        if !index.is_empty() {
            let mut store_index = store.inner.index.write().await;
            let mut store_matrix = store.inner.matrix.write().await;
            let mut store_ids = store.inner.id_by_position.write().await;
            *store_index = index;
            *store_matrix = matrix;
            *store_ids = ids;
            tracing::info!("EmbeddingStore: deferred load populated {} vectors", store_index.len());
        }

        Ok(())
    }

    /// Flush dirty state to the local binary file only — no Redis write.
    /// Use this during normal periodic flushes to avoid Redis network costs.
    /// Call flush() (with a Redis client) only on graceful shutdown or when
    /// multi-IDE support requires cross-instance synchronization.
    pub async fn flush_disk_only(&self) -> Result<()> {
        if !self.inner.dirty.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let matrix = self.inner.matrix.read().await;
        let ids = self.inner.id_by_position.read().await;
        Self::persist_to_disk_internal(&self.inner.bin_path, &ids, &matrix)?;
        self.inner.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
        tracing::debug!("EmbeddingStore: flushed {} vectors to disk (Redis sync skipped)", ids.len());
        Ok(())
    }

    fn load_from_disk(
        bin_path: &Path,
        index: &mut HashMap<String, usize>,
        matrix: &mut Vec<Vec<f32>>,
        id_by_position: &mut Vec<String>,
    ) -> bool {
        let Ok(mut file) = std::fs::File::open(bin_path) else { return false; };

        let mut count_buf = [0u8; 4];
        if file.read_exact(&mut count_buf).is_err() { return false; }
        let count = u32::from_le_bytes(count_buf) as usize;

        let mut record_buf = vec![0u8; RECORD_SIZE];
        for pos in 0..count {
            if file.read_exact(&mut record_buf).is_err() { break; }

            let id_bytes = &record_buf[..ENTRY_ID_BYTES];
            let null_pos = id_bytes.iter().position(|&b| b == 0).unwrap_or(ENTRY_ID_BYTES);
            let entry_id = String::from_utf8_lossy(&id_bytes[..null_pos]).to_string();

            let vector_bytes = &record_buf[ENTRY_ID_BYTES..];
            let vector: Vec<f32> = vector_bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            if vector.len() == EMBEDDING_DIM && !entry_id.is_empty() {
                index.insert(entry_id.clone(), pos);
                id_by_position.push(entry_id);
                matrix.push(vector);
            }
        }

        !index.is_empty()
    }

    async fn load_from_redis(
        client: &redis::Client,
        redis_key: &str,
        index: &mut HashMap<String, usize>,
        matrix: &mut Vec<Vec<f32>>,
        id_by_position: &mut Vec<String>,
    ) -> Result<()> {
        use redis::AsyncCommands;
        let mut conn = client.get_multiplexed_async_connection().await?;

        // Redis hash: field = entry_id, value = base64-encoded f32 bytes
        let entries: HashMap<String, Vec<u8>> = conn.hgetall(redis_key).await?;
        for (entry_id, bytes) in entries {
            let vector: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            if vector.len() == EMBEDDING_DIM {
                let pos = matrix.len();
                index.insert(entry_id.clone(), pos);
                id_by_position.push(entry_id);
                matrix.push(vector);
            }
        }
        Ok(())
    }

    /// Insert or update an embedding. Writes to in-memory cache immediately,
    /// marks dirty. The caller should batch writes and call `flush` periodically.
    pub async fn upsert(&self, entry_id: &str, vector: Vec<f32>) {
        if vector.len() != EMBEDDING_DIM {
            tracing::warn!("EmbeddingStore: ignoring vector with wrong dimension {} for {}", vector.len(), entry_id);
            return;
        }

        let mut index = self.inner.index.write().await;
        let mut matrix = self.inner.matrix.write().await;
        let mut ids = self.inner.id_by_position.write().await;

        if let Some(&pos) = index.get(entry_id) {
            // Update in place
            matrix[pos] = vector;
        } else {
            // Append new entry
            let pos = matrix.len();
            index.insert(entry_id.to_string(), pos);
            ids.push(entry_id.to_string());
            matrix.push(vector);
        }

        self.inner.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Remove an embedding when a file is deleted.
    pub async fn remove(&self, entry_id: &str) {
        let mut index = self.inner.index.write().await;
        if let Some(pos) = index.remove(entry_id) {
            let mut matrix = self.inner.matrix.write().await;
            let mut ids = self.inner.id_by_position.write().await;
            // Swap-remove for O(1) deletion — updates the moved element's index entry
            let last = ids.len().saturating_sub(1);
            if pos < last {
                ids.swap(pos, last);
                matrix.swap(pos, last);
                // Fix the index for the element we just moved
                if let Some(moved_id) = ids.get(pos) {
                    index.insert(moved_id.clone(), pos);
                }
            }
            ids.truncate(last);
            matrix.truncate(last);
        }
        self.inner.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Look up a single vector by entry ID.
    pub async fn get(&self, entry_id: &str) -> Option<Vec<f32>> {
        let index = self.inner.index.read().await;
        let pos = *index.get(entry_id)?;
        let matrix = self.inner.matrix.read().await;
        matrix.get(pos).cloned()
    }

    /// Find the top-k most similar entries to the query vector using cosine similarity.
    /// This is the semantic search path used by the context compiler.
    pub async fn search(&self, query: &[f32], top_k: usize) -> Vec<(String, f32)> {
        if query.len() != EMBEDDING_DIM {
            return Vec::new();
        }

        let _index = self.inner.index.read().await;
        let matrix = self.inner.matrix.read().await;
        let ids = self.inner.id_by_position.read().await;

        if matrix.is_empty() {
            return Vec::new();
        }

        // Normalize the query vector once for efficient cosine computation
        let query_norm: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
        if query_norm < f32::EPSILON {
            return Vec::new();
        }
        let query_normalized: Vec<f32> = query.iter().map(|x| x / query_norm).collect();

        // Score every entry — O(N × D) where N = entries, D = 384
        // For N = 2,000 entries: ~770k multiply-adds, completes in < 1ms on modern hardware
        let mut scores: Vec<(usize, f32)> = matrix
            .iter()
            .enumerate()
            .map(|(pos, vec)| {
                let vec_norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                if vec_norm < f32::EPSILON {
                    return (pos, 0.0_f32);
                }
                let dot: f32 = query_normalized
                    .iter()
                    .zip(vec.iter())
                    .map(|(q, v)| q * (v / vec_norm))
                    .sum();
                (pos, dot)
            })
            .collect();

        // Partial sort to get top-k — O(N log k) rather than O(N log N)
        scores.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);

        scores
            .into_iter()
            .filter_map(|(pos, score)| {
                ids.get(pos).map(|id| (id.clone(), score))
            })
            .collect()
    }

    /// Flush dirty state to disk and Redis simultaneously.
    /// This is the write-through step. Call from a background flush timer
    /// (every 30 seconds works well) and on graceful shutdown.
    pub async fn flush(&self, redis_client: Option<&redis::Client>) -> Result<()> {
        if !self.inner.dirty.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(()); // Nothing to do
        }

        let index = self.inner.index.read().await;
        let matrix = self.inner.matrix.read().await;
        let ids = self.inner.id_by_position.read().await;

        // Write disk file
        Self::persist_to_disk_internal(&self.inner.bin_path, &ids, &matrix)?;

        // Write Redis hash (background, non-blocking write — fire-and-forget)
        if let Some(client) = redis_client {
            let client = client.clone();
            let redis_key = self.inner.redis_key.clone();
            let embeddings: Vec<(String, Vec<u8>)> = ids
                .iter()
                .enumerate()
                .filter_map(|(pos, id)| {
                    matrix.get(pos).map(|vec| {
                        let bytes: Vec<u8> = vec
                            .iter()
                            .flat_map(|f| f.to_le_bytes())
                            .collect();
                        (id.clone(), bytes)
                    })
                })
                .collect();

            tokio::spawn(async move {
                if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                    use redis::AsyncCommands;
                    // Write all embeddings as a single HMSET for atomicity
                    let pairs: Vec<(String, Vec<u8>)> = embeddings;
                    if !pairs.is_empty() {
                        let _: redis::RedisResult<()> = conn.hset_multiple(
                            &redis_key,
                            &pairs.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect::<Vec<_>>(),
                        ).await;
                    }
                }
            });
        }

        self.inner.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
        tracing::debug!("EmbeddingStore: flushed {} vectors to disk + Redis", index.len());
        Ok(())
    }

    fn persist_to_disk_internal(
        bin_path: &Path,
        ids: &[String],
        matrix: &[Vec<f32>],
    ) -> Result<()> {
        if let Some(parent) = bin_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write to a temp file first, then rename for atomicity
        let tmp_path = bin_path.with_extension("bin.tmp");
        let mut file = std::fs::File::create(&tmp_path)?;

        let count = ids.len() as u32;
        file.write_all(&count.to_le_bytes())?;

        let mut id_buf = [0u8; ENTRY_ID_BYTES];
        for (id, vec) in ids.iter().zip(matrix.iter()) {
            // Write entry ID (null-padded to ENTRY_ID_BYTES)
            let id_bytes = id.as_bytes();
            let copy_len = id_bytes.len().min(ENTRY_ID_BYTES);
            id_buf[..copy_len].copy_from_slice(&id_bytes[..copy_len]);
            id_buf[copy_len..].fill(0);
            file.write_all(&id_buf)?;

            // Write embedding vector as little-endian f32 bytes
            for &val in vec {
                file.write_all(&val.to_le_bytes())?;
            }
        }
        file.flush()?;
        drop(file);

        std::fs::rename(tmp_path, bin_path)?;
        Ok(())
    }

    pub async fn len(&self) -> usize {
        self.inner.index.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.index.read().await.is_empty()
    }
}
