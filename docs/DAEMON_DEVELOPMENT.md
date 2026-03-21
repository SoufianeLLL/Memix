# Daemon Development Guide

## Overview

This guide covers developing and running the Memix daemon (`memix-daemon`) locally for extension development and testing. The daemon is a Rust binary that runs continuously alongside the IDE, observing the workspace through multiple analysis layers and serving structural intelligence to the VS Code extension over a Unix domain socket (macOS/Linux) or TCP (Windows).

## Prerequisites

Rust (latest stable, via rustup), Redis (local or cloud — Upstash works well for development), and VS Code or Cursor with the Memix extension installed as a `.vsix`.

The embedding model is bundled into the binary at compile time. Before the first build, download it once by running `bash scripts/download_model.sh` from inside the `daemon/` directory.

## Running the Daemon

### Standalone Mode (development)

Set these environment variables and run directly:

```bash
export MEMIX_WORKSPACE_ROOT="/path/to/your/project"
export MEMIX_DEV_EXTERNAL_DAEMON=true
export MEMIX_REDIS_URL="redis://default:password@host:port"

cd daemon
cargo run
```

Setting `MEMIX_DEV_EXTERNAL_DAEMON=true` tells the extension to connect to your externally-running daemon instead of spawning its own. Without this flag, the extension will spawn a separate daemon instance, and the two will collide on the socket or PID file.

### With the Extension

The extension normally manages the daemon lifecycle automatically — downloading the binary if absent, verifying its SHA-256 checksum, spawning it with the workspace root and project ID as environment variables, and connecting via the Unix domain socket at `~/.memix/daemon.sock`.

## Configuration

Configuration is loaded in this priority order, with later sources overriding earlier ones: built-in defaults, then `~/.memix/config.toml`, then environment variables with the `MEMIX_` prefix.

| Variable | Default | Description |
|---|---|---|
| `MEMIX_REDIS_URL` | `redis://127.0.0.1/` | Redis connection string |
| `MEMIX_WORKSPACE_ROOT` | Current directory | Root directory to watch and index |
| `MEMIX_PROJECT_ID` | `default` | Project identifier for Redis key namespacing |
| `MEMIX_DEV_EXTERNAL_DAEMON` | `false` | Skip spawning; connect to externally-run daemon |
| `MEMIX_OXC_ENABLED` | `true` | Enable OXC semantic enrichment for TS/JS files |
| `MEMIX_OXC_RESOLVE_NODE_MODULES` | `false` | Include node_modules in import resolution |
| `MEMIX_FORCE_REINDEX` | `false` | Force full workspace rescan on next start |
| `MEMIX_INDEXER_RATE_LIMIT` | `10` | Background indexer files-per-second throttle |

## Project Structure

```
daemon/
├── src/
│   ├── main.rs                  # Entry point, startup sequence, event loop
│   ├── server.rs                # HTTP API (Axum), AppState definition
│   ├── config.rs                # Configuration loading and normalization
│   ├── brain/
│   │   └── schema.rs            # MemoryEntry, MemoryKind, MemorySource
│   ├── context/
│   │   └── mod.rs               # Context Compiler (7-pass pipeline)
│   ├── observer/
│   │   ├── mod.rs               # Module declarations
│   │   ├── parser.rs            # AST parsing (tree-sitter, all languages)
│   │   ├── differ.rs            # Semantic diff between AST versions
│   │   ├── graph.rs             # Dependency graph with petgraph integration
│   │   ├── importance.rs        # Betweenness, PageRank, blast radius
│   │   ├── call_graph.rs        # Resolved call graph with dual-index
│   │   ├── skeleton.rs          # FSI + FuSI entry builders
│   │   ├── embedding_store.rs   # Write-through embedding persistence
│   │   ├── background_indexer.rs # Startup workspace scan
│   │   ├── oxc_semantic.rs      # OXC Layer 2 analysis for TS/JS
│   │   ├── dna.rs               # Project Code DNA
│   │   ├── imports.rs           # Import extraction and signature helpers
│   │   └── watcher.rs           # File system event subscription
│   ├── storage/
│   │   ├── mod.rs               # StorageBackend trait
│   │   └── redis.rs             # Redis + JSON mirror implementation
│   ├── token/
│   │   ├── engine.rs            # Token counting (tiktoken-rs)
│   │   └── tracker.rs           # Session + lifetime token accounting
│   ├── intelligence/
│   │   ├── intent.rs            # Developer intent classification
│   │   ├── predictor.rs         # Context pre-loading predictor
│   │   └── autonomous.rs        # Autonomous pair programmer
│   ├── git/
│   │   └── archaeologist.rs     # Git history and churn analysis
│   ├── agents/
│   │   └── mod.rs               # Background agent runtime
│   ├── learning/
│   │   └── mod.rs               # Prompt outcome recording and optimization
│   ├── license/
│   │   └── mod.rs               # Ed25519 license validation
│   └── ...
├── scripts/
│   └── download_model.sh        # Downloads AllMiniLM-L6-v2 for bundling
├── models/                      # Downloaded model files (git-ignored)
├── keys/                        # Ed25519 public key for license validation
└── Cargo.toml
```

