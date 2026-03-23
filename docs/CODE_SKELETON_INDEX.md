# Code Skeleton Index

## Overview

The **Code Skeleton Index** is a three-layer structural intelligence system that gives AI agents an architectural understanding of the entire codebase — even for files they haven't recently seen and without dumping raw file content into context. It provides lightweight structural summaries that capture the shape, relationships, and semantics of every source file, updated continuously as code changes.

## The Three Layers

The index is built through three successive passes that each add a different kind of understanding. Each layer runs independently — they are not sequential gates — and the combination of all three gives richer structural data than any single pass could provide.

**Layer 1 — Structural tree (tree-sitter)** processes every supported source file on save, producing an AST and extracting `AstNodeFeature` values: function names, kinds, visibility, cyclomatic complexity, pattern tags, export status, call sites, and line counts. This layer runs on every file save, is deterministic, and completes in under 5ms per file. It supports TypeScript, JavaScript, Rust, Python, Go, Java, Kotlin, Swift, C#, C++, Ruby, and PHP.

**Layer 2 — Semantic enrichment (OXC)** runs immediately after Layer 1 for TypeScript and JavaScript files. Where tree-sitter produces a syntax tree, OXC produces a semantic model with full scope analysis and resolved imports. Specifically, OXC resolves each import statement to the actual file path it maps to (following `tsconfig.json` paths, barrel file re-exports, and `node_modules` resolution). Calls to imported symbols can then be represented as `ResolvedEdge` values with a concrete `callee_file` and `callee_line`, turning a nominal call graph into a resolved one. OXC also detects unresolved relative imports — dead imports that point to files that no longer exist — and creates warning entries for them.

**Layer 3 — Semantic similarity (AllMiniLM-L6-v2)** computes a 384-dimensional embedding vector for each skeleton entry's rendered content. These embeddings are what make skeleton entries searchable by semantic similarity rather than only by exact ID. When the context compiler needs to find "which files are structurally relevant to this question about authentication", it queries the embedding store rather than doing keyword matching. The embedding model is bundled directly into the daemon binary at compile time — no network access, no first-run download. Embeddings are computed once per file and cached; only files that actually change trigger recomputation.

## Background Indexer

The initial index build is handled by the `BackgroundIndexer`, which is conceptually identical to TypeScript's language server indexing behavior: it starts automatically 5 seconds after the daemon launches, processes the workspace at a rate-limited pace (default 10 files per second to avoid competing with the developer's active work), and populates the full FSI + embedding store before any context compilation requests arrive.

The indexer only runs when the embedding store is empty — which happens on first project open or after a fresh install. On subsequent daemon restarts, the embedding store is loaded from the hybrid disk-and-Redis persistence layer and the indexer skips itself immediately. The `MEMIX_FORCE_REINDEX` environment variable forces a full rescan regardless of existing state, which is useful during development.

Progress is tracked through the `TokenTracker`: the `files_skeleton_indexed` counter increments with each file the background indexer processes, making the indexing progress visible in the Token Intelligence panel.

## Embedding Storage: Write-Through Hybrid

Skeleton embeddings are stored using a write-through architecture that keeps a local binary file as the primary fast read path and Redis as the authoritative source for cross-IDE sharing.

The local file at `.memix/skeleton_embeddings.bin` uses a fixed-size record format: 128 bytes for the entry ID (null-padded) followed by 384 × 4 bytes for the float vector, giving 1,664 bytes per record. A 4-byte little-endian count header precedes all records. This format allows the entire store to be loaded with a single sequential read at startup — approximately 3ms for 2,000 entries. Writes use an atomic temp-file-then-rename pattern to prevent corruption from mid-write crashes.

The Redis hash at `embeddings:{project_id}` mirrors the same data. It is written asynchronously in a spawned task after each disk flush, so it never blocks the main processing path. When a second IDE instance (or a new machine) starts the daemon for the same project and finds no local binary file, it loads embeddings from Redis and then writes the binary file locally, eliminating the cold start problem for subsequent sessions on that machine.

A periodic flush task runs every 30 seconds, writing dirty in-memory state to disk and Redis simultaneously. The `dirty` flag uses an atomic boolean — it is set on every upsert and cleared after a successful flush — so flushes that find nothing changed return immediately without doing any I/O.

## File Skeleton Index (FSI)

One FSI entry exists per source file and is always updated when the file is saved (subject to a 1-second debounce to collapse rapid repeated saves). The entry captures the file's language, all type and class names, all function signatures with visibility and async status, the exports list, the imports list, dependency relationships from the graph, and average cyclomatic complexity. The rendered entry is intentionally compact — typically 10 to 20 lines — so many FSI entries can fit within a modest token budget.

Entry IDs follow the format `fsi::{normalized_path}` where normalization replaces backslashes with forward slashes and strips leading `./` prefixes, ensuring consistent IDs across operating systems.

## Function Symbol Index (FuSI)

