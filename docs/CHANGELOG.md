# Change Log

All notable changes to the "memix" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

Here's the markdown to add at the top of your changelog entries, above whatever the previous most recent version was:

---

## [1.0.9-beta] — 2026-03-23
### Fixed
- The daemon's most critical reliability problem was that the Unix socket could bind
up to 11 seconds after process start because migrations, EmbeddingStore, and
TokenTracker all performed Redis I/O synchronously before the socket was created.
On throttled cloud Redis connections (Upstash free tier under load), this caused
consistent `ECONNREFUSED` failures at the extension health check. The entire
startup sequence has been restructured into two phases: everything before the
socket bind is now synchronous and Redis-free (EmbeddingStore and TokenTracker
both start empty), and all Redis-dependent work runs in deferred background tasks
that start 500ms–2s after the socket is already serving health checks. The daemon
now becomes reachable within ~50ms of process start regardless of Redis latency.
- The `warning_signature_*` entries were flooding the Brain Key Coverage panel
because the signature comparison logic fired on every file-save event even when
the AST cache had no prior snapshot for that file (i.e., every file on first open
looked like a "breaking change" from empty to its current state). Added a guard
that requires `old_bytes` to be non-empty before a warning entry is written, which
eliminates the entire class of false positives that appear at daemon startup and
after daemon restarts.
- The Sessions counter in Token Intelligence was incrementing on every 5-minute flush
cycle rather than once per daemon lifetime. The `LifetimeTotals::absorb_session`
method was incrementing `sessions_recorded` unconditionally, while a new
`session_recorded: AtomicBool` field was added to `TokenTracker` to ensure the
increment happens at most once per process — the first flush sets the flag, all
subsequent flushes skip the increment.
- The `openBrainKey` click handler in the Brain Key Coverage panel was silently
swallowed because no corresponding `case` existed in the `onDidReceiveMessage`
switch in `debug-panel.ts`. Clicks now open the `.memix/brain/<key>.json` mirror
file if it exists on disk, or an untitled JSON document with the raw Redis value
if it does not.
- The hot zones and stable zones lists were rendering with commas between items
because `.map()` was being string-concatenated without `.join('')`, triggering
JavaScript's implicit array-to-string coercion. All affected map chains now
explicitly join with an empty string.
- Rust module paths (`crate::brain::schema`, `chrono::Utc`, etc.) were appearing in
the Related Files list in the Predictive Intent panel because the dependency graph
import filter only blocked bare single-word package names. Extended the filter to
also reject any import specifier containing `::`, which covers all Rust use-paths
while leaving absolute and relative file paths intact.
- The tree-sitter bindings for `language_tsx`, `language_typescript`, and
`language` were renamed from callable functions to `LanguageFn` constants
(`LANGUAGE_TSX`, `LANGUAGE_TYPESCRIPT`, `LANGUAGE`) in v0.23 of the respective
crates. Updated all three call sites in `patterns.rs` to use the new constant
names.

### Added
- A 20-second in-memory cache for `get_entries` results was added to `RedisStorage`
as a `tokio::sync::RwLock<HashMap<String, (Instant, Vec<MemoryEntry>)>>`. The
cache is keyed by project ID and collapses rapid panel refreshes — which each
independently call `get_entries` — into at most one Redis round-trip per 20-second
window. Both `upsert_entry` and `delete_entry` synchronously invalidate the cache
for the affected project before touching Redis, preserving write-through
consistency so brain updates from `pending.json` are visible on the next read
without waiting for TTL expiry.
- `PatternEngine` (`observer/patterns.rs`) provides on-demand structural pattern
discovery across three tiers. Tier 1 (Known) detects 15+ universal software
patterns — async/await, guard clauses, repository, factory, singleton, and others
— by analysing function body structure from the tree-sitter AST. Tier 2
(Framework) reads `package.json` dependencies and cross-references them against a
curated map to detect Next.js App Router patterns, Prisma ORM, Tailwind CSS,
Vitest, custom hooks, and similar framework-specific shapes. Tier 3 (Emergent)
uses four unsupervised discovery strategies — function shape clustering, import
constellation analysis, export shape detection, and error-handling fingerprinting
— to surface patterns that have no predefined name but recur consistently in the
codebase. The engine is exposed via `GET /api/v1/observer/patterns`, runs in
`spawn_blocking` to avoid stalling the async executor, and is triggered manually
from the panel rather than on every file save.
- `EmbeddingStore::flush_disk_only()` was added as the new target for the 5-minute
periodic flush task. It writes the local binary file only and skips the Redis sync
entirely, eliminating ~30MB/day of outbound Redis traffic on the free tier. The
Redis sync path (`flush()` with a client argument) is preserved and commented with
a `MULTI-IDE NOTE` explaining when to re-enable it once cross-IDE sharing support
is built.
- `EmbeddingStore::load_into()` and `TokenTracker::load_lifetime_into()` were added
as deferred-loading counterparts to the existing constructors. Both accept an
already-constructed instance and populate it from disk or Redis after the fact,
enabling the new two-phase startup architecture where instances start empty and
load real data in background tasks after the socket is bound.

