# Memix Roadmap

> **Memix** is an autonomous engineering intelligence layer for AI-assisted coding. This roadmap tracks what has been shipped, what is in active development, and where the project is heading.

---

## Released — v1.0.0-beta.1

- ✅ Persistent brain storage with Redis
- ✅ Session tracking and continuity
- ✅ Health monitoring and smart pruning
- ✅ Team Sync (push, pull, merge)
- ✅ Import / Export brain data
- ✅ Secure credential storage (OS keychain)
- ✅ IDE auto-detection (VS Code, Cursor, Windsurf, Antigravity, Claude Code)
- ✅ Brain Monitor sidebar panel
- ✅ Auto-generated rules files for all IDEs
- ✅ File-based brain sync (`.memix/brain/*.json`)
- ✅ Prompt Pack preview + copy to clipboard

---

## Released — v1.0.0-beta.2 (Native Core)

- ✅ Rebuilt backend memory engine in Rust (Axum HTTP server, tokio async runtime)
- ✅ Autonomous codebase observation via tree-sitter AST integration (8 languages)
- ✅ Offline semantic embeddings using fastembed + AllMiniLM-L6-v2
- ✅ Code DNA and dependency mapping with O(1) adjacency matrix
- ✅ Advanced predictor with DashMap concurrency caching
- ✅ CRDT team sync foundation with AES-256-GCM key storage
- ✅ Git archaeology via native git2

---

## Released — v1.0.0-beta.4 (Daemon-Managed Mirror + Migration Safety)

- ✅ Daemon-managed JSON mirror with atomic writes
- ✅ Pending writeback flow (pending.json → validate/apply → pending.ack.json → clear)
- ✅ Embedding cache (content-hash keyed) to avoid repeated vector compute
- ✅ Hybrid similarity scoring (cosine + keyword overlap)
- ✅ Observer and session endpoints with persisted artifacts
- ✅ Migration framework with per-project schema markers
- ✅ Single-daemon PID lock with stale lock recovery

---

## Released — v1.0.0-beta.5 (Intelligence & Language Expansion)

- ✅ 13 languages: added C#, Ruby, Swift, Kotlin, PHP (complete AST entity extraction, complexity, exports, pattern tagging)
- ✅ 7-pass Context Compiler with DP knapsack budget fitting
- ✅ Multi-factor Intent Engine (4 signal categories, 7 intent types, calibrated confidence)
- ✅ Flight Recorder with O(1) analytics counters and session timeline
- ✅ Configurable Security Scanner (externalized to `memix-security.toml`)
- ✅ Prompt replay and optimization (learning layer)
- ✅ AI model performance tracking by task type
- ✅ Cross-project developer profile
- ✅ Brain hierarchy with CSS-cascade layer merging
- ✅ TokenEngine caching (process-wide BPE tokenizer)

---

## Released — v1.0.8-beta (Token Intelligence + Structural Intelligence Layer)

- ✅ **Resolved Call Graph** — dual-index architecture with `ResolvedEdge` (callee file + line) for TypeScript/JavaScript and nominal fallback for all other languages
- ✅ **OXC Semantic Analysis** — Layer 2 enrichment for TS/JS: scope analysis, resolved import paths, dead import detection, call target resolution with source cache
- ✅ **Code Skeleton Index (3-layer)** — FSI (one per file) + FuSI (one per hot function) stored in an isolated Redis hash, with betweenness and PageRank priority boosting in the context compiler
- ✅ **Embedding Store** — write-through hybrid persistence: local binary file (`.memix/skeleton_embeddings.bin`) as fast read path, Redis hash as cross-IDE synchronization layer
- ✅ **Background Indexer** — startup workspace scan at configurable rate (default 10 files/sec), skips if index already populated, populates FSI + embeddings for entire project
- ✅ **Dependency Graph v2** — `set_dependencies` for atomic edge replacement, petgraph integration for betweenness centrality, PageRank, SCC cycle detection, and topological ordering
- ✅ **Blast Radius Analysis** — BFS-based forward transitive impact analysis with cycle-safe critical path reconstruction
- ✅ **Token Intelligence** — three-dimension token accounting: session and lifetime totals for AI consumption, context compilation, and estimated tokens saved; embedding cache efficiency; compression ratio
- ✅ **Naive Token Estimate** — context compiler reports what a naive paste approach would have cost, enabling compression ratio calculation
- ✅ `ImportanceScores` injected into context compiler priority boosting
- ✅ `FileCausalContext` exposed through AppState call graph for HTTP handler access
- ✅ `AGENTS.md` generation added to rules file writer for workspace-root placement

---

## Released — v1.0.9-beta (Performance & Pattern Discovery)

- ✅ **Brain Entry Cache** — `RedisStorage` now maintains an in-memory `entry_cache` with 20-second TTL, keyed by project_id. Synchronously invalidated on writes to preserve consistency. Reduces Redis round-trips for repeated brain queries during context compilation.
- ✅ **Deferred Startup Architecture** — Two-phase startup: Phase One binds the socket in under 50ms (empty stores, no I/O), Phase Two loads lifetime totals, embeddings, and runs migrations in the background. Health checks succeed before Redis I/O begins.
- ✅ **PatternEngine** — Three-tier pattern detection: Known (AST structural heuristics), Framework (package.json dependency detection), and Emergent (function shape clustering and sequence detection). Runs on-demand via `POST /api/v1/observer/patterns`.

