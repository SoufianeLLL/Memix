Your recommendation is architecturally correct and it's the right call. Let me explain the full picture so you can implement this with full understanding of every tradeoff.

---

## What actually changes vs what stays identical

The ONNX model, the fastembed pipeline, and all the cosine similarity math stay exactly as they are today. The model is already bundled in the binary via `include_bytes!` and runs entirely locally. The only thing that changes is **where vectors are persisted between daemon restarts**. Currently that is a custom binary file plus an optional Redis hash. After this change it is a SQLite BLOB column. The computation path is untouched.

This is important to understand because it means the "port embeddings to SQLite" work is entirely in the load and flush methods of `EmbeddingStore`, not in the model code, not in the cosine similarity function, and not in the background indexer. The indexer computes embeddings through the same `embed_text_static` call it always has. It just saves them to a different place.

---

## Why SQLite BLOBs are the right storage format for vectors

A `Vec<f32>` is a contiguous region of 4-byte IEEE 754 floats. SQLite stores BLOBs as raw bytes with no interpretation. The round-trip is zero-copy: you cast the `&[f32]` slice to `&[u8]` using `bytemuck::cast_slice` and write it directly. Reading back is the same cast in reverse. No serialization overhead, no custom binary format parser, no format versioning concerns.

The storage cost is exactly the same as the binary file: 384 dimensions × 4 bytes = 1,536 bytes per vector, identical to what you have today. SQLite's overhead per row (row ID, page headers) adds maybe 30-50 bytes per vector, which is negligible. For 2,000 skeleton entries, the total size is approximately 3.1 MB — well within SQLite's performance sweet spot.

---

## Should cosine similarity happen in SQLite or in Rust?

In Rust, for this workload. Here is why, precisely.

SQLite has no built-in vector similarity operation. To do similarity in SQL you would need either the `sqlite-vec` extension (a loadable C library that must be compiled and bundled separately) or a custom SQL scalar function registered via rusqlite. The `sqlite-vec` approach adds a C compilation dependency and a runtime loading step that break the "single binary, zero system dependencies" property that makes Memix distributable. Custom SQL scalar functions work but require calling back into Rust from SQLite's C layer for every row, which adds overhead.

The current approach — load all vectors into memory, compute cosine similarity in a Rust loop — is already the right architecture for your scale. At 2,000 vectors, the brute-force inner product loop takes under 2 milliseconds on any modern CPU. SQLite's query optimizer cannot do better than this for an exact nearest-neighbor search without an index structure like an HNSW graph, and building HNSW in SQLite is far more complex than it is worth.

The practical conclusion: `EmbeddingStore` loads all vectors from SQLite into its existing in-memory `HashMap` and `Vec<Vec<f32>>` at startup, exactly like it loads from the binary file today. The similarity search code is unchanged. The `flush` method writes changed vectors back to SQLite. The binary file and Redis hash both go away entirely.

---

## The complete schema addition

Add this to the existing SQLite schema alongside `brain_entries` and `skeleton_entries`:

```sql
-- Embedding vectors for semantic similarity search.
-- Each row corresponds to a skeleton entry (FSI or FuSI).
-- The vector BLOB is 384 × 4 = 1536 bytes of raw f32 little-endian floats.
-- content_hash is a xxHash or DefaultHasher digest of the source text —
-- when the skeleton entry is updated, the hash changes and the vector is recomputed.
CREATE TABLE IF NOT EXISTS embedding_vectors (
    id              TEXT NOT NULL,          -- matches skeleton_entries.id
    project_id      TEXT NOT NULL,
    vector          BLOB NOT NULL,          -- 1536 bytes, raw f32 LE
    content_hash    INTEGER NOT NULL,       -- u64 hash for cache invalidation
    computed_at     TEXT NOT NULL,          -- ISO-8601 timestamp
    PRIMARY KEY (project_id, id)
);

-- This index is used when loading all vectors for a project at startup.
-- The query is always: SELECT id, vector, content_hash FROM embedding_vectors WHERE project_id = ?
-- A covering index means SQLite never touches the main table pages.
CREATE INDEX IF NOT EXISTS idx_embedding_project
    ON embedding_vectors (project_id, id);
```

