# Change Log

All notable changes to the "memix" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

---

## [1.5.0] (Daemon: 0.8.0-beta) — 2026-04-03
### Added
- **Multi-IDE Support:** Memix now works across multiple AI IDEs simultaneously (VS Code, Cursor, Windsurf, Claude Code, etc.).
  - **Shared Daemon Binary:** All IDEs share the same daemon binary at `~/.memix/bin/` instead of each downloading their own copy.
  - **Active Workspace Context:** Health and config endpoints now return data for the currently active (focused) workspace.
  - **Per-Project Config:** `daemon_config.json` is now read from the active workspace's `.memix/` directory.
  - **Workspace Registry API:** `/api/v1/workspace/register`, `/unregister`, `/activate`, `/list` for workspace lifecycle management.
  - **Per-Workspace Indexing:** Background indexer spawns independently for each registered workspace.
  - **Instant Project Switching:** ~0ms switch time when changing between projects - no daemon restart needed.
  - **Window Focus Tracking:** Active workspace updates automatically when switching VS Code windows.

### Fixed
- **Per-Workspace Brain Pause:** Brain pause/resume now applies to the active workspace only, not globally across all IDEs/windows.
- **Intelligence Metrics Per-Project:** Decisions, Core Facts, Patterns, and Anti-Patterns now show correct counts for the active workspace.
- **Session State Per-Project:** Last Updated and Current Task now reflect the active workspace's state.
- **Config Path Bug:** Fixed `/control/status` and health endpoint returning config path from first opened project instead of currently active project.

### Known Limitations
- **Observer Data (Code DNA, Git Insights, Call Graph):** These are currently global singletons. Per-workspace observer data is planned for a future release.
- **Token Intelligence (Cache Efficiency):** Token stats (session compilations, cache efficiency, compression ratio) are currently global across all workspaces. Per-workspace token tracking is planned for a future release.
- **Tasks:** Pending tasks count requires the `tasks.json` brain file to be populated. This is a manual process or requires rules-based automation.

---

## [1.4.4-beta] (Daemon: 0.7.2-beta) — 2026-04-03
### Fixed
- **Project Sticking Bug:** Fixed critical bug where the first initialized project would "stick" to the daemon, preventing subsequent projects from being properly initialized. The daemon now correctly handles multiple projects through the workspace registry instead of restarting for each project change.
- **Redundant Daemon Downloads:** Fixed issue where each IDE (VS Code, Cursor, Windsurf, etc.) downloaded its own copy of the daemon binary. The daemon is now stored in a shared location `~/.memix/bin/` so all IDEs use the same binary.

---

## [1.4.0-beta] (Daemon: 0.7.0-beta) — 2026-03-26
### Added
- **Context Orchestrator Refinements:** Major improvements to context relevance and accuracy.
  - **Query-Aware Rules Pruning:** Rules files now match against query terms, not just task-type keywords. Sections containing query terms get +0–10 priority boost.
  - **AGENTS.md Exclusion:** Generic AI instruction files are now excluded from context compilation. They are agent prompts, not project-specific conventions.
  - **Global GENERATED_DIRS Constant:** 150+ generated directory patterns consolidated into `daemon/src/constants.rs` for consistent filtering across indexer and compiler.
  - **Context Orchestrator Documentation:** New `docs/CONTEXT_ORCHESTRATOR.md` documenting architecture, query-aware selection, and output format.

### Fixed
- **Intelligence Metrics:** Decisions and Patterns now correctly persist and display in debug panel.
  - Decisions stored as JSON array under `decisions.json` key.
  - Patterns persisted to `patterns.json` after scan.
- **Session State Auto-Tracking:** `session_state.json` now auto-updates with modified files on every file save.
- **Skeleton Index Filtering:** `.next/` and other generated directories now correctly excluded from background indexer using unified GENERATED_DIRS constant.

### Changed
- **Rules File Candidates:** Now includes `.cursorrules`, `.github/copilot-instructions.md`, and `.rules/` directory patterns.
- **Orchestrator Output:** Simplified header format to `MEMIX STRUCTURAL CONTEXT — {n} sections`.

---

