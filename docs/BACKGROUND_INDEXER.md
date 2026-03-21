# Background Indexer

## Overview

The **Background Indexer** performs the one-time initial build of the Code Skeleton Index when a project is first opened. It is conceptually identical to TypeScript's language server indexing process: it starts automatically after daemon launch, runs quietly in the background at a controlled pace, and finishes without any user interaction. Subsequent daemon restarts skip the indexer entirely because the index is loaded from the persisted embedding store.

## Problem

On first project open, the skeleton index is empty. Without a populated index, the context compiler cannot inject skeleton sections, the semantic similarity search returns no results, and the embedding store has nothing to search against. The question is how to populate the index for a project with hundreds or thousands of files without blocking the developer's work during that process.

## Solution

The `BackgroundIndexer` starts 5 seconds after the daemon launches (to allow the socket to bind and the extension's health check to succeed first), then processes the workspace at a rate-limited pace. The default rate limit is 10 files per second, configurable via `MEMIX_INDEXER_RATE_LIMIT`. This means a 500-file project takes approximately 50 seconds to fully index — a duration during which the developer can already be working normally, because the indexer yields to the event loop between every file.

The indexer only runs when the embedding store is empty. If the binary file at `.memix/skeleton_embeddings.bin` was loaded successfully at startup (which happens on every restart after the first), the indexer detects this and returns immediately without doing any work. The `MEMIX_FORCE_REINDEX` environment variable overrides this check and forces a full rescan, which is useful during development or after significant structural changes to the project.

## What the Indexer Does

The indexer walks the workspace directory recursively using `walkdir`, skipping directories that contain build artifacts and transient files: `node_modules`, `.git`, `target`, `dist`, `build`, `.next`, `.cache`, `__pycache__`, `.venv`, and `vendor`. It collects all source files with supported extensions and processes up to 10,000 files per run (a hard cap to prevent runaway behavior on misconfigured workspace roots pointing at very large directories).

For each file, the indexer runs the same tree-sitter parsing pass that the live file watcher uses: it reads the file, parses it with `AstParser`, extracts features, builds a `FileSkeleton`, converts it to a `MemoryEntry`, and persists the FSI entry to the skeleton Redis hash. After persisting, it calls `embed_text_static` to compute the 384-dimensional embedding vector and upserts it into the `EmbeddingStore`.

The progress is reported through the `TokenTracker`: the `files_skeleton_indexed` session counter increments with each successfully processed file. The debug panel's Token Intelligence section shows this count, so developers can see indexing progress without any separate progress indicator.

After completing the full workspace scan, the indexer calls `EmbeddingStore::flush` to write all computed embeddings to the binary file and the Redis hash simultaneously. From that point forward, all embeddings are available for semantic similarity search.

## Rate Limiting Behavior

The `tokio::time::sleep(delay_between_files)` call between each file serves two purposes. First, it yields execution back to the tokio runtime, allowing the main event loop to process file-save events and HTTP requests without queuing behind the indexer. Second, it limits CPU and I/O consumption to a predictable rate, avoiding the situation where the indexer saturates the machine during the developer's active morning setup routine.

At the default rate of 10 files per second, the delay is 100ms between files. Setting `MEMIX_INDEXER_RATE_LIMIT=50` reduces this to 20ms, completing a 500-file project in about 10 seconds at the cost of higher CPU usage during that window. Setting it to 1 reduces the impact to nearly zero but takes proportionally longer.

## Relationship to the Live File Watcher

The background indexer and the live file watcher are parallel systems that converge on the same index. The indexer handles the initial bulk population; the watcher handles incremental updates when individual files change during normal development. A file saved while the indexer is running will be processed twice — once by the watcher (immediately, because file saves are high priority) and once by the indexer when it reaches that file in its walk. The second processing is a harmless upsert that writes the same or newer data over the indexer's result.

## Key File

`daemon/src/observer/background_indexer.rs` contains the `BackgroundIndexer` struct, the `run_if_needed` entry point, the `collect_supported_files` workspace walker, and the throttled processing loop.