FuSI entries are generated only for "hot" files — files that are either recently changed or have three or more dependents (high fan-in in the dependency graph). This constraint caps the total FuSI entry count and ensures that the per-function detail is concentrated where it matters most: the active development frontier.

Each FuSI entry covers one function or method and captures its name, kind, visibility, async status, cyclomatic complexity, line count, call targets (from the call graph), callers (from the reverse call graph), and pattern tags. The parent FSI entry is referenced via `parent_id`, creating a hierarchical relationship between file-level and function-level structural data.

Entry IDs follow the format `fusi::{normalized_path}::{symbol_name}::{kind}`. Symbol names containing `::` (Rust path separators) are sanitized by replacing `::` with `_` to prevent key injection.

## Lifecycle Events

When a file is saved, the event loop builds the FSI entry, updates the call graph for that file's symbols, persists the FSI entry to the skeleton Redis hash, and conditionally generates FuSI entries if the file is hot. The embedding for the FSI entry is computed and upserted into the embedding store, marking it dirty for the next flush.

When a file is deleted, the call graph removes all edges associated with that file, the FSI entry is deleted from the skeleton Redis hash by exact ID, and all FuSI entries with the file's normalized path as a prefix are deleted via a prefix scan of the skeleton hash.

## Context Compiler Integration

The context compiler receives skeleton entries as a separate input alongside brain entries. FSI entries are injected as sections with priority 85 and FuSI entries with priority 78. Both ranks are above dependency code-skeleton sections (priority 72) but below the active file's code skeleton (priority 95), ensuring structural index data always makes it into the compiled context before lower-value sections compete for budget.

## Key Files

`daemon/src/observer/skeleton.rs` contains `FileSkeleton`, `FunctionShape`, all ID helper functions, the `to_memory_entry` and `to_symbol_entries` conversion methods, and the `detect_language_from_path` fallback.

`daemon/src/observer/embedding_store.rs` contains the `EmbeddingStore` with its write-through hybrid storage, the binary file format implementation, and the semantic similarity search method.

`daemon/src/observer/background_indexer.rs` contains the `BackgroundIndexer` including the workspace file walker, the throttle logic, and the rate limit configuration.

`daemon/src/observer/oxc_semantic.rs` contains the OXC-based semantic analysis pass including import resolution, call extraction, and the `resolve_symbol_line` helper that maps imported symbols back to their declaration line numbers.

`daemon/src/storage/redis.rs` contains the `upsert_skeleton_entry`, `get_skeleton_entries`, and `delete_skeleton_entry` methods operating on the isolated `{project_id}_skeletons` Redis hash with its 2,000-entry LRU cap.

## Configuration

| Variable | Default | Description |
|---|---|---|
| `MEMIX_INDEXER_RATE_LIMIT` | 10 | Files per second during background indexing |
| `MEMIX_FORCE_REINDEX` | false | Force full workspace rescan on next daemon start |
| `MEMIX_OXC_ENABLED` | true | Enable OXC Layer 2 enrichment for TS/JS files |
| `MEMIX_OXC_RESOLVE_NODE_MODULES` | false | Include node_modules in OXC import resolution |
| `MEMIX_MAX_FUNCTIONS_PER_FILE` | 50 | Cap on functions per FSI entry |
| `MEMIX_MAX_TYPES_PER_FILE` | 30 | Cap on types per FSI entry |
| `MEMIX_MAX_IMPORTS_PER_FILE` | 20 | Cap on imports per FSI entry |
| `MEMIX_MAX_DEPS_PER_FILE` | 20 | Cap on dependency edges per FSI entry |
| `MEMIX_MAX_SYMBOLS_PER_HOT_FILE` | 50 | Cap on FuSI entries per hot file |

## API Endpoint

`GET /api/v1/skeleton/stats/:project_id` returns the current FSI count, FuSI count, total entries, and the embedding store size.

## Pattern Discovery (PatternEngine)

The `PatternEngine` provides three-tier pattern detection that runs on-demand rather than continuously:

**Known Patterns** — AST structural heuristics detect common code patterns: guard clauses, early returns, validation functions, error handling patterns, and async/await patterns. These are identified via AST node inspection and function body analysis.

**Framework Patterns** — package.json dependency detection identifies the frameworks in use (React, Express, etc.) and loads associated pattern rules. Framework-specific patterns like React hooks usage or Express middleware chains are detected based on imports and call patterns.

**Emergent Patterns** — function shape clustering and sequence detection identifies recurring code shapes that aren't framework-specific. Functions with similar parameter structures, return types, and call sequences are clustered. Common sequences (e.g., validate → sanitize → process → respond) are detected via N-gram analysis of function call chains.

PatternEngine runs via `spawn_blocking` because the workspace walk is synchronous I/O. It is triggered on-demand via `POST /api/v1/observer/patterns` rather than running automatically — this keeps the daemon responsive during normal editing while allowing deep analysis when requested.