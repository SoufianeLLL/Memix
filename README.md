<p align="center">
  <img src="media/memix-logo.png" width="80" alt="Memix Logo" />
</p>

<p align="center">
  <a href="https://marketplace.visualstudio.com/items?itemName=digitalvizellc.memix"><img src="https://img.shields.io/visual-studio-marketplace/v/digitalvizellc.memix?label=VS%20Marketplace&color=0078D4&logo=visual-studio-code&style=for-the-badge" alt="VS Marketplace" /></a>
  <img src="https://img.shields.io/badge/BUILT WITH-RUST / REDIS / TYPSCRIPT-blue?style=for-the-badge" alt="Daemon version" />
  <img src="https://img.shields.io/badge/INTEGRATED MODELS-AllMiniLM--L6--v2-yellow?style=for-the-badge" alt="Daemon version" />
  <a href="./daemon"><img src="https://img.shields.io/badge/daemon-v0.11.5-emerald?style=for-the-badge" alt="Daemon version" /></a>
  <a href="./extension"><img src="https://img.shields.io/badge/extension-v1.8.7-orange?style=for-the-badge" alt="Extension version" /></a>
</p>

<h1 align="center">
  Memix — The Autonomous AI Memory Bridge
</h1>

Memix is an **autonomous engineering intelligence layer** that runs silently in the background of your IDE, continuously observing your codebase, building a live structural model of it, and preparing precisely the right context for your AI assistant before you even open the chat.

While other AI memory tools store notes about what you've told them, Memix understands your code structurally — at the AST level, the dependency graph level, and the function call level — through a continuously updated index that no cloud service can replicate.

<br/>
<p align="center">
  <a href="./daemon/LICENSE-BSL.md"><img src="https://img.shields.io/badge/Daemon%20License-BSL%201.1-black?style=for-the-badge" alt="Daemon license" /></a>
  <a href="./extension/LICENSE"><img src="https://img.shields.io/badge/Extension%20License-MIT-black?style=for-the-badge" alt="Extension license" /></a>
</p>
<br/>

## Requirements

- A **Redis instance** to store your project brain. Upstash free tier works well for individual developers.
- VS Code 1.107+ or a compatible fork: Cursor, Windsurf, Antigravity, or Claude Code.

## Quick Start

```bash
# From the VS Code Marketplace
# 1. Install Memix
# 2. Open a workspace folder
# 3. Run: Memix: Connect Redis
# 4. Run: Memix: Initialize Brain
```

For building from source:

```bash
cd daemon
bash scripts/download_model.sh  # downloads AllMiniLM-L6-v2
cargo build --release
```

<br/>

## What Memix Does

### Persistent Project Brain (Redis-backed)
Stores identity, session state, tasks, decisions, patterns, file map, and known issues across every working session. Your AI assistant resumes with full project context rather than starting from zero each time.

### Three-Layer Structural Intelligence
Every file save triggers three successive analysis passes that each add a different kind of understanding.