## [1.3.0-beta] (Daemon: 0.6.0-beta) — 2026-03-26
### Added
- **Intelligent Decision Detection Engine:** World-class automated architectural decision recording system that observes code changes and automatically captures the "why" behind code evolution.
  - **TOML-based Rule System:** 70+ configurable rules in `daemon/rules/decisions.toml` covering dependencies, structure, patterns, APIs, config, refactoring, testing, and documentation decisions. Hot-reloadable without recompilation.
  - **AST Pattern Matching:** Tree-sitter queries detect singleton, repository, factory, middleware patterns directly from code structure.
  - **Embedding Similarity Detection:** Uses AllMiniLM-L6-v2 embeddings to detect patterns not covered by explicit rules (≥92% similarity threshold).
  - **Multi-Signal Processing:** Handles dependency additions, directory creation, file saves, moves, config changes, endpoint creation, git commits, and symbol renames.
  - **Decision Feedback API:** `POST /api/v1/decisions/:id/feedback` accepts useful/dismissed/incorrect feedback from users.
  - **Self-Improving Confidence:** Rules automatically adjust confidence based on accumulated feedback (+0.05 useful, -0.02 dismissed, -0.10 incorrect).
  - **Cross-Feature Enhancement:** Decisions enhance warnings (contradiction detection), prompt packs (decision context), code review (dependency conflicts), and onboarding (architecture history).

### Changed
- **DecisionDetector Architecture:** Refactored from naive hardcoded detection to intelligent rule-based engine with async processing and deduplication.
- **Brain Storage:** Decisions now stored with `MemoryKind::Decision` and `MemorySource::AgentExtracted`, including rule provenance and evidence chains.

---

## [1.2.0-beta] (Daemon: 0.5.0-beta) — 2026-03-25
### Fixed
- **Scan Patterns Route:** Fixed 404 error when clicking "Scan Patterns" button - route was `/observer/patterns` but client called `/api/v1/observer/patterns`.
- **Brain File Location:** Fixed daemon writing brain files to wrong directory - `data_dir` now resolves to absolute path using workspace root.
- **.env Loading Order:** Fixed daemon not loading workspace `.env` early enough, causing `MEMIX_WORKSPACE_ROOT` to be unset.
- **Windsurf Rules Format:** Changed from single `.windsurfrules` file to `.windsurf/rules/*.md` directory format (matches Cursor pattern).
- **Claude Code Rules:** Changed from `CLAUDE.md` to `.claude/rules/memix.md` directory format.
- **Next.js Dev Noise:** Added exclusion patterns to file watcher (`.next/dev`, `node_modules`, `.git`, `target`, etc.) to prevent build artifacts from polluting timeline.
- **UI Performance:** Parallelized independent API calls in `sendUpdate()` to reduce button response latency.
- **Rules Templates:** Shortened brain and guard rule templates while preserving all core concepts (~300 lines → ~90 lines combined).

### Changed
- **Rules Directory Structure:** Windsurf and Claude Code now use dedicated rules directories instead of single files in root.

---

## [1.1.0-beta] (Daemon: 0.4.0-beta) — 2026-03-24
### Added
- **Proactive Risk UI:** Completely redesigned the Proactive Risk dashboard to parse raw warning signatures into clean bullet points with a "Open explanation" modal displaying the diff changes.
- **Skeleton Cache:** Added a 60-second in-memory locking TTL cache to the Redis storage backend. This safely eliminates hundreds of redundant `HVALS` calls hitting the Redis database when the UI polls for skeleton structures.
- **Accurate Size Accreation:** Added `size_bytes` calculation directly into the Rust daemon `/api/v1/skeleton/stats` endpoint. `Memix Size` now aggregates the size of `memix-project_skeletons` locally and accurately files them under the *Semantic* Memory Category.

### Fixed
- **UI Polling Spikes:** Dropped the background advanced data polling span inside `debug-panel.ts` from 5 seconds to 15 seconds to drastically lower CPU footprints.
- **Background Flusher Lifetimes:** Consolidated the warning signature cleanup loop directly into the 5-minute telemetry token flush worker. This fixes a double-borrow allocation error and centralizes the `tokio::spawn` intervals perfectly into one unit.

## [1.0.11-beta] (Daemon: 0.3.2-beta) — 2026-03-23
### Added
- **Brain Sleep Check:** Added handling during `initBrain()` to detect if the daemon is currently paused; if so, it temporarily resumes processing, performs initialization, and restores the paused state.
- **Client Control APIs:** Implemented `/control/status`, `/control/resume`, and `/control/pause` API wrappers natively in the `MemoryClient`.

