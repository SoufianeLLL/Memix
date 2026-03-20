# Code Skeleton Index

## Overview

The **Code Skeleton Index** is a three-layer structural intelligence system that gives AI agents an architectural understanding of the codebase — even for files they haven't recently seen. Instead of embedding full file contents into context, the skeleton index provides lightweight summaries that capture the _shape_ of the code.

## Problem & Solution

### Problem
AI agents lose structural awareness of files outside the active editing window. When you save file A, the agent forgets the layout of files B, C, and D. RAG memory retrieves _topically related_ entries, but doesn't capture _structural relationships_ like call chains and dependency patterns.

### Solution
The Code Skeleton Index introduces two layers of structural data:

1. **File Skeleton Index (FSI)**: One entry per file—captures exports, imports, function count, average complexity, and dependency counts.
2. **Function Symbol Index (FuSI)**: One entry per function (for "hot" files)—captures function name, kind, line count, complexity, exported status, and call targets.

These entries are stored in a **separate Redis hash** (`{project_id}_skeletons`) to avoid competing with brain entries for the 1,000-entry brain cap.

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────────┐
│  File Save   │────▶│  AST Parser │────▶│  FileSkeleton    │
│  Event       │     │  (features) │     │  Builder         │
└─────────────┘     └─────────────┘     └────────┬────────┘
                                                  │
                           ┌──────────────────────┴──────────────┐
                           │                                      │
                    ┌──────▼──────┐                       ┌──────▼──────┐
                    │  FSI Entry   │                       │  FuSI Entries │
                    │  (1 per file)│                       │  (1 per fn)   │
                    └──────┬──────┘                       └──────┬──────┘
                           │                                      │
                    ┌──────▼──────────────────────────────────────▼──────┐
                    │         Redis Hash: {project_id}_skeletons         │
                    │         Capacity: 2,000 entries (LRU eviction)      │
                    └────────────────────────────────────────────────────┘
```

## How It Works

### On File Save (event loop in `main.rs`)
1. AST parser extracts features including `calls` and `line_count`
2. **CallGraph** is updated with the file's call sites
3. **FSI entry** is built and persisted (always, debounced 1s)
4. **FuSI entries** are built and persisted only if file is "hot"

### Hot File Detection (`is_hot_file()`)
A file is "hot" if:
- It has been recently parsed (in `feature_snapshots`) and the total snapshot count is ≤ 30
- It has 3+ dependents (high fan-in)

### On File Deletion
1. CallGraph entries for the file are removed
2. FSI entry is deleted from Redis
3. All FuSI entries with the file's prefix are deleted

### Context Compilation
When the context compiler runs, skeleton entries from the Redis hash are fetched and injected as additional sections:
- **FSI sections**: priority 85 (higher than code-skeleton, lower than active file)
- **FuSI sections**: priority 78 (above history, below FSI)

## Key Files

| File | Purpose |
|------|---------|
| `observer/skeleton.rs` | `FileSkeleton`, `FunctionShape`, ID helpers, MemoryEntry conversion |
| `observer/call_graph.rs` | In-memory `CallGraph` for tracking function call relationships |
| `observer/parser.rs` | Extended `AstNodeFeature` with `calls` and `line_count` |
| `storage/redis.rs` | Isolated skeleton Redis hash with 2,000-entry cap + LRU |
| `storage/mod.rs` | `StorageBackend` trait with skeleton method defaults |
| `context/mod.rs` | `compile()` with skeleton entry injection |
| `main.rs` | Event loop wiring, `is_hot_file()` helper |
| `server.rs` | `GET /api/v1/skeleton/stats/:project_id` endpoint |

## API Endpoints

### `GET /api/v1/skeleton/stats/:project_id`
Returns skeleton index statistics:
```json
{
  "project_id": "my-project",
  "fsi_count": 42,
  "fusi_count": 128,
  "total": 170,
  "capacity": 2000
}
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `MAX_SKELETON_ENTRIES` | 2,000 | Max entries in the skeleton Redis hash |
| `MAX_HOT_FILES` | 30 | Max files eligible for FuSI generation |
| `FSI_DEBOUNCE_SECS` | 1 | Minimum interval between FSI persists |

## Entry ID Format

- **FSI**: `fsi::{normalized_path}` (e.g., `fsi::src/lib/auth.ts`)
- **FuSI**: `fusi::{normalized_path}::{symbol_name}::{kind}` (e.g., `fusi::src/lib/auth.ts::validateToken::function`)

Path normalization strips the workspace root prefix, collapses `//` to `/`, and removes leading `/`.
