# Redis Connection Pooling and Usage

## Overview

The daemon uses a **single persistent multiplexed Redis connection** via `redis::aio::ConnectionManager` for all brain and skeleton storage operations. This prevents connection exhaustion on cloud Redis providers that enforce strict per-account connection limits. A second dedicated async connection is used by the `EmbeddingStore` for its write-through sync path, keeping embedding persistence independent from the main storage operations.

## Problem

Early versions of the daemon created a new Redis connection for each storage operation. With many concurrent async tasks — file saves triggering skeleton updates, context compilations querying brain entries, background indexer writing embeddings, and the pending.json poller running every 15 seconds — the connection count grew rapidly under normal developer load.

Cloud Redis providers such as Upstash and Redis Cloud impose strict connection limits, often 100 or 300 on free and entry-level tiers. Hitting the limit caused `ERR max number of clients reached` errors on random operations, making the daemon appear unreliable even when Redis itself was healthy.

## Connection Strategy

The main storage connection in `RedisStorage` uses `redis::aio::ConnectionManager`, which multiplexes all async requests over a single persistent TCP connection. From Redis's perspective, the entire daemon looks like one client regardless of how many concurrent tasks are running. The connection is established lazily on first use and reconnects automatically if the underlying TCP connection drops — no manual reconnection logic is required.

The `EmbeddingStore` uses a separate multiplexed connection obtained from the Redis client when performing its write-through flushes. This separation is intentional: embedding flushes are large batch writes that happen on a 30-second timer and could create backpressure on the main connection if interleaved with frequent brain reads and writes. Keeping them on a separate connection means a slow flush (caused by writing thousands of embedding vectors atomically) does not delay brain operations.

Both connections clone as an `Arc` clone internally — there is no heap allocation per operation — and both are safe to use from multiple concurrent async tasks without external synchronization.

## Redis Key Namespacing

The daemon uses a consistent key naming scheme to avoid collisions between different data types and projects:

- Brain entries live in a Redis hash at `{project_id}` (e.g., `my-project`).
- Skeleton index entries live in a separate Redis hash at `{project_id}_skeletons`.
- Skeleton embeddings live in a Redis hash at `embeddings:{project_id}`.
- License cache lives at `{project_id}_license_cache` if stored in Redis.
- CRDT operation logs use `{project_id}_crdt_ops`.

This namespacing means multiple projects can share the same Redis instance without any risk of key collision, and each data type can be inspected or cleared independently.

## Skeleton Hash Capacity

The `{project_id}_skeletons` hash is capped at 2,000 entries with LRU eviction. This bound prevents unbounded growth in large projects and keeps query performance stable. The cap is intentionally separate from the main brain hash's 1,000-entry cap — skeleton entries are structurally different from brain entries (they are generated automatically from code, not written by AI agents) and have different retention semantics.

When the 2,000-entry cap is reached, the least recently accessed skeleton entries are evicted to make room for new ones. Hot files (recently saved or high-dependency) are naturally retained because they are accessed frequently. Rarely touched legacy files may be evicted but will be reindexed the next time they are saved.

## Embedding Hash Capacity

The `embeddings:{project_id}` Redis hash has no explicit server-side eviction policy. The in-memory embedding store's size is bounded by the number of skeleton entries (which is in turn bounded by the 2,000-entry skeleton cap), so the Redis hash grows to at most a few megabytes — well within the memory budget of any Redis tier that can handle the rest of Memix's operations.

The Redis embedding hash is a write-through mirror of the local `.memix/skeleton_embeddings.bin` file and can be deleted and rebuilt from the binary file without any data loss. It is consulted during daemon startup only when the binary file is absent, making it primarily a cross-IDE sharing mechanism rather than the primary storage path.

## Cargo.toml Dependencies

The `connection-manager` and `tokio-rustls-comp` features are required:

```toml
redis = { version = "0.28", features = ["tokio-rustls-comp", "connection-manager"] }
```

The `tokio-rustls-comp` feature provides TLS support via rustls (pure Rust, no system OpenSSL dependency) which is required for cloud Redis URLs using `rediss://` (TLS-encrypted connections) and ensures the daemon cross-compiles to musl Linux targets without OpenSSL system library dependencies.

## Key File

`daemon/src/storage/redis.rs` — the `RedisStorage` struct, `get_conn()` method, skeleton hash operations, and JSON mirror write path.