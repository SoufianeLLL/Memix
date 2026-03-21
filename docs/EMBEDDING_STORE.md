# Embedding Store

## Overview

The **Embedding Store** manages the lifecycle of 384-dimensional vector embeddings for every skeleton index entry in the project. It provides semantic similarity search across the entire structural index, and uses a write-through hybrid architecture — a local binary file for speed, Redis for cross-IDE sharing — that makes multi-IDE support transparent without requiring any architectural changes when that feature is built.

## Why Embeddings on Skeleton Entries

The skeleton index stores structural summaries of every source file and hot function as text: things like function signatures, dependency relationships, and complexity metrics. Text can be searched by keyword, but keyword matching fails for semantically related concepts that don't share vocabulary. A question about "token validation" won't keyword-match a function called `verifyJwt` unless the skeleton entry happens to contain the word "token." Embeddings solve this by representing the meaning of each entry as a point in 384-dimensional vector space, where semantically related entries cluster together regardless of exact word choice.

The AllMiniLM-L6-v2 model was chosen for its balance of quality and speed. Its 384 output dimensions are small enough that a project with 2,000 indexed entries requires only about 3MB of storage and can be exhaustively searched in under 1ms. It runs entirely on CPU — no GPU requirement, no cloud API, no network access. The model weights are compiled directly into the daemon binary via `include_bytes!` at build time.

## Storage Architecture

Two storage tiers work together, with writes going to both simultaneously and reads preferring the faster tier.

The **local binary file** at `.memix/skeleton_embeddings.bin` is the primary read path. Its format is simple and fast to load: a 4-byte little-endian count header followed by fixed-size records of 1,664 bytes each (128 bytes for the null-padded entry ID plus 1,536 bytes for the 384 × 4-byte float vector). Loading the full file for 2,000 entries takes approximately 3ms — a single sequential read with no parsing overhead. Writes use an atomic rename pattern: the new content is written to a `.bin.tmp` file first, then renamed over the production file. A crash during write leaves the old file intact.

The **Redis hash** at `embeddings:{project_id}` stores the same data as binary-encoded float arrays in a hash field per entry ID. It serves two purposes: it survives daemon restarts independently of the local file (though the local file does too), and it acts as the synchronization point for any future IDE instance that starts on the same project without a local binary file. When a second IDE instance starts cold and finds no local file, it loads embeddings from Redis in a few milliseconds and then writes them locally, so subsequent restarts on that machine are fast again.

Redis writes are always asynchronous — they happen in a spawned task and never block the main processing path. The local disk write is synchronous but uses the atomic rename pattern so it completes very quickly (no fsync). A `dirty` atomic boolean tracks whether the in-memory state has unsaved changes; the flush timer checks this before doing any I/O and returns immediately if nothing has changed.

## In-Memory Layout

The store maintains three synchronized structures in memory. The `matrix` is a `Vec<Vec<f32>>` — a row-per-entry embedding matrix where each row is one entry's 384-dimensional vector. The `index` is a `HashMap<String, usize>` mapping entry IDs to their row position in the matrix. The `id_by_position` is a `Vec<String>` providing the reverse mapping.

This three-structure layout supports O(1) lookup by ID, O(1) random access to any vector by position, and O(N×D) exhaustive search where N is the number of entries and D is 384. The search itself normalizes the query vector once and then computes dot products against each row — equivalent to cosine similarity when both vectors are normalized. A partial sort using `sort_unstable_by` extracts the top-k results in O(N log k) rather than O(N log N).

Upserts first check whether the entry ID already exists in the index. If it does, the vector is updated in place at the existing matrix row. If not, the entry is appended and the index is updated. Removals use a swap-remove for O(1) deletion: the entry to be removed is swapped with the last entry, the last row is truncated, and the swapped entry's index position is updated. All three operations are guarded by async `RwLock`s — separate locks for the matrix and for the index/id structures would deadlock under concurrent access, so a single conceptual write lock covers all three structures during mutation.

## Similarity Search

The `search` method accepts a query vector and a `top_k` parameter and returns a sorted list of `(entry_id, similarity_score)` pairs. The similarity score is cosine similarity in [0, 1], where 1.0 means identical meaning and 0.0 means orthogonal concepts.

For the expected scale of a developer's project (hundreds to low thousands of indexed files), brute-force search is the correct algorithm. Approximate nearest-neighbor indexes like HNSW become beneficial at hundreds of thousands of entries — far beyond what any single developer project's skeleton index would contain. The brute-force approach also has no index-building step, meaning newly upserted entries are immediately searchable without any index update overhead.

## Flush Behavior

The `flush` method writes dirty in-memory state to both tiers simultaneously. It first writes the binary file atomically, then spawns an async task for the Redis write. The flush is called on a 30-second timer in the daemon's main task and also during graceful shutdown. The background indexer calls flush explicitly after completing its full workspace scan.

## Key File

`daemon/src/observer/embedding_store.rs` contains the complete implementation including the binary format specification, the write-through flush logic, the cosine similarity search, and the load path that falls back from disk to Redis.