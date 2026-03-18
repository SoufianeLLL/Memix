<p align="center">
  <img src="media/memix-logo.png" width="80" alt="Memix Logo" />
</p>

<p align="center">
  <a href="https://marketplace.visualstudio.com/items?itemName=digitalvizellc.memix"><img src="https://img.shields.io/visual-studio-marketplace/v/digitalvizellc.memix?label=VS%20Marketplace&color=0078D4&logo=visual-studio-code" alt="VS Marketplace" /></a>
  <a href="./main/daemon"><img src="https://img.shields.io/badge/daemon-v0.1.0--beta-green?style=flat" alt="Daemon version" /></a>
  <a href="./main/extension"><img src="https://img.shields.io/badge/extension-v1.0.0--beta--5-orange?style=flat" alt="Extension version" /></a>
</p>

<h1 align="center">
  Memix — The Autonomous AI Memory Bridge
</h1>

Memix isn't just another AI coding assistant. It's an **autonomous intelligence layer** that runs silently in the background of your IDE, continuously observing, predicting, and remembering everything about your codebase. 

While other tools wait for you to ask a question, Memix is already understanding the context, mapping your architecture, and preparing the exact codebase memories your AI needs before you even open the chat.

<br/>
<p align="center">
  <a href="./daemon/LICENSE-BSL.md"><img src="https://img.shields.io/badge/Daemon%20License-BSL%201.1-black?style=flat" alt="Daemon license" /></a>
  <a href="./extension/LICENSE"><img src="https://img.shields.io/badge/Extension%20License-MIT-black?style=flat" alt="Extension license" /></a>
</p>
<br/>

## Requirements

- **A Redis instance is required** to store your project brain.
- VS Code (or compatible forks: Antigravity, Cursor, Windsurf, Claude Code) with the Memix extension installed.

## Local daemon development

```bash
bash scripts/download_model.sh
cargo build
```


<br/><br/>
## Features

### Persistent Project Brain (Redis-backed)
Stores identity, session state, tasks, decisions, patterns, file map, and known issues so your AI resumes with continuity.

### Daemon-Managed JSON Mirror (`.memix/brain/*.json`)
Every successful brain write is mirrored to workspace-local JSON files. This gives your AI and tooling an instant, file-based read path without direct Redis access.

### Safe AI Writeback via `pending.json`
AI agents can propose writes by creating `.memix/brain/pending.json`. The daemon validates, applies, writes `pending.ack.json`, and clears pending input.

