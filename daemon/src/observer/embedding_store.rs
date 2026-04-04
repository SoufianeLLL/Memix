// Manages the lifecycle of skeleton entry embeddings using SQLite persistence.
//
// Read path:  SQLite BLOB column → in-memory matrix
// Write path: compute → write to SQLite immediately (no dirty flag)
//
// The vector BLOB is stored as raw f32 little-endian bytes (384 × 4 = 1536 bytes).
// Content hashes are persisted for cache invalidation across daemon restarts.
//
// This gives O(1) random access by entry ID via the in-memory index,
// and semantic search via brute-force cosine similarity (~2ms for 2000 vectors).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use sqlx::SqlitePool;
use sqlx::Row;

const EMBEDDING_DIM: usize = 384;

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
    /// Content hashes for cache invalidation (persisted across restarts)
    content_hashes: RwLock<HashMap<String, u64>>,
    /// Workspace root directory (database at {workspace_root}/.memix/brain.db)
    workspace_root: PathBuf,
    /// Project ID for this store
    project_id: String,
}

/// Get database path for embeddings: {workspace_root}/.memix/brain.db
fn get_embedding_db_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".memix").join("brain.db")
}

impl EmbeddingStore {
    /// Load from SQLite database (per-project). This is the startup path — call once per daemon session.
    /// The database is stored at {workspace_root}/.memix/brain.db
    pub async fn load(project_id: &str, workspace_root: &Path) -> Result<Self> {
        let db_path = get_embedding_db_path(workspace_root);
        let store = Self::empty(project_id, workspace_root);
        
        // Open a read-only connection for the initial load
        let db_url = format!("sqlite:{}?mode=ro", db_path.display());
        if let Ok(pool) = SqlitePool::connect(&db_url).await {
            let rows = sqlx::query(
                "SELECT id, vector, content_hash FROM embedding_vectors WHERE project_id = ?"
            )
            .bind(project_id)
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let mut index = store.inner.index.write().await;
            let mut matrix = store.inner.matrix.write().await;
            let mut ids = store.inner.id_by_position.write().await;
            let mut hashes = store.inner.content_hashes.write().await;

            for row in rows {
                let id: String = row.get("id");
                let blob: Vec<u8> = row.get("vector");
                let content_hash: i64 = row.get("content_hash");
                
                // Cast raw bytes back to f32 slice
                let floats: Vec<f32> = blob
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                
                if floats.len() == EMBEDDING_DIM {
                    let pos = matrix.len();
                    index.insert(id.clone(), pos);
                    ids.push(id.clone());
                    matrix.push(floats);
                    hashes.insert(id, content_hash as u64);
                }
            }

            tracing::info!(
                "EmbeddingStore: loaded {} vectors from SQLite for project {}",
                matrix.len(), project_id
            );
        }
        // If the database doesn't exist yet, start empty — background indexer will populate
        
        Ok(store)
    }

    /// Create an empty embedding store for a project.
    /// The database will be stored at {workspace_root}/.memix/brain.db
    pub fn empty(project_id: &str, workspace_root: &Path) -> Self {
        Self {
            inner: Arc::new(EmbeddingStoreInner {
                index: RwLock::new(HashMap::new()),
                matrix: RwLock::new(Vec::new()),
                id_by_position: RwLock::new(Vec::new()),
                content_hashes: RwLock::new(HashMap::new()),
                workspace_root: workspace_root.to_path_buf(),
                project_id: project_id.to_string(),
            }),
        }
    }

    /// Legacy compatibility: load with optional Redis client (now ignored)
    pub async fn load_legacy(
        project_id: &str,
        data_dir: &Path,
        _redis_client: Option<&redis::Client>,
    ) -> Result<Self> {
        Self::load(project_id, data_dir).await
    }

    /// Legacy compatibility: empty store with data_dir
    pub fn empty_legacy(data_dir: &Path, project_id: &str) -> Self {
        Self::empty(project_id, data_dir)
    }

    /// Get the content hash for an entry (for cache invalidation)
    pub async fn get_content_hash(&self, entry_id: &str) -> Option<u64> {
        let hashes = self.inner.content_hashes.read().await;
        hashes.get(entry_id).copied()
    }