### Fixed
- **Daemon Boot Latency:** Removed hardcoded sleep allocations (`sleep(5s)`) across startup workers (`run_project_migrations`, `load_lifetime_into`, etc.). They now listen accurately via a `Notify` sync token guaranteeing background indexing begins the instant the unix socket is bound.
- **Embedding Cache Mutex:** Exchanged `std::sync::Mutex` for `parking_lot::Mutex` in FastEmbed initialization to prevent thread panics and handle text chunks safely across asynchronous Redis workers.
- **CI Interpolation Bug:** Fixed a YAML deployment issue where GitHub Action string expansion passed the literal Bash variable parsing string `"Daemon v${GITHUB_REF_NAME#daemon-v}"` to release tags. Release workflow now executes extraction via CLI first.

---

## [1.0.10-beta] (Daemon: 0.3.1-beta) — 2026-03-23
### Added
- **Change Redis Connection:** Added a new quick-pick option in the Memix Settings menu to easily swap the active Redis connection without clearing the brain.
- **ONNX Bundle Pipeline:** GitHub Actions now explicitly download and bundle the ONNX Runtime `libonnxruntime` libraries into release ZIP artifacts for all platforms (macOS, Windows, Linux).

### Fixed
- **Debug Panel UI/UX Polish:** Revamped Advanced tabs (`Compiled Context`, `Brain Key Coverage`, `Top Memory Vectors`) to use responsive flexbox grids with 50/50 splits, text truncation with hover tooltips, and clickable inline file-path links.
- **FastEmbed & ORT Stability:** Resolved a `fastembed` / `ort-sys` dependency incompatibility by upgrading to `fastembed v5` and `ort 2.0.0` stable, ensuring ONNX features compile correctly across OS toolchains.

---

## [1.0.9-beta] (Daemon: 0.3.0-beta) — 2026-03-23
### Added
- **Pattern Engine:** Added a 3-tier structural pattern discovery engine (Known, Framework, Emergent) running as an on-demand async task to avoid disk stalls.
- **Redis Throttling:** Added a 20-second edge cache to `get_entries` to prevent rapid debug-panel refreshes from rate-limiting the Redis connection, while maintaining write-through synchronization for updates.
- **Free-Tier Optimization:** The periodic 5-minute telemetry flush now strictly writes to the local `.bin` file, disabling outbound Redis pushes and saving ~30MB/day of cloud DB bandwith.
- **Deferred Loading:** Separated startup loading into structural init and background data population methods for non-blocking UI interactions (`load_into`, `load_lifetime_into`).

### Changed
- **Code DNA Panel:** Cleaned up the Observer Code DNA display format by splitting language stats and system rules into dedicated view states.
- **Human-Readable Timelines:** Daemon Timeline payloads are now mapped into descriptive English sentences (e.g. "Fixing a bug", "File changed (3 nodes)").
- **Context Debouncing:** Prevented flickering in the Advanced Panel by throttling `compileContext` to once per 30 seconds per active file.
- **Webview Isolation:** Successfully decoupled all inline JavaScript block logic (`debug-panel.ts`) into a standalone `panel.js` asset for better Content Security Policy (CSP) alignment.

### Fixed
- **Daemon Boot Resilience:** Fixed an `ECONNREFUSED` cloud DB race condition by moving all Redis sync tasks into deferred background threads (500ms–2s), guaranteeing the extension socket binds under 50ms.
- **False-Positive Git Warnings:** Brain Key Coverage warning anomalies emitted during initial AST builds the first time a file opened have been suppressed.
- **Telemetry Counter:** The Tokens Lifetime session counter now strictly increments only once per lifespan rather than compounding on every 5-minute tick.
- **Click Actions:** Fixed the webview's `openBrainKey` message handler so that clicking internal `.memix` JSON mirror states properly opens the document in VS Code.
- **UI Concatenation Ticks:** Cleaned up rogue commas showing in the stable zones list caused by implicit `.map()` JS coercions.
- **Cross-Module Imports:** Rust module imports (e.g. `crate::brain::schema`) are now correctly excluded from the Predictive Intent file path array.
- **Tree-Sitter Bump:** Patched bindings to use `LANGUAGE_TSX` consts over deprecated callables to fix tree-sitter v0.23 breaking changes.

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