### Changed
- The Observer Code DNA section in the debug panel now shows languages and rules as
separate, independently refreshing subsections rather than mixing them into the
patterns block. Languages are rendered from the live DNA snapshot on every panel
refresh using a `stat`-row layout with file counts. Rules source only appears when
non-default rules are active, keeping the panel clean for the common case.
- The Daemon Timeline now displays human-readable sentences instead of raw JSON
payloads. `formatTimelineEvent` in `debug-panel.ts` maps each `SessionEvent`
variant to a descriptive English phrase — `AstMutation` becomes "File changed —
src/server.rs (3 nodes changed)", `IntentDetected` maps internal snake_case names
to labels like "Fixing a bug", and `ScorePenalty` includes a severity tier label.
Unknown future event types fall back to a camelCase-split representation rather
than raw JSON.
- The architecture label in Observer Code DNA maps raw internal identifiers like
`component-driven/application-layered` to display strings like
"Component-driven UI". Pattern tag names like `try-catch` and `edge-guards` map
to "Error handling (try/catch)" and "Guard clauses". Both mappings cover the full
set of values currently emitted by the DNA engine, with a `.replace(/-/g, ' ')`
fallback for any future unlabelled values.
- `compileContext` calls are now throttled to once per 30 seconds per active file in
`sendUpdate`. The other advanced calls — blast radius, causal chain, proactive
risk, and importance scores — are unaffected and run on every refresh because they
are lightweight. When the throttle suppresses a compile, the panel keeps displaying
the previously compiled context without flicker. The throttle resets immediately
whenever the active file changes.
- The inline `<script>` block from `debug-panel.ts` has been extracted to
`media/panel.js` and loaded via `getWebviewScript()`, following the same pattern
as the CSS extraction to `media/app.css`. The TypeScript file now contains only
class logic and HTML structure; all webview JavaScript lives in the separate file.
- `stripRoot` is now defined at module scope in `panel.js` and updated from
`data.workspaceRoot` on every `update` message, making it available to all
command handlers including `patternReport`. Previously it was a local function
inside the `update` block and unreachable from other handlers.

## [1.0.8-beta] - 2026-03-21 (Token Intelligence Panel)
### Features
- **Token Intelligence Debug Panel**: New "Token Intelligence" section in Advanced tab displaying session and lifetime token metrics
  - Session stats: AI Tokens consumed, Context tokens compiled, Tokens saved, Files indexed, Context compilations, Embedding cache efficiency, Compression ratio
  - Lifetime stats: Total AI tokens, Total tokens saved, Sessions recorded
- **Token Stats API Integration**: Extension now fetches token statistics from daemon `/api/v1/tokens/stats` and renders in real-time

## [1.0.7-beta] - 2026-03-20 (Structural Intelligence Layer)
### Features
- **Code Skeleton Index (FSI/FuSI)**: Live structural map of the codebase — File Skeleton Index gives per-file architecture summaries, Function Symbol Index gives per-function call/caller details. Stored in an isolated Redis hash with LRU eviction (separate from brain entries).
- **CallGraph Engine**: In-memory call graph rebuilt incrementally on file saves, powering the FuSI layer with `calls_from` / `callers_of` queries.
- **Skeleton-Aware Context Compiler**: FSI sections injected at priority 85, FuSI at priority 78 into the context compilation pipeline. Structural context now competes fairly with other sources in the DP knapsack budget fitting.
- **Skeleton Stats API**: `GET /api/v1/skeleton/stats/:project_id` returns FSI, FuSI, and total counts.
- **Embedding Store**: Write-through hybrid persistence with local binary file (`.memix/skeleton_embeddings.bin`) as fast read path, Redis hash as cross-IDE synchronization layer.
- **Background Indexer**: Startup workspace scan at configurable rate (default 10 files/sec), skips if index already populated, populates FSI + embeddings for entire project.
- **Dependency Graph v2**: `set_dependencies` for atomic edge replacement, petgraph integration for betweenness centrality, PageRank, SCC cycle detection, and topological ordering.
- **Blast Radius Analysis**: BFS-based forward transitive impact analysis with cycle-safe critical path reconstruction.
- **OXC Semantic Analysis**: Layer 2 enrichment for TS/JS with scope analysis, resolved import paths, dead import detection, call target resolution with source cache.
- **Naive Token Estimate**: Context compiler reports what a naive paste approach would have cost, enabling compression ratio calculation.
- **AGENTS.md Generation**: Rules file writer now generates `AGENTS.md` for workspace-root placement.