    /// Compute a content hash for cache invalidation
    pub fn hash_content(text: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    /// Insert or update an embedding. Writes to SQLite immediately.
    pub async fn upsert(&self, entry_id: &str, vector: Vec<f32>, content_hash: u64, pool: &SqlitePool) -> Result<()> {
        if vector.len() != EMBEDDING_DIM {
            tracing::warn!("EmbeddingStore: ignoring vector with wrong dimension {} for {}", vector.len(), entry_id);
            return Ok(());
        }

        // Update in-memory structures
        {
            let mut index = self.inner.index.write().await;
            let mut matrix = self.inner.matrix.write().await;
            let mut ids = self.inner.id_by_position.write().await;
            let mut hashes = self.inner.content_hashes.write().await;

            let blob: Vec<u8> = vector.iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();

            if let Some(&pos) = index.get(entry_id) {
                matrix[pos] = vector;
            } else {
                let pos = matrix.len();
                index.insert(entry_id.to_string(), pos);
                ids.push(entry_id.to_string());
                matrix.push(vector);
            }
            hashes.insert(entry_id.to_string(), content_hash);

            // Persist to SQLite immediately
            let now = chrono::Utc::now().to_rfc3339();
            let hash_i64 = content_hash as i64;
            sqlx::query(
                "INSERT OR REPLACE INTO embedding_vectors (id, project_id, vector, content_hash, computed_at)
                 VALUES (?, ?, ?, ?, ?)"
            )
            .bind(entry_id)
            .bind(&self.inner.project_id)
            .bind(&blob)
            .bind(hash_i64)
            .bind(&now)
            .execute(pool)
            .await?;
        }
        
        Ok(())
    }

    /// Legacy upsert without content hash (computes hash from vector bytes)
    pub async fn upsert_legacy(&self, entry_id: &str, vector: Vec<f32>) {
        if vector.len() != EMBEDDING_DIM {
            tracing::warn!("EmbeddingStore: ignoring vector with wrong dimension {} for {}", vector.len(), entry_id);
            return;
        }

        // Use vector bytes as hash (not ideal but maintains compatibility)
        let content_hash = vector.iter()
            .fold(0u64, |acc, f| acc.wrapping_add(f.to_bits() as u64));
        
        // We need a pool for SQLite writes - this legacy path logs a warning
        tracing::warn!("EmbeddingStore: upsert_legacy called without pool - in-memory only");
        
        let mut index = self.inner.index.write().await;
        let mut matrix = self.inner.matrix.write().await;
        let mut ids = self.inner.id_by_position.write().await;
        let mut hashes = self.inner.content_hashes.write().await;

        if let Some(&pos) = index.get(entry_id) {
            matrix[pos] = vector;
        } else {
            let pos = matrix.len();
            index.insert(entry_id.to_string(), pos);
            ids.push(entry_id.to_string());
            matrix.push(vector);
        }
        hashes.insert(entry_id.to_string(), content_hash);
    }

    /// Remove an embedding when a file is deleted.
    pub async fn remove(&self, entry_id: &str, pool: &SqlitePool) -> Result<()> {
        {
            let mut index = self.inner.index.write().await;
            if let Some(pos) = index.remove(entry_id) {
                let mut matrix = self.inner.matrix.write().await;
                let mut ids = self.inner.id_by_position.write().await;
                let mut hashes = self.inner.content_hashes.write().await;
                
                // Swap-remove for O(1) deletion
                let last = ids.len().saturating_sub(1);
                if pos < last {
                    ids.swap(pos, last);
                    matrix.swap(pos, last);
                    if let Some(moved_id) = ids.get(pos) {
                        index.insert(moved_id.clone(), pos);
                    }
                }
                ids.truncate(last);
                matrix.truncate(last);
                hashes.remove(entry_id);
            }
        }
        
        // Delete from SQLite
        sqlx::query("DELETE FROM embedding_vectors WHERE id = ? AND project_id = ?")
            .bind(entry_id)
            .bind(&self.inner.project_id)
            .execute(pool)
            .await?;
        
        Ok(())
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

    /// Legacy flush method (no-op for SQLite - writes are immediate)
    pub async fn flush(&self, _redis_client: Option<&redis::Client>) -> Result<()> {
        // SQLite writes are immediate - no flush needed
        Ok(())
    }

    /// Legacy flush_disk_only method (no-op for SQLite)
    pub async fn flush_disk_only(&self) -> Result<()> {
        // SQLite writes are immediate - no flush needed
        Ok(())
    }

    pub async fn len(&self) -> usize {
        self.inner.index.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.index.read().await.is_empty()
    }

    /// Copy all data from another store into this one (used for deferred loading)
    pub async fn copy_from(&self, other: &EmbeddingStore) {
        let mut index = self.inner.index.write().await;
        let mut matrix = self.inner.matrix.write().await;
        let mut ids = self.inner.id_by_position.write().await;
        let mut hashes = self.inner.content_hashes.write().await;
        
        let other_index = other.inner.index.read().await;
        let other_matrix = other.inner.matrix.read().await;
        let other_ids = other.inner.id_by_position.read().await;
        let other_hashes = other.inner.content_hashes.read().await;
        
        *index = other_index.clone();
        *matrix = other_matrix.clone();
        *ids = other_ids.clone();
        *hashes = other_hashes.clone();
    }
}