## Startup Sequence

Understanding the daemon's startup order helps when debugging why certain features aren't immediately available after launch.

First, configuration loads and Redis connects. Then `EmbeddingStore` loads from `.memix/skeleton_embeddings.bin` (falling back to Redis if the file is absent), and `TokenTracker` loads lifetime totals from `.memix/token_lifetime.json`. Migrations run for the configured project ID.

The Axum router is built with all state attached to `AppState`, then the Unix socket is bound and the accept loop is spawned — at this point the extension's health check can succeed. TCP is also bound for development and Windows compatibility.

Five seconds after startup, the `BackgroundIndexer` spawns. If the embedding store is empty (first run), it walks the workspace at the configured rate limit, building FSI entries and embeddings for every source file it finds. A periodic flush task runs every 5 minutes to persist session token totals and dirty embedding state.

The file watcher goroutine starts processing events — save events trigger the full Layer 1 → Layer 2 → Layer 3 pipeline for the saved file, while delete events trigger graph cleanup.

## Key Architectural Facts for Contributors

The call graph is an `Arc<Mutex<CallGraph>>` in `AppState`, shared between the event loop (which writes to it on file saves) and HTTP handlers (which read from it for causal context queries). Write lock contention should be low because file saves are serialized through the single event channel.

The `EmbeddingStore` uses async `RwLock`s internally, so it is safe to call `search` from HTTP handlers while the background indexer is simultaneously calling `upsert` during startup. The search will briefly block on the read lock, which is held only for the duration of the matrix read — typically microseconds.

The `TokenTracker`'s `session` field uses `AtomicU64` counters with `Ordering::Relaxed` semantics. This is intentional: the counters are updated from multiple concurrent tasks and absolute ordering between updates is not required. The important property is that each counter is individually consistent (no torn reads or writes), not that updates across multiple counters appear in a specific order.

The `naive_token_estimate` field in `CompiledContext` is computed during Pass 1 of the context compiler by summing the byte sizes of all relevant files multiplied by 0.25. This estimate feeds the `estimated_tokens_saved` counter in `SessionCounters`, which is the difference between the naive estimate and the actual compiled token count. It is an approximation, not an exact measurement, but it is computed consistently so the trend is accurate even if the absolute numbers are slightly off.

## Testing

```bash
# Run all tests from the daemon directory
cargo test

# Run with test output visible
cargo test -- --nocapture

# Run tests for a specific module
cargo test observer::importance::tests
cargo test observer::skeleton::tests
cargo test observer::call_graph::tests
```

All major modules have unit test suites. The most important ones to run after changes are `importance`, `skeleton`, and `call_graph` — these cover the correctness properties of the structural analysis algorithms.

## Building

```bash
# Debug build (fast compile, slow runtime)
cargo build

# Release build (slow compile, fast runtime — use for .vsix packaging)
cargo build --release

# Cross-compile for Linux musl (requires the model to be in daemon/models/)
cargo build --release --target x86_64-unknown-linux-musl
```

## Common Issues

**"Daemon already running"** — Set `MEMIX_DEV_EXTERNAL_DAEMON=true` in your environment before launching the extension. This prevents the extension from spawning a competing daemon instance that would collide with your development daemon on the socket or PID lock file.

**Redis connection limit reached** — The daemon uses `ConnectionManager` for multiplexed connections. If you see connection limit errors, verify no other processes are holding Redis connections open. In development, `redis-cli client list` shows all current connections.

**Observer Code DNA shows all zeros** — The observer only processes files that are saved while the daemon is running. Open any source file in your workspace and press Cmd+S (or Ctrl+S) to trigger the first processing event. Code DNA, intent, and the dependency graph all populate from file-save events.

**Embedding store empty after restart** — Check that `~/.memix` (or your `MEMIX_DATA_DIR`) is writable and that `.memix/skeleton_embeddings.bin` exists in the project's data directory. If it doesn't, the background indexer will rebuild it 5 seconds after startup.

**OXC import resolution failures** — Relative imports that can't be resolved generate `dead-import` warning entries in the brain. This is expected when OXC can't find a file that has been moved or deleted. The daemon continues processing the file through tree-sitter regardless. Set `RUST_LOG=trace` to see detailed OXC resolution logs.