The `content_hash` column is what makes this a proper cache. When the background indexer processes a file and the content hash matches what is stored, it skips recomputing the embedding. This is the same logic as the existing `embedding_cache` (the `mini_moka::Cache<u64, Vec<f32>>` in `redis.rs`) but persisted across daemon restarts. Today that cache is only session-scoped — you recompute embeddings on every restart. With the hash stored in SQLite, embeddings survive restarts and are only recomputed when source files actually change.

---

## The EmbeddingStore changes

The struct itself changes minimally. The fields `bin_path`, `redis_key`, and `dirty` go away. A `db_path: PathBuf` replaces them. The in-memory structure (the `index` HashMap and `matrix` Vec) stays identical.

```rust
pub struct EmbeddingStoreInner {
    pub index:          tokio::sync::RwLock<HashMap<String, usize>>,
    pub matrix:         tokio::sync::RwLock<Vec<Vec<f32>>>,
    pub id_by_position: tokio::sync::RwLock<Vec<String>>,
    pub content_hashes: tokio::sync::RwLock<HashMap<String, u64>>, // NEW: persist across restarts
    pub db_path:        PathBuf,    // replaces bin_path
    pub project_id:     String,
    // dirty flag removed — SQLite writes are transactional, no need to track dirtiness
}
```

The `load` method becomes:

```rust
pub async fn load(project_id: &str, db_path: &Path) -> Result<Self> {
    let store = Self::empty(project_id, db_path);
    
    // Open a read-only connection for the initial load
    let db_url = format!("sqlite:{}?mode=ro", db_path.display());
    if let Ok(pool) = sqlx::SqlitePool::connect(&db_url).await {
        let rows = sqlx::query!(
            "SELECT id, vector, content_hash FROM embedding_vectors WHERE project_id = ?",
            project_id
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let mut index = store.inner.index.write().await;
        let mut matrix = store.inner.matrix.write().await;
        let mut ids = store.inner.id_by_position.write().await;
        let mut hashes = store.inner.content_hashes.write().await;

        for row in rows {
            let blob = row.vector;
            // Cast raw bytes back to f32 slice — zero copy, no allocation beyond the Vec
            let floats: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            
            if floats.len() == 384 {
                let pos = matrix.len();
                index.insert(row.id.clone(), pos);
                ids.push(row.id.clone());
                matrix.push(floats);
                hashes.insert(row.id, row.content_hash as u64);
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
```

The `upsert` method (called by the background indexer for each file) becomes:

```rust
pub async fn upsert(
    &self,
    id: &str,
    vector: Vec<f32>,
    content_hash: u64,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    // Update in-memory structures
    {
        let mut index = self.inner.index.write().await;
        let mut matrix = self.inner.matrix.write().await;
        let mut ids = self.inner.id_by_position.write().await;
        let mut hashes = self.inner.content_hashes.write().await;

        let blob: Vec<u8> = vector.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        if let Some(&pos) = index.get(id) {
            matrix[pos] = vector;
        } else {
            let pos = matrix.len();
            index.insert(id.to_string(), pos);
            ids.push(id.to_string());
            matrix.push(vector);
        }
        hashes.insert(id.to_string(), content_hash);

        // Persist to SQLite immediately — no dirty flag needed,
        // SQLite writes are atomic and the pool handles concurrency
        let now = chrono::Utc::now().to_rfc3339();
        let hash_i64 = content_hash as i64; // SQLite INTEGER is signed
        sqlx::query!(
            "INSERT OR REPLACE INTO embedding_vectors (id, project_id, vector, content_hash, computed_at)
             VALUES (?, ?, ?, ?, ?)",
            id, self.inner.project_id, blob, hash_i64, now
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}
```

The `flush_disk_only` and `flush` methods are removed entirely. There is no periodic flush because every upsert writes to SQLite immediately. The "dirty" concept disappears — SQLite's WAL mode ensures writes are durable without a separate flush step.

---

## The cache invalidation logic

