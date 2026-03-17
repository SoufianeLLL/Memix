# Memix Roadmap

> **Memix** is a persistent memory layer for AI coding assistants. This roadmap outlines planned features, improvements, and long-term vision — informed by real developer pain points from the community.

---

## Released — v1.0.0-beta.1

- ✅ Persistent brain storage with Redis
- ✅ Session tracking and continuity
- ✅ Health monitoring and smart pruning
- ✅ Team Sync (push, pull, merge)
- ✅ Import / Export brain data
- ✅ Secure credential storage (OS keychain)
- ✅ IDE auto-detection (VS Code, Cursor, Windsurf, Antigravity)
- ✅ Brain Monitor sidebar panel
- ✅ Auto-generated rules files for all IDEs
- ✅ File-based brain sync (`.memix/brain/*.json`)
- ✅ Prompt Pack preview + copy to clipboard

---

## Released — v1.0.0-beta.2 (Native Core)

- ✅ **Lightning-Fast Native Core**: Completly rebuilt Memix's backend memory engine using native compiled Rust.
- ✅ **Autonomous Codebase Observation**: Watch changes dynamically via AST `tree-sitter` integration.
- ✅ **Offline Semantic Intelligence**: Local embedding-backed semantic retrieval keeps memory search private and on-device.
- ✅ **Code DNA & Dependency Mapping**: O(1) Adjacency matrices and native cyclomatic complexity analysis based on edit-distance.
- ✅ **Advanced Predictor & Caching**: Pre-loaded memory context via native `DashMap` concurrency caching.
- ✅ **Ironclad Security & Team Sync**: CRDTs (Conflict-free Replicated Data Types) for team context sync and `AES-256-GCM` key resting storage.
- ✅ **Git Archaeology**: Native `git2` tracking of file churn stability.

---

## Released — v1.0.0-beta.4 (Daemon-Managed Mirror + Migration Safety)

- ✅ Daemon-managed JSON mirror (`.memix/brain/*.json`) with import/export endpoints.
- ✅ Pending writeback flow (`pending.json` -> validate/apply -> `pending.ack.json` -> clear).
- ✅ Embedding cache (content-hash keyed) to reduce repeated vector compute.
- ✅ Hybrid similarity scoring (cosine + keyword overlap).
- ✅ Observer/session endpoints and persisted observer artifacts.
- ✅ Migration framework + per-project migration endpoint and schema marker.
- ✅ Single-daemon PID lock to prevent concurrent daemon race on shared port.
- ✅ Atomic mirror writes (temp file + rename) to avoid partial JSON corruption.

---

## Released — v1.0.0-beta.5 (Intelligence & Language Expansion)

- ✅ **13 Languages Supported**: Added C#, Ruby, Swift, Kotlin, and PHP to the existing 8 (TypeScript, JavaScript, Rust, Python, Go, Java, C/C++) — complete with AST entity queries, cyclomatic complexity analysis, export detection, and pattern tagging.
- ✅ **7-Pass Context Compiler**: Dead context elimination → skeleton extraction → brain deduplication → history compaction → rules pruning → priority ranking → DP knapsack budget fitting. Optimal token allocation, not heuristic.
- ✅ **Multi-Factor Intent Engine**: Weighted voting system across 4 signal categories (file-path, structural edit ratios, pattern-tag prevalence, change volume) with calibrated confidence scores for 7 intent types.
- ✅ **World-Class Flight Recorder**: `parking_lot::Mutex` for fast-path locking, running O(1) analytics counters, `SessionAnalytics` with development velocity, hottest-files, intent distribution, and temporal queries.
- ✅ **Configurable Security Scanner**: Externalized rules to `memix-security.toml` with 10 default rules across critical/warning/info severity. User-customizable with workspace-root or `~/.memix/` override.
- ✅ **Prompt Replay & Optimization**: Learns from past prompt outcomes to suggest always-include sections, potentially excludable context, and calibrated token budgets per task type.
- ✅ **AI Model Performance Tracking**: Groups results by model × task type, computing first-try success rate and average token consumption.
- ✅ **Cross-Project Developer Profile**: Aggregates patterns, preferred stack, and code style across project boundaries.
- ✅ **Brain Hierarchy (CSS-Cascade)**: Layer-based memory resolution with priority merging and array concatenation — supports monorepo-style inheritance.
- ✅ **TokenEngine Caching**: Process-wide `once_cell::Lazy` BPE tokenizer — zero reinitialization overhead.
- ✅ **CI/CD Pipeline Fixes**: Resolved duplicate artifact naming for macOS targets, added missing artifact download step, and improved release note generation.

---

## Near-Term

### Multi-Project Daemon (Priority)
Run a single daemon instance that can attach multiple workspaces/projects safely and concurrently.

Planned scope:
- Project registry with explicit attach/detach lifecycle.
- Per-project watcher tasks and per-project backpressure.
- Hard isolation for storage and mirror paths per project.
- Project-scoped health/status endpoint slices for observability.

### Phase 1 Hardening Completion
- PID lock hardening + stale lock recovery (single-daemon safety).
- Full Brain CRUD + purge guardrails.
- End-to-end vector retrieval path validation and tuning for the local hybrid similarity flow.

### The "Web LLM Bridge" (Export Brain for AI)
A magical feature for Claude and ChatGPT web users. Export your entire Memix Brain and core project context into a single, highly compressed structure. Just drag and drop it into any web LLM, and it instantly possesses 100% of your complex, local IDE context.