### AST-driven Code DNA + Architecture Explainability
Memix computes project DNA from Tree-Sitter ASTs across **13 languages** (TypeScript, JavaScript, Rust, Python, Go, Java, C/C++, C#, Ruby, Swift, Kotlin, PHP). That means language-aware cyclomatic complexity, structural pattern tagging, exported symbol detection, architecture inference, hot-zone ranking, circular risk detection, and a short explainability summary you can inject directly into AI context.

### Configurable DNA Rules Per Project
Observer DNA can be tuned with workspace-local overrides in `dna_rules.toml`, letting you classify custom folders, tag proprietary patterns, and handle project-specific edge cases without changing the daemon code.

Config discovery order:
- Workspace root: `dna_rules.toml`
- Workspace fallback: `.memix/dna_rules.toml`
- User override: `~/.memix/dna_rules.toml`

Start from `dna_rules.toml.example`.

### Autonomous Codebase Observation
Memix quietly monitors your active workspace. As you scaffold new files, refactor architecture, or fix bugs, the Memory Engine natively understands the intent behind your changes. It maps how your functions connect together in real-time.

### Zero-Latency Context Routing
Tired of manually highlighting code so your AI understands what you're talking about? Memix pre-loads the exact files, dependencies, and historical decisions into your AI's context window. When you ask a question, the AI already knows the answer.

### Ironclad Privacy & Offline Execution
Your code never leaves your machine unless you explicitly prompt your AI model. Memix operates entirely natively on your hardware. It builds lightning-fast semantic graphs, vector indexes, and structural models running at purely native performance.

### Seamless Team Sync
Memix ensures your entire developer team shares the exact same codebase "brain." Architectural decisions, API boundaries, and feature plans sync effortlessly without conflict, ensuring every developer—and every AI agent—is on the identical page.

### One-click Health Check
Validates required brain keys, detects invalid shapes, staleness, and oversized entries.

### Brain Key Coverage (Advanced)
Shows which brain keys exist, their sizes, and their taxonomy so you can quickly spot missing categories before an AI session.

### Prompt Pack Preview + One-click Copy (Advanced)
Generates a ready-to-paste context bundle (identity, session state, patterns, decisions, known issues, tasks, file map). Includes a token estimate so you can fit it into any model’s context window.

### Mirror Import / Export + Migrations
- Daemon endpoints for full mirror export/import.
- Schema migrations endpoint for project backfills (for example: vector backfill).
- Helps keep old projects up to date as Memix capabilities evolve.

### Token utilities (daemon)
Exact token counting and budget-based context selection.

### Context Compiler (daemon)
Memix compiles a task-focused context packet from the active file, recent history, project rules, and brain state using a **7-pass optimization pipeline** with DP knapsack budget fitting. This keeps prompts smaller, more relevant, and optimally allocated — not heuristic.

### AGENTS Runtime (daemon)
`AGENTS.md` now drives daemon-side autonomous agents that run independently of the chat model. Memix can parse agent definitions, execute supported triggers, and persist agent reports for later AI or developer consumption.

### Proactive Risk Analysis + Configurable Security Scanner
Before risky edits, Memix scores a file using dependents, known issues, past breakage signals, and stability indicators. The built-in security scanner loads configurable rules from `memix-security.toml` (10 default rules across critical/warning/info severity).

Config discovery order:
- Workspace root: `memix-security.toml`
- Workspace fallback: `.memix/memix-security.toml`
- User override: `~/.memix/memix-security.toml`

### Learning Layer + Cross-Project Profile
Memix can learn from prompt outcomes, compare model performance by task type, and derive a cross-project developer profile so future context assembly gets better over time.

### Brain Hierarchy / Context Inheritance
Layered brain resolution supports parent-child context inheritance for monorepos and nested project structures.

### Observer + Session APIs
- `/api/v1/observer/graph`
- `/api/v1/observer/changes`
- `/api/v1/observer/dna`
- `/api/v1/observer/dna/otel`
- `/api/v1/session/current`
- `/api/v1/session/replay`

### New intelligence APIs
- `/api/v1/context/compile`
- `/api/v1/agents/config`
- `/api/v1/agents/reports`
- `/api/v1/proactive/risk`
- `/api/v1/learning/prompts/:project_id/optimize`
- `/api/v1/learning/model-performance/:project_id`
- `/api/v1/learning/developer-profile`
- `/api/v1/brain/hierarchy/resolve`

### Advanced panel UX for large payloads
Large artifacts like Prompt Pack and Observer DNA OTel export are presented as action-first summaries with modal detail views and copy actions instead of dumping raw JSON inline.

### Local daemon over Unix Socket + TCP
A local Axum (Rust) daemon powers memory APIs, rules generation, observer snapshots, migrations, and mirror sync.

### Smarter Similarity Search
Hybrid similarity now combines normalized vector similarity (cosine) with keyword overlap for stronger relevance.

### Local Embeddings with Safe Fallback
`all-MiniLM-L6-v2` is bundled into the daemon binary via `fastembed`, so semantic similarity works without a first-run model download. If ONNX Runtime is unavailable at runtime, the daemon safely falls back to deterministic dummy embeddings instead of failing requests.

<br/>

## Hard advantages (why Memix improves AI-chat workflows)

- **Prompt Pack (copy/paste ready)**
  A curated, structured bundle you can paste into any AI chat to eliminate “re-explain the repo” prompts.

- **Token-aware context**
  Memix can estimate tokens for your Prompt Pack so you can stay under model limits.

- **Decision guardrails**
  Persist architectural decisions and feed them forward so the AI stops re-debating solved choices.

- **Task-aware prompting**
  Session state + tasks keep the AI aligned with what you’re actually doing right now.

- **Mirror-first reliability**
  Workspace-local JSON mirror ensures low-latency reads and resilient workflows even when network/Redis conditions fluctuate.

- **Migration-safe evolution**
  As memory schema and embedding capabilities improve, migration hooks keep existing projects consistent instead of drifting.

- **Architecture-aware AI context**
  Code DNA summaries give your AI a compact briefing of architecture style, hot zones, dependency depth, rule-derived patterns, and circular dependency risks before it starts making edits.

- **Proactive change safety**
  Risk scoring and known-issue surfacing help developers identify dangerous files before making edits that ripple through the project.

- **Autonomous background intelligence**
  The daemon can prepare agent reports, optimized context, model-performance summaries, and hierarchy-aware memory resolution before the AI is even asked a question.

<br/>

## Why Memix matters

 Memix is not only about remembering facts. It is designed to make AI-assisted coding materially better across the full development loop.

 - **Faster coding**
   Memix reduces the time you spend re-explaining your project to AI. The daemon observes structure, recent changes, architectural decisions, and active work areas so the AI starts closer to the real context.

 - **Better code quality**
   By preserving project decisions, patterns, file roles, dependency relationships, and Code DNA summaries, Memix helps the AI produce changes that fit the codebase instead of generic answers that fight your architecture.

 - **Less context loss**
   In long sessions, after IDE restarts, or across multiple contributors, Memix keeps continuity through persistent brain state, task tracking, project mirrors, and observer snapshots.

 - **Safer architectural changes**
   Memix surfaces hot zones, dependency depth, circular risks, and active development areas. That gives the AI and the developer better visibility into where edits are risky and where changes are likely to ripple.

 - **More useful than memory alone**
   Traditional memory tools mostly store notes. Memix also watches the codebase itself, builds structural intelligence from ASTs, exposes observer APIs, and prepares compact explainability summaries that can be injected directly into AI workflows.

 - **Higher leverage for teams**
   Memix helps multiple developers and AI agents work from the same shared understanding of architecture, conventions, tasks, and known issues instead of rebuilding that understanding from scratch every session.

 In short: Memix helps you code faster, keep quality higher, preserve architectural consistency, and make AI assistance feel less stateless.

 <br/>

 ## Installation

 1. Install **Memix** from the VS Code Extensions Marketplace.
 2. Open a workspace folder.
3. Run `Memix: Connect Redis` then `Memix: Initialize Brain`.
4. Optional operations from **Memix Settings**:
   - `Export Brain Mirror`
   - `Import Brain Mirror`
   - `Run Brain Migrations`


<br/>

## Documentation
For detailed information regarding updates, security, and future plans, please refer to the internal documentation:
- [Getting Started](./docs/GETTING_STARTED.md)
- [Security Policy](./docs/SECURITY.md)
- [Roadmap](./docs/ROADMAP.md)
- [Changelog](./docs/CHANGELOG.md)

## Support the project

If Memix saves you time, please support the project:

- Star the repo
- Share it with your team
- Open issues with actionable repro steps

## License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## License

- The VS Code extension and GitHub-facing repository assets are licensed under the [MIT License](./extension/LICENSE).
- The Rust daemon is licensed under the [Business Source License 1.1](./daemon/LICENSE-BSL.md).

If you are evaluating reuse or redistribution, check the license file for the specific component you are using.
