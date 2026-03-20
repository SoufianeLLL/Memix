# Daemon Development Guide

## Overview

This guide covers developing and running the Memix daemon (`memix-daemon`) locally for extension development and testing.

## Prerequisites

- **Rust** (latest stable, via rustup)
- **Redis** (local or cloud — e.g., Upstash)
- **VS Code** with the Memix extension

## Running the Daemon

### Standalone Mode (for development)
Set these environment variables and run directly:

```bash
export MEMIX_WORKSPACE_ROOT="/path/to/your/project"
export MEMIX_DEV_EXTERNAL_DAEMON=true
export MEMIX_REDIS_URL="redis://default:password@host:port"

cd vs-extension/daemon
cargo run
```

Setting `MEMIX_DEV_EXTERNAL_DAEMON=true` tells the VS Code extension to connect to your externally-running daemon instead of spawning its own. This prevents the "Daemon already running" error during development.

### With the Extension
The extension normally manages the daemon lifecycle automatically — spawning it on activation and connecting via Unix domain socket (`~/.memix/daemon.sock`).

## Configuration

Configuration is loaded in this priority order:
1. Environment variables (`MEMIX_REDIS_URL`, `MEMIX_WORKSPACE_ROOT`)
2. `config.toml` in the data directory
3. Built-in defaults

### Key Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MEMIX_REDIS_URL` | Redis connection string | `redis://127.0.0.1/` |
| `MEMIX_WORKSPACE_ROOT` | Workspace root for file watching | Current directory |
| `MEMIX_DEV_EXTERNAL_DAEMON` | Use external daemon (dev mode) | `false` |

## Project Structure

```
daemon/
├── src/
│   ├── main.rs          # Entry point, event loop, file watcher
│   ├── server.rs         # HTTP API (Axum)
│   ├── config.rs         # Configuration loading
│   ├── brain/
│   │   └── schema.rs     # MemoryEntry, MemoryKind, etc.
│   ├── observer/
│   │   ├── parser.rs     # AST parsing (tree-sitter)
│   │   ├── graph.rs      # Dependency graph
│   │   ├── dna.rs        # Project code DNA (patterns)
│   │   ├── skeleton.rs   # File Skeleton Index
│   │   └── call_graph.rs # Function call graph
│   ├── storage/
│   │   ├── mod.rs        # StorageBackend trait
│   │   └── redis.rs      # Redis implementation
│   ├── context/
│   │   └── mod.rs        # Context Compiler (7-pass pipeline)
│   ├── intelligence/
│   │   └── intent.rs     # Intent classification
│   └── ...
├── Cargo.toml
└── models/               # Bundled embedding model
```

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test observer::skeleton::tests::test_file_skeleton_build
```

## Building

```bash
# Debug build
cargo build

# Release build (for .vsix packaging)
cargo build --release
```

## Common Issues

### "Daemon already running"
Set `MEMIX_DEV_EXTERNAL_DAEMON=true` when developing to prevent the extension from spawning a competing daemon instance.

### Redis connection limit reached
The daemon uses `ConnectionManager` for multiplexed connections. If you see connection limit errors, verify no other processes are holding Redis connections.