### Configuration
- **Environment-Driven Safeguards**: All skeleton limits (`MAX_FUNCTIONS_PER_FILE`, `MAX_TYPES_PER_FILE`, `MAX_IMPORTS_PER_FILE`, `MAX_DEPS_PER_FILE`, `MAX_HOT_FILES`, `MAX_SYMBOLS_PER_HOT_FILE`, `MAX_SKELETON_ENTRIES`, `FSI_DEBOUNCE_SECS`) are now configurable via `.env` with `MEMIX_*` prefix and safe fallback defaults.
- **Redis Connection Pooling**: Daemon now uses a single multiplexed `ConnectionManager` instead of per-request connections, preventing Redis connection exhaustion on cloud instances.

### UX
- **Clickable Memory Vectors**: Keys in "Top Memory Vectors (Size)" are now clickable links that open the corresponding brain JSON file in the editor.
- **Version Info**: New "Current Version" button in Settings shows daemon and extension versions with last-updated time.
- **Change Redis URL**: New setting in the Settings tab to switch Redis accounts without re-initializing.

### Documentation
- Added `docs/CODE_SKELETON_INDEX.md` — FSI/FuSI architecture, data flow, token budget math.
- Added `docs/CONTEXT_COMPILER.md` — 7-pass compilation pipeline, priorities, metrics.
- Added `docs/CALL_GRAPH.md` — In-memory call graph design and query API.
- Added `docs/REDIS_CONNECTION_POOLING.md` — ConnectionManager architecture.
- Added `docs/DAEMON_DEVELOPMENT.md` — Development guide, setup, common issues.

## [1.0.0-beta.5] - 2026-03-10 (Daemon Intelligence Layer + Advanced Panel UX)
### Features
- **AGENTS Runtime**: Added daemon-side `AGENTS.md` parsing and runtime execution for supported triggers, with persisted agent reports and config/report APIs.
- **Context Compiler**: Added a task-focused multi-pass context compiler exposed through `/api/v1/context/compile`.
- **Proactive Risk API**: Added risk scoring endpoint for active-file safety analysis.
- **Learning Layer**: Added prompt optimization, model-performance reporting, and cross-project developer-profile APIs.
- **Brain Hierarchy**: Added hierarchical context resolution API for layered/monorepo memory inheritance.
- **Extension Surfacing**: Wired the new daemon capabilities into the extension client and advanced debug panel.

### Build & Release
- **Daemon release pipeline**: Updated daemon build requirements to install `protoc` on CI runners before compiling the Rust daemon and embedding stack.
- **macOS compatibility target**: Daemon release artifacts are now built with macOS `12.0` as the deployment target.

### UX
- **Prompt Pack Modal UX**: Prompt Pack is now summarized in-panel and opened in a modal for full inspection/copying.
- **Observer DNA OTel Modal UX**: OTel export is now action-first instead of raw inline JSON.
- **Silent Background Refresh**: Background panel refresh no longer shows noisy loading copy; explicit user actions still surface progress.
- **Coverage Semantics**: Brain Key Coverage now distinguishes required, recommended, generated, and system keys with a restore path for missing baseline keys.