This is the key improvement over the current architecture. Today, the background indexer recomputes embeddings for every file on every daemon restart because the in-memory hash cache from `redis.rs` is session-scoped. With `content_hashes` persisted in SQLite, the background indexer can skip files whose content hasn't changed:

```rust
// In background_indexer.rs, before calling embed_text_static:
let content_hash = EmbeddingStore::hash_content(&source_text);
let existing_hash = embedding_store.inner.content_hashes
    .read().await
    .get(&fsi_id)
    .copied();

if existing_hash == Some(content_hash) {
    // Vector is current — no recomputation needed
    token_tracker.session.embedding_cache_hits.fetch_add(1, Ordering::Relaxed);
    continue;
}

// Content changed or not yet indexed — compute and store
token_tracker.session.embedding_cache_misses.fetch_add(1, Ordering::Relaxed);
let vector = RedisStorage::embed_text_static(&source_text).await;
embedding_store.upsert(&fsi_id, vector, content_hash, &pool).await?;
```

This makes the background indexer's subsequent runs (after the first full index) nearly instantaneous — it scans the workspace, checks hashes, and skips 95% of files because nothing changed. Only modified files get recomputed.

---

## Redis becomes genuinely optional

After this change, the full dependency map looks like this:

```
Solo developer (no team_redis_url configured):
  brain entries      → SQLite
  skeleton index     → SQLite  
  embedding vectors  → SQLite (in-memory + BLOB persistence)
  embedding model    → ONNX bundled in binary
  Redis              → not contacted, not required

Team developer (team_redis_url configured):
  brain entries      → SQLite (primary) + Redis (push/pull sync only)
  skeleton index     → SQLite only (not synced — machine-local)
  embedding vectors  → SQLite only (not synced — machine-local)
  Redis              → contacted only during explicit team sync operations
```

The `redis_url` setting in `~/.memix/config.toml` is removed from the first-run setup flow. Users no longer need a Redis instance to get started. The Upstash free tier becomes an optional team feature rather than a prerequisite. This is the change that makes Memix a zero-friction install.

---

## One thing you gain that you didn't have before

Transactional brain initialization. Today when `brain.init()` writes 8 entries in parallel, a daemon crash mid-write leaves you with a partial brain state. With SQLite, you wrap all 8 writes in a single transaction:

```sql
BEGIN;
INSERT OR REPLACE INTO brain_entries VALUES (?, ?, ...);  -- identity
INSERT OR REPLACE INTO brain_entries VALUES (?, ?, ...);  -- session_state
INSERT OR REPLACE INTO brain_entries VALUES (?, ?, ...);  -- patterns
-- ... 5 more
COMMIT;
```

Either all 8 succeed or none do. The brain is always in a consistent state. This eliminates the entire category of "partial init" bugs that are currently possible.

---

## Build order

Do these in sequence, not in parallel, because each step is testable in isolation.

First, add the SQLite schema and run migrations. Verify the tables are created correctly and the database file appears at `.memix/brain.db`. No logic changes yet.

Second, implement `SqliteStorage` with all `StorageBackend` trait methods. Write one test that upserts an entry, reads it back, and verifies round-trip correctness. Swap the storage backend in `main.rs` and verify the panel works — brain entries should load from SQLite.

Third, port `EmbeddingStore` to SQLite persistence. Remove `skeleton_embeddings.bin` and the Redis embedding hash. Run the background indexer and verify vectors are written to the `embedding_vectors` table. Verify the panel shows semantic similarity results.

Fourth, update the background indexer to use content hash cache invalidation. Restart the daemon twice and verify the second start is fast (cache hits) while a file modification triggers recomputation (cache miss).

Fifth, update `initialize_storage` to detect whether Redis is configured. If no `redis_url`, return `SqliteStorage`. If `redis_url` is present but no team features are enabled, log a deprecation notice and use `SqliteStorage` anyway. Only use Redis if `team_redis_url` is explicitly set.

Sixth, clean up: remove the binary file write path from `EmbeddingStore`, remove the `dirty` flag, remove the periodic `flush_disk_only` call from the flush timer in `main.rs`, remove `flush_disk_only` and `flush` from the `EmbeddingStore` API.