---

## Near-Term

### Context Orchestrator
A middleware endpoint (`POST /api/v1/orchestrate`) that uses the daemon's live structural index to produce an AI-ready enhanced prompt. A small, inexpensive model call (GPT-4o-mini or equivalent) reformulates the developer's raw question into a structurally precise prompt with exact file references, function names, and call chain context. Falls back gracefully to structural-context-only injection if the model call fails or is unconfigured.

### Automatic Context Injection
Wire the context compiler into the extension's `onDidChangeActiveTextEditor` listener so that a compiled context packet is assembled speculatively whenever the developer switches files. When they open the AI chat within a few seconds of the file switch, the pre-compiled context is available instantly rather than being computed on demand.

### Multi-Project Daemon
Run a single daemon instance attached to multiple workspace roots simultaneously. Requires per-project watcher isolation, per-project storage namespacing, and a project registry with explicit attach/detach lifecycle endpoints.

### Proactive Risk Warning Popups
Surface the existing `/api/v1/proactive/risk` endpoint results as non-intrusive VS Code decoration or notification when the developer opens a high-risk file. The risk score already exists; the missing piece is the extension-side wiring.

### Salsa Incremental Computation
Replace the current full-rebuild-per-save skeleton computation with a salsa-based incremental model. Files whose AST output is unchanged between saves would skip Redis writes entirely, reducing I/O by 80%+ on typical save-without-meaningful-change events (whitespace reformatting, comment edits).

### DriftDetector Agent
Autonomous agent that compares brain entries against the live filesystem to detect stale context: deleted files still referenced in memory, renamed exports, mismatched package.json dependencies, and entries that reference symbols that no longer exist.

### DeadCodeDetector Agent
Graph-walk agent that uses the dependency graph to identify zero-importer exports, unreferenced files, orphaned types, and unused utility functions. Generates actionable cleanup recommendations stored as warning entries.

### Conversation Compactor
Structured extraction of facts, decisions, corrections, and preferences from AI conversation history, feeding extracted knowledge back into the brain so context compounds across chat sessions rather than only from file saves.

---

## Mid-Term

### Cursor-Position-Aware Context
Resolve context at the scope level, not just the file level. Use the call graph and symbol index to build function-local context packages that follow the exact symbols referenced at the cursor position.

### CRDT Sync with Automerge
Replace the current CRDT foundation with Automerge as the default collaborative model for Team Sync, enabling proper three-way merge semantics for concurrent brain edits across team members.

### Team Brain Analytics Dashboard
Web application connecting directly to the project's Redis instance. Visualizes team-wide brain health, decision velocity, sync frequency, and contributor-level context quality metrics.

### Brain Compression for Very Large Projects
For projects exceeding 2,000 indexed files, introduce tiered compression: most-frequently-accessed skeleton entries remain in the hot tier, older entries are compressed and promoted on demand based on access patterns.

---

## Long-Term

### Multi-IDE Simultaneous Support
Support multiple IDE instances (Cursor + Claude Code + Windsurf) attached to the same project brain simultaneously. The embedding store's Redis synchronization layer and the brain's CRDT architecture are already designed for this — the remaining work is lifecycle management, per-IDE session scoping, and conflict resolution for concurrent brain writes.

### Knowledge Graph Intelligence
Causal memory graph using `caused_by` and `enables` relationships for reasoning-chain retrieval. Contradiction and supersession links so historical truth is preserved across schema changes without AI confusion.

---

## Community-Driven Features

| Problem | Memix Solution | Status |
|---|---|---|
| "AI wakes up with amnesia every session" | Persistent brain with boot sequence | ✅ Shipped |
| "I keep re-explaining my project" | Identity + patterns + file map | ✅ Shipped |
| "AI forgets tasks mid-conversation" | Persistent task tracker | ✅ Shipped |
| "Context window is too small" | 7-pass context compiler with DP knapsack | ✅ Shipped |
| "AI doesn't understand my codebase structure" | Code Skeleton Index + Call Graph | ✅ Shipped |
| "I don't know how much the AI is costing me" | Token Intelligence tracking | ✅ Shipped |
| "AI misses context from files I'm not editing" | Background Indexer + embedding search | ✅ Shipped |
| "AI breaks things it previously fixed" | Known issues tracker + proactive risk | ✅ Shipped |
| "My teammate's AI doesn't know our patterns" | Team Sync | ✅ Shipped |
| "AI can't find relevant code without being told" | Semantic similarity search over skeleton index | ✅ Shipped |
| "AI rewrites functions with its own logic" | Patterns + decision log enforcement | ✅ Shipped |

---

<p align="center">
  Made with ❤️ by <a href="https://www.linkedin.com/in/loudaini">Soufiane Loudaini</a> · <a href="https://digitalvize.com">DigitalVize LLC</a>
</p>