## [1.0.0-beta.4] - 2026-03-09 (Mirror Maturity + Migration Safety)
### Features
- **Embedding Cache**: Added content-hash embedding cache (DashMap) to avoid recomputing vectors for unchanged content.
- **Hybrid Similarity**: Upgraded semantic ranking to normalized cosine + keyword overlap scoring.
- **AST-driven Code DNA**: Replaced heuristic DNA scoring with Tree-Sitter-backed pattern extraction, exported symbol detection, and AST-derived cyclomatic complexity.
- **Project-specific DNA Rules**: Added workspace-level DNA override support via `dna_rules.toml` (workspace root), with `.memix/dna_rules.toml` as a fallback, for custom categorization and pattern tagging.
- **Explainability Summary**: Added a compact natural-language architecture summary covering hot zones, dependency depth, circular risks, and dominant patterns.
- **OTel DNA Export**: Added standardized observer DNA export endpoint for downstream tooling interoperability.
  - `GET /api/v1/observer/dna/otel`
- **Observer UI Upgrade**: Extension debug panel now surfaces explainability text, language mix, rule provenance, and circular risk highlights from observer DNA.
- **Bulk Mirror APIs**: Added daemon endpoints for full brain mirror import/export.
  - `POST /api/v1/brain/export/:project_id`
  - `POST /api/v1/brain/import/:project_id`
- **Migration Framework**: Added project migration runner with embedding backfill and schema marker entry.
  - `POST /api/v1/brain/migrate/:project_id`
- **Extension Value Upgrades**: Added actionable commands in Memix Settings:
  - Export Brain Mirror
  - Import Brain Mirror
  - Run Brain Migrations

### Reliability & Testing
- Added ignored integration-style stress test for Redis/JSON mirror export-import roundtrip under load.

## [1.0.0-beta.3] - 2026-03-06 (Autonomous Pair Programmer + Multi-Language)
### Features
- **Autonomous Pair Programmer**: Proactive impact analysis that predicts what will break BEFORE you make changes
- **Conflict Detection**: Automatically detects simultaneous modifications that may cause conflicts
- **Unlimited Language Support**: Added tree-sitter parsers for Python, Go, Java, C/C++ (in addition to existing TS/JS/Rust)
- **Rules Generation via Rust**: Complete rules file generation moved to Rust daemon for all IDEs
- **AGENTS.md Protocol**: Created standard protocol file for AI IDEs that support AGENTS.md convention
- **Extended Daemon API**: Full REST API for impact analysis, token counting, context optimization

### Architecture
- `daemon/src/rules/mod.rs` - Rust rules generation engine
- `daemon/src/intelligence/autonomous.rs` - Autonomous Pair Programmer
- `daemon/src/observer/parser.rs` - Multi-language AST parsing
- Updated daemon HTTP API with `/api/v1/rules/generate`, `/api/v1/tokens/*`, `/api/v1/autonomous/*`

## [1.0.0-beta.2] - 2026-03-06 (Rust Native Re-architecture)
### Features
- **Core Engine**: Fully rebuilt intelligence layer operating autonomously in a local Rust Daemon bridging via UDS sockets.
- **Predictive Context & Intent Detection**: Observes edit distances within AST trees using `tree-sitter` and preemptively loads relevant Vector chunks.
- **Offline Local Embeddings**: Ships with local 384-dimensional vector embedding support and native cosine-similarity-based retrieval.
- **Full-Text Brain Search**: Integrates Tantivy for indexing and traversing deep semantic history allowing massive brain volumes without contextual token-wasting.
- **Negative Memories & Contradictions**: Memory models now support complex graph resolutions mapping legacy logic over directly to newer architectures dynamically.
- **Secure Architecture Sync**: Supports standard `AES-256-GCM` encryption inside Redis, completely mitigating read risks on shared environments, mapped through robust `CRDT` merging algorithms.

## [1.0.0-beta.1] - Unreleased
### Features
- Persistent brain storage with Redis
- Session tracking and continuity
- Health monitoring and smart pruning
- Team Sync (push, pull, merge)
- Import / Export brain data
- Secure credential storage via OS keychain
- IDE auto-detection (VS Code, Cursor, Windsurf, Antigravity)
- Brain Monitor sidebar panel with real-time stats
- Auto-generated rules files for all supported IDEs
- File-based brain sync — AI reads/writes local `.memix/brain/*.json` files, extension auto-syncs to Redis
- Prompt conventions documented in generated rules for assistants that follow Memix workflows
- Brain/task-related structures generated as part of Memix rules and memory flows

### Security
- Credentials stored via VS Code SecretStorage API (OS keychain)
- Built-in secret detection blocks API keys, tokens, and passwords from brain data
- No telemetry, local-first architecture, no MCP dependency
