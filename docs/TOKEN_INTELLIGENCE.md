# Token Intelligence

## Overview

The **Token Intelligence** system gives developers a precise, honest accounting of every token Memix touches on their behalf — across both the current working session and the lifetime of the project. It tracks three fundamentally different token dimensions: tokens consumed by AI models, tokens compiled by the context compiler, and tokens saved through Memix's structural compression techniques.

Most AI coding tools track only one dimension: tokens sent to a model. This tells you what things cost but nothing about whether they were necessary. Token Intelligence answers the harder questions — how much of that cost was avoidable, how efficiently is the context compiler operating, and what has Memix saved over the life of the project.

## The Three Token Dimensions

**Consumed tokens** are the tokens actually sent to or received from an AI model during a working session. These are recorded when the extension reports a completed AI exchange via the `POST /api/v1/tokens/record` endpoint. Consumed tokens accumulate into both session and lifetime totals, and the per-call statistics (last, max, min, average) help identify which kinds of tasks are token-expensive.

**Compiled tokens** are the tokens the context compiler assembled across all compilation calls. A compiled token is not the same as a consumed token — many compiled context packets may be assembled before a developer sends a single message, as the compiler runs speculatively when the active file changes. The compiled token count tells you how much structural analysis work the daemon has done on the developer's behalf.

**Saved tokens** are the difference between what the context compiler actually produced and what a naive approach would have cost. The naive estimate is calculated during Pass 1 of the context compilation pipeline: the sum of byte sizes of all relevant files multiplied by a 0.25 tokens-per-byte heuristic for source code. When the compiler produces a 1,200-token packet covering eight relevant files that would have required 6,400 tokens as raw pastes, the saving is 5,200 tokens. This is the most diagnostic of the three dimensions because it measures the leverage Memix provides — a developer seeing a 5× compression ratio is getting meaningful structural intelligence, while a ratio near 1× suggests the skeleton index may not yet be fully populated.

## Session vs. Lifetime Tracking

Session counters are held entirely in atomic variables — `AtomicU64` values that can be incremented from any async task without taking a lock. This matters because context compilations happen on the hot path of the event loop, and any locking overhead there would accumulate into visible latency. All session counters reset to zero when the daemon starts; they represent the current working session only.

Lifetime totals are persisted to `.memix/token_lifetime.json` inside the project's data directory. A flush task runs every 5 minutes and on graceful shutdown, reading the current session snapshot and adding it to the running lifetime totals before writing the updated JSON to disk. The flush is a no-op if both `ai_calls` and `context_compilations` are zero, which prevents unnecessary disk writes when the daemon is idle.

Loading lifetime totals at startup is designed to handle partial failures gracefully. If the JSON file is missing (new project), corrupted, or from an incompatible version, the system starts fresh with zero lifetime totals rather than failing to start. Session counters always start at zero regardless of lifetime state.

## Embedding Cache Efficiency

Two additional counters track the performance of the embedding cache specifically: `embedding_cache_hits` and `embedding_cache_misses`. These are maintained by the `EmbeddingStore` and exposed through the token stats response. The `cache_efficiency_pct` field in `TokenStatsResponse` is computed as hits divided by total lookups, expressed as a percentage.

A freshly started daemon with an empty embedding store will see 100% misses until the background indexer completes its first pass. Once the store is fully populated, subsequent sessions should see very high hit rates (above 90%) because most files don't change between sessions. A low hit rate on a mature project indicates that many files are changing frequently — which is expected on an active development day — or that the embedding store is being rebuilt more often than necessary.

## API

`GET /api/v1/tokens/stats` returns a `TokenStatsResponse` containing a `session` snapshot, `lifetime` totals, `cache_efficiency_pct`, and `compression_ratio`. A single call to this endpoint provides everything the debug panel needs to render the full Token Intelligence section.

`POST /api/v1/tokens/record` accepts `{ "tokens": N, "task_type": "string" }` from the extension and records an AI model call. The extension should call this endpoint when it can observe that an AI exchange has completed — this is how the consumed tokens dimension is populated.

## Key Files

`daemon/src/token/tracker.rs` contains the complete implementation including `SessionCounters`, `LifetimeTotals`, `TokenTracker`, `SessionSnapshot`, and `TokenStatsResponse`. The `SessionCounters::new()` constructor initializes `ai_tokens_min` to `u64::MAX` (the sentinel value meaning "no calls yet"), which is converted to zero in the snapshot for display.

The `record_context_compilation` method on `SessionCounters` is called by the context compiler immediately after each successful compilation, passing both the actual token count and the naive estimate. The method computes the saving internally and atomically increments all relevant counters.

## Persistence Format

The lifetime totals file at `.memix/token_lifetime.json` is a human-readable JSON file. It can be inspected, backed up, or deleted independently of any other daemon state. Deleting it resets only the lifetime totals — session counters and the skeleton index are unaffected. The file is written using `tokio::fs::write` directly (not with a temp-file rename) because the data is append-only totals that are safe to partially overwrite, and the lower I/O overhead on the periodic flush path is preferable.

## Debug Panel Display

The Token Intelligence section in the debug panel is organized into three groups. The Session group shows context compilations, tokens compiled by Memix, tokens sent to AI, and the per-call breakdown of last, largest, smallest, and average. The highlighted metric is estimated tokens saved this session. The Lifetime group shows total AI tokens consumed, total tokens saved by Memix, the number of sessions recorded, and the last updated timestamp. The Index Health group shows files indexed this session, the embedding cache efficiency percentage, and the current compression ratio.