### Cross-IDE Brain Portability (Zero-Config)
Use the exact same brain across Cursor, Windsurf, Antigravity, and VS Code simultaneously. We use secure root-path hashing so the `projectId` remains strictly consistent, perfectly syncing your project's memory regardless of which editor you open.

### Context Budget Optimizer
Intelligently compress brain context to fit within the AI's token limit without dumping all the data.

### Persistent Task Tracking
Ship a first-class daemon-backed task system instead of relying only on generated rules and prompt conventions.

### Built-in Voice Commands
Expose real voice-triggered actions in the extension instead of only documenting command phrases in generated rules.

### Phase 2 Launch Features
- Semantic diff as autonomous memory trigger (AST signature-change detection -> warning memory).
- Token-budget context packing (signal/token ratio first).
- CRDT sync foundation with **Automerge** as the default collaborative model for team sync.

### DriftDetector Agent
Autonomous agent that compares brain entries against the live filesystem to detect stale context. Triggers on interval to identify deleted files referenced in memory, package.json dependency mismatches, and entries that reference renamed/moved exports.

### DeadCodeDetector Agent
Graph-walk agent that uses the dependency graph to find zero-importer exports, unreferenced files, orphaned types, and unused utility functions. Runs on interval or workspace-open and generates cleanup recommendations.

### BrainJanitor Agent
Automated brain garbage collection agent. Prunes stale observation logs, archives old issues, compacts decision entries, verifies JSON health, and removes entries for files that no longer exist.

### Monorepo Context Inheritance
Automatic workspace detection for monorepos (`packages/*/`, `apps/*/`). Auto-builds the `BrainHierarchy` layer stack from workspace structure so child packages inherit parent context without manual configuration.

### Conversation Compactor
Structured extraction of facts, decisions, corrections, and implicit preferences from AI conversation history. Feeds extracted knowledge back into the brain so context compounds across chat sessions, not just file saves.

### Cursor-Position-Aware Context Retrieval
Resolve context at the scope level, not just file level. Follow `current_scope.referenced_symbols()` and `resolved_types()` to build precise, function-local context packages for the AI.

### Proactive Warning Extension Popup
Wire the existing `/proactive/risk` endpoint into the VS Code extension via `onDidChangeActiveTextEditor` and `onDidOpenTextDocument` listeners. Show risk warnings as non-intrusive popups when developers open or switch to high-risk files.

---

## Mid-Term

### Phase 3 Knowledge Graph Intelligence
- Causal memory graph (`caused_by`/`enables`) for reasoning-chain retrieval.
- Contradiction/supersession links so historical truth is preserved without AI confusion.
- Flight recorder timeline as auditable AI-context/debug surface.
- Intent detection from edit-pattern deltas (not only open-file heuristics).

### Team Telemetry / Brain Analytics Dashboard
A powerful web application connecting directly to your Redis instance. It visualizes team-wide brain health, decision speed, sync frequency, and shows which team members are contributing the most critical architectural context.

### Direct Chat API Integration
A dedicated chat UI running entirely inside your IDE, managed by our high-performance native core, to handle API streaming and local context injection autonomously.

### Intelligent Brain Merging & Conflict Resolution
Offline updates imported from web LLMs are intelligently merged using fuzzy matching and conflict rules. Real-time brain sync resolves team conflicts smoothly instead of silently overwriting.

---

## Long-Term

### AI Memory Compression (RAG)
For very large projects, use retrieval-augmented generation to index brain data. Instead of loading everything, the AI queries the most relevant brain fragments based on the current conversation context.

### Team Brain Analytics Dashboard
A web dashboard showing team-wide brain health, sync frequency, decision velocity, and which team members contribute most to shared context.

---

## Community-Driven Features

These features address the most common frustrations developers report with AI coding assistants:

| Problem (from Reddit & community) | Memix Solution | Status |
|---|---|---|
| "AI wakes up with amnesia every session" | Persistent brain with boot sequence | ✅ Shipped |
| "I keep re-explaining my project" | Identity + patterns + file map | ✅ Shipped |
| "AI rewrites my functions with its own logic" | Patterns enforce coding style | ✅ Shipped |
| "AI forgets tasks mid-conversation" | **Persistent task tracker** | 🟡 Upcoming |
| "AI ignores my rules files" | Guard template with anti-drift enforcement | ✅ Shipped |
| "Context window is too small" | 7-pass context compiler with DP knapsack | ✅ Shipped |
| "No memory between chat sessions" | Session log + continuity | ✅ Shipped |
| "AI can't remember architectural decisions" | Decisions key (append-only) | ✅ Shipped |
| "My teammate's AI doesn't know our patterns" | Team Sync | ✅ Shipped |
| "AI breaks things it previously fixed" | Known issues tracker | ✅ Shipped |
| "JSON File reads slow, I want Redis speed" | Redis HTTP API | ✅ Shipped |

---

## Contributing Ideas

Have a feature request or pain point that Memix should solve? [Open an issue](https://github.com/SoufianeLLL/Memix/issues) with the `feature-request` label.

---

<p align="center">
  Made with ❤️ by <a href="https://www.linkedin.com/in/loudaini">Soufiane Loudaini</a> · <a href="https://digitalvize.com">DigitalVize LLC</a>
</p>