The first layer uses **tree-sitter AST parsing** across 13 supported languages (TypeScript, JavaScript, Rust, Python, Go, Java, C/C++, C#, Ruby, Swift, Kotlin, PHP) to extract function signatures, types, exports, call sites, cyclomatic complexity, and structural patterns.

The second layer uses **OXC semantic analysis** for TypeScript and JavaScript files to resolve import statements to their actual file paths, build a resolved call graph where each edge carries the callee's file and line number, and detect dead imports pointing to files that no longer exist. This turns a nominal dependency graph into a resolved one.

The third layer uses the **AllMiniLM-L6-v2 embedding model** — bundled into the daemon binary, no network required — to compute 384-dimensional vector representations of every skeleton entry. These embeddings power semantic similarity search across the entire codebase structure.

### Code Skeleton Index
Maintains a continuously updated structural map of the project. The **File Skeleton Index (FSI)** provides one entry per source file capturing its shape: language, types, functions with signatures and complexity, exports, imports, and dependency relationships. The **Function Symbol Index (FuSI)** provides per-function entries for hot files, enriched with call graph data. Both layers are stored in an isolated Redis hash and persisted to a local binary file with a write-through Redis mirror for cross-IDE sharing.

### Background Indexer
Builds the complete skeleton index for the entire workspace automatically at daemon startup, running at a throttled pace (10 files/second by default) so it never disrupts active development. After the first run, the index is restored from the binary file in milliseconds on every subsequent start.

### 7-Pass Context Compiler
Assembles a token-budget-fitted context packet from the structural index, dependency graph, brain entries, session history, and project rules. The compilation pipeline runs dead context elimination (BFS from active file), skeleton extraction, brain deduplication, history compaction, rules pruning, skeleton index injection with betweenness-centrality priority boosting, and optimal budget fitting via a 0/1 dynamic programming knapsack. The result is not a dump of raw files — it is a precisely curated selection of the most relevant structural information for the current task.

### Token Intelligence
Tracks three distinct token dimensions: tokens consumed by AI models, tokens compiled by the context compiler, and tokens estimated as saved through structural compression versus naive full-file injection. Session and lifetime totals are both maintained, with the estimated savings expressed as both a token count and an approximate dollar figure. The compression ratio — compiled tokens divided by the naive paste estimate — shows how much leverage Memix is providing on each context compilation.

### Dependency Graph with Structural Importance
Maintains a live directed dependency graph updated on every file save. Betweenness centrality (Brandes' algorithm) and PageRank are computed from the graph and applied as priority boosts in the context compiler. Blast radius analysis uses forward BFS through the reverse dependency graph to compute all files transitively affected when a given file changes, including critical path reconstruction.

### Resolved Call Graph
Tracks function-to-function call relationships with a dual-index architecture. When OXC resolves a call target to a specific file and line, the exact callee location is stored. When resolution is unavailable (dynamic dispatch, external libraries), a nominal name-only entry serves as a fallback. Both indexes are queried transparently — callers see the best available information without needing to know which tier answered the query.

### Autonomous Codebase Observation
The file watcher runs continuously on the workspace root. File saves trigger semantic diffs (not just line diffs), breaking signature detection across the old and new AST, intent classification using a multi-factor weighted voting engine, and dependency graph updates. Warning entries are created automatically for breaking signature changes, unresolved imports, and security scanner findings.

### AGENTS.md Protocol Support
Generates and maintains an `AGENTS.md` file in the workspace root following the convention used by Claude Code, Cursor, and other tools that auto-load agent protocol files. This ensures the AI assistant receives the Memix operating protocol, daemon API reference, and memory writeback instructions automatically without any manual copy-paste.

### Proactive Risk Analysis
Before invasive edits, scores a file's risk using its dependents in the dependency graph, Code DNA stability metrics, known issues, and git archaeology churn data. Available through the API and visible in the debug panel.

### Configurable Security Scanner
Loads rules from `memix-security.toml` (workspace root, `.memix/` fallback, or `~/.memix/` user override). Ships with 10 default rules across critical, warning, and info severity levels. Fully customizable per project without changing daemon code.

### Context DNA + Architecture Explainability
Computes a project-wide Code DNA summary from AST patterns across all supported languages: architecture style inference, hot and stable zone identification, circular dependency detection, type coverage ratio, language breakdown, and an explainability summary suitable for direct injection into AI context.

### Git Archaeology
Maintains deep git history context: file churn analysis (identifies hot and stable zones), recent author tracking, commit summaries, and structural change patterns. Combined with Code DNA, this provides a complete picture of which areas need careful handling versus which are safe for aggressive refactoring.

### Intelligent Decision Detection
Automatically observes code changes and records architectural decisions — capturing the **why** behind code evolution, not just the **what**. Uses a three-layer detection system: TOML-based rules (70+ pre-defined), AST pattern matching (tree-sitter), and embedding similarity (AllMiniLM-L6-v2). Decisions are stored in the brain with evidence chains, confidence scores, and rule provenance. User feedback adjusts rule confidence over time for self-improving detection accuracy.

### Decision Feedback Loop
Users can provide feedback on auto-detected decisions through the API (`POST /api/v1/decisions/:id/feedback`). Feedback types include useful, dismissed, and incorrect. Rules automatically adjust confidence based on accumulated feedback, enabling the system to learn from user corrections and improve detection quality over time.
Native git2 integration for tracking file churn and stability. Identifies hot files (frequently modified recently) and stable files (infrequently modified, low-risk to edit). Used as a signal in proactive risk scoring and as context for the intent engine.

### Learning Layer + Cross-Project Profile
Records prompt outcomes, compares model performance by task type, and derives a developer profile from patterns across projects. Surfaces context optimization suggestions specific to the current developer's working style.

### Brain Hierarchy (Monorepo Support)
Layer-based memory resolution supports parent-child context inheritance for monorepo structures. Child packages can override or extend inherited context from parent workspace layers.

### Multi-Tenant Workspace Support
Run Memix across multiple VS Code windows and projects simultaneously. A single daemon instance tracks all open workspaces through a workspace registry, spawning independent background indexers per project. Switching between projects is instant (~0ms) with no daemon restart needed. Window focus automatically activates the corresponding workspace for background indexing priority.

### Multi-IDE Support
Use Memix across multiple AI IDEs at the same time — VS Code, Cursor, Windsurf, Claude Code, Antigravity, and any VS Code-compatible fork. All IDEs share a single daemon binary stored at `~/.memix/bin/`, eliminating redundant downloads. The daemon tracks which workspace is active across all IDEs and routes API calls to the correct project context.

### Team Sync
CRDT-based brain synchronization for teams. Push, pull, and merge architectural decisions, patterns, and shared context across team members using a conflict-free replicated data type foundation.

### Daemon-Managed JSON Mirror
Every brain write is atomically mirrored to `.memix/brain/*.json` files in the workspace. AI agents can read current brain state from these files instantly without a daemon API call. The write path goes through the daemon's pending.json protocol to ensure validation, deduplication, and consistency.

<br/>

## Hard Advantages Over Existing Memory Tools

**Structural understanding, not just notes.** Other memory tools store what you've told them. Memix continuously derives structural facts from your code — call relationships, dependency depth, complexity distribution, export surfaces — that no amount of manual note-taking could replicate.

**Token-efficient context, not file dumps.** The 7-pass context compiler with DP knapsack fitting produces a context packet that is typically 5-10× smaller than a naive file paste with the same information content. The Token Intelligence system shows exactly how much is being saved.

**Offline, private, machine-local.** No code leaves your machine except what you explicitly paste into an AI chat. The AST analysis, embedding computation, dependency graph, and all structural indexes run entirely on your local hardware.

**Works with any AI model.** Because Memix pre-assembles context structurally before it reaches the AI, even cheap or local models perform well — they receive precisely curated, structurally complete prompts rather than being overwhelmed by raw file content.

**Survives IDE restarts and session breaks.** The brain persists across restarts in Redis. The skeleton index persists across restarts in the binary embedding file. Token intelligence lifetime totals persist in the workspace data directory. Nothing resets when you close and reopen the IDE.

<br/>

## Documentation

- [Getting Started](./docs/GETTING_STARTED.md)
- [Daemon Development Guide](./docs/DAEMON_DEVELOPMENT.md)
- [Context Orchestrator](./docs/CONTEXT_ORCHESTRATOR.md)
- [Code Skeleton Index](./docs/CODE_SKELETON_INDEX.md)
- [Context Compiler](./docs/CONTEXT_COMPILER.md)
- [Call Graph](./docs/CALL_GRAPH.md)
- [Dependency Graph](./docs/DEPENDENCY_GRAPH.md)
- [Semantic Analysis (OXC)](./docs/SEMANTIC_ANALYSIS.md)
- [Embedding Store](./docs/EMBEDDING_STORE.md)
- [Background Indexer](./docs/BACKGROUND_INDEXER.md)
- [Token Intelligence](./docs/TOKEN_INTELLIGENCE.md)
- [Redis Connection Pooling](./docs/REDIS_CONNECTION_POOLING.md)
- [Roadmap](./docs/ROADMAP.md)
- [Security Policy](./docs/SECURITY.md)
- [Changelog](./docs/CHANGELOG.md)

## License

The VS Code extension is licensed under the [MIT License](./extension/LICENSE).

The Rust daemon is licensed under the [Business Source License 1.1](./daemon/LICENSE-BSL.md), which converts to Apache 2.0 after four years.

<br/>

<p align="center">
  Made with ❤️ by <a href="https://www.linkedin.com/in/loudaini">Soufiane Loudaini</a> · <a href="https://digitalvize.com">DigitalVize LLC</a>
</p>