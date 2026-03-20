# Change Log

All notable changes to the "memix" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

## [1.0.7-beta] - 2026-03-20 (Code Skeleton Index + Panel Enhancements)
### Features
- **Code Skeleton Index (FSI/FuSI)**: Live structural map of the codebase — File Skeleton Index gives per-file architecture summaries, Function Symbol Index gives per-function call/caller details. Stored in an isolated Redis hash with LRU eviction (separate from brain entries).
- **CallGraph Engine**: In-memory call graph rebuilt incrementally on file saves, powering the FuSI layer with `calls_from` / `callers_of` queries.
- **Skeleton-Aware Context Compiler**: FSI sections injected at priority 85, FuSI at priority 78 into the context compilation pipeline. Structural context now competes fairly with other sources in the DP knapsack budget fitting.
- **Skeleton Stats API**: `GET /api/v1/skeleton/stats/:project_id` returns FSI, FuSI, and total counts.

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
