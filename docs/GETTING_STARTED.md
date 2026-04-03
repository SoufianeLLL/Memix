# Getting Started

## Requirements

- **A Redis instance is required** — Memix stores your project brain in Redis. Any Redis-compatible provider works: local Redis, Upstash, Redis Cloud, or AWS ElastiCache. Free tiers on Upstash work well for individual developers.
- VS Code `1.107+` or a compatible fork (Cursor, Windsurf, Antigravity, Claude Code).
- Platform support: macOS 12+, modern Linux x64, Windows x64.

No plugins or AI-side configuration required — Memix works through each IDE's native rules and instructions mechanism.

## Install

1. Install **Memix** from the VS Code Marketplace.
2. Open a workspace folder. Memix is workspace-scoped — one brain per project root.

On first activation, the extension fetches the daemon manifest from the Memix release channel, downloads the correct platform binary into VS Code global storage, verifies its SHA-256 checksum, and launches it. The daemon is updated automatically in the background when new versions are published — you will see a "Reload Window" prompt when an update is staged and ready.

## First-Time Setup

### 1 — Connect to Redis

From the Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`):

```
Memix: Connect Redis
```

Enter your Redis connection URL (e.g. `redis://localhost:6379` or `rediss://default:password@host:port`). The URL is stored securely in your operating system's native keychain via VS Code Secret Storage — it is never written to disk in plaintext.

### 2 — Initialize the Brain

```
Memix: Initialize Brain
```

This generates the AI rules files for your IDE (the correct format for Cursor, Windsurf, Claude Code, Antigravity, or VS Code Copilot is auto-detected), creates the initial brain keys in Redis (identity, session state, patterns, tasks, decisions, file map, known issues), and writes the `AGENTS.md` file to your workspace root for tools that follow that convention.

### 3 — Open the Brain Monitor

```
Memix: Open Debug Panel
```

Or click the Memix status bar item. The panel shows live brain state, observer data, session timeline, skeleton index stats, token intelligence metrics, and the compiled context for the currently active file.

## What Happens at Startup

Once the daemon is running and connected to Redis, several things happen automatically without any user action.

The **file watcher** starts observing your workspace immediately. Every file save triggers the three-layer analysis pipeline: tree-sitter AST parsing (all 13 supported languages), OXC semantic enrichment for TypeScript and JavaScript (import resolution, resolved call graph), and embedding computation using the bundled AllMiniLM-L6-v2 model.

The **background indexer** starts 5 seconds after daemon launch and walks the entire workspace at a throttled pace (10 files per second by default). This is the one-time cost of building the full Code Skeleton Index — similar to TypeScript's language server indexing process. On subsequent daemon restarts, the index is loaded from the persisted binary file at `.memix/skeleton_embeddings.bin` in milliseconds.

The **token tracker** begins recording session statistics: context compilations, tokens compiled by Memix, tokens sent to AI, and the estimated savings from structural compression versus a naive paste approach. Both the EmbeddingStore and TokenTracker start empty and load in the background, so the panel becomes responsive within 1–2 seconds even on slow Redis connections, with data appearing progressively as the background tasks complete.

## Multi-Window / Multi-Project Support

Memix supports multiple VS Code windows and projects simultaneously. When you open a second project:

1. The extension registers the new workspace with the running daemon
2. A background indexer spawns independently for that project
3. Switching between windows instantly activates the corresponding workspace
4. No daemon restart is needed — the single daemon instance handles all projects

Each project maintains its own brain in Redis (keyed by project ID) and its own `.memix` folder in the workspace root. The daemon tracks which window is active and prioritizes background indexing for the focused project.

## How It Works (Architecture Summary)

Memix runs as a local Rust daemon and communicates with the extension over a Unix domain socket (`~/.memix/daemon.sock` on macOS and Linux) or TCP on port 3456 (Windows and developer mode).

The daemon maintains several continuously updated indexes:

- **Project brain** — structured JSON entries in Redis covering identity, current task, patterns, decisions, known issues, and session history.
- **Dependency graph** — a live directed graph mapping file-level import relationships, updated on every save.
- **Call graph** — a resolved function-to-function call index, enriched with exact file paths and line numbers for TypeScript/JavaScript via OXC.
- **Code Skeleton Index** — one FSI entry per file and FuSI entries per function for hot files, stored in a separate Redis hash and persisted to a local binary file with embeddings.
- **Code DNA** — an aggregate architectural summary derived from AST patterns across the entire project.

The **Context Compiler** consumes all of the above through a seven-pass optimization pipeline to produce a compact, high-signal context packet for any given active file and task type. The output is what gets injected into AI prompts — not raw file dumps, but a precisely budget-fitted selection of the most relevant structural information.

The **JSON mirror** at `.memix/brain/*.json` gives AI agents direct file-read access to brain entries without requiring Redis access. The daemon writes every brain change to this mirror atomically. AI agents using the pending.json writeback protocol should always read from these files before writing, to avoid overwriting fields they didn't intend to change.

## Brain Data Structure

The following brain keys are created during initialization and updated as you work:

- `identity.json` — project name, purpose, tech stack, architecture overview
- `session_state.json` — current task, progress, blockers, next steps, modified files
- `patterns.json` — code style conventions, naming preferences, architectural rules
- `decisions.json` — append-only log of architectural decisions and rationale
- `file_map.json` — key files and their purposes
- `known_issues.json` — tracked bugs, tech debt, and warnings
- `tasks.json` — persistent task lists with status history
- `session_log.json` — per-session summaries appended at session end

These files live at `.memix/brain/` in your workspace root. They are gitignored by the rules generator.

## Debug Panel Sections

The Brain Monitor panel has two tabs: Brain and Advanced.

The **Brain** tab shows health status, brain size in KB, Redis memory usage, key count, session number, last updated timestamp, current task, and any warnings or recommendations.

The **Advanced** tab includes:

- **Brain Key Coverage** — a checklist of all brain keys with sizes and readiness status. Required keys are separated from recommended and generated ones.
- **Prompt Pack** — a curated context bundle ready to paste into any AI chat. Includes an approximate token estimate and a copy button. This is especially useful in chat-only AI tools that don't automatically read Memix state.
- **Compiled Context** — the context compiler's output for the active file, showing which sections were selected, how many tokens each occupies, and how much budget was used.
- **Token Intelligence** — session and lifetime token metrics: context compilations, tokens compiled by Memix, tokens sent to AI, per-call breakdown (last, max, min, average), estimated tokens saved, and embedding cache efficiency.
- **Observer DNA** — the Code DNA summary including architecture style, complexity distribution, hot/stable zones, circular dependency risks, and language breakdown.
- **Daemon Agents** — the currently loaded `AGENTS.md` configuration and recent background agent reports.
- **Proactive Risk** — file risk scoring for the active file using dependency analysis, known issues, Code DNA stability, and git churn data.
- **Daemon Timeline** — the flight recorder's session event stream with intent detection and AST mutation history.
- **Git Archaeology** — hot/stable file classification and recent contributor data from git history.
- **Predictive Intent** — the current intent classification (scaffolding, bug fixing, refactoring, etc.) with confidence score and rationale.
- **Learning Layer** — prompt optimization suggestions, model performance summaries, and the cross-project developer profile.
- **Hierarchy Resolution** — the resolved context inheritance stack for monorepo setups.

## Development: Run the Daemon Separately

When developing the extension with F5, keep the daemon running as a standalone process to avoid the extension spawning a competing instance.

Download the embedding model first, then build:

```bash
cd daemon
bash scripts/download_model.sh
cargo run
```

Tell the extension not to spawn its own daemon:

```bash
# Option A: VS Code setting
# memix.dev.externalDaemon: true

# Option B: environment variable or .env file
MEMIX_DEV_EXTERNAL_DAEMON=true
```

See the Daemon Development Guide (`DAEMON_DEVELOPMENT.md`) for the full environment variable reference and troubleshooting steps.

## Writeback Protocol for AI Agents

AI agents proposing brain updates should follow this protocol precisely to avoid data loss:

1. Read the current `.memix/brain/<key>.json` file before writing.
2. Merge changes into the complete existing object — never write a partial object that omits existing fields.
3. Write the merged result to `.memix/brain/pending.json` with the correct schema (top-level `project_id`, `upserts` array, `deletes` array, with each upsert entry also carrying `project_id`).
4. Wait for `.memix/brain/pending.ack.json` to confirm the daemon processed the update.

Writing a partial update silently destroys fields that existed before. The daemon does not merge — it replaces the entry with whatever the upsert contains.

---

## Commands reference

All commands are accessible via the **Command Palette** (`Cmd+Shift+P` / `Ctrl+Shift+P`) or from the **Brain Monitor** panel's action dropdown.

### Connect Redis

```
Memix: Connect Redis
```

Connects Memix to your Redis database. On first use, you'll be prompted for your connection URL. The URL is securely stored in your operating system's native keychain — so you only need to enter it once, and it's never saved in plaintext.

**When to use:** The first time you set up Memix, or after running "Clear Stored Secrets."

### Initialize Brain

```
Memix: Initialize Brain
```

Creates the foundational data structure in Redis for your project. You'll be prompted to enter a **Project Name** — this becomes the identity of your brain. Memix also auto-generates a rules file so your AI assistant can read the brain context automatically.

**When to use:** Once per project, the very first time you use Memix in a workspace.

**What it creates:**
- **Identity** — Your project's name and purpose
- **Session State** — Tracks your current task, session number, and last activity
- **Patterns** — A place to record recurring patterns, preferences, and architectural rules

### Disconnect Redis

```
Memix: Disconnect Redis
```

Disconnects Memix from the current Redis instance. Your brain data is safely preserved in Redis — disconnecting simply stops the live connection.

### Open Debug Panel

```
Memix: Open Debug Panel
```

Opens the **Brain Monitor** sidebar panel. This gives you a real-time dashboard showing:

- **Health Status** — Is your brain healthy, at warning level, or critical?
- **Brain Size** — How much data your brain is using (in KB)
- **Redis Memory** — A visual progress bar of your Redis instance's memory usage
- **Key Count** — How many brain categories are populated
- **Session Number** — Which session you're on
- **Last Updated** — Relative time since the brain was last modified (e.g., "3 minutes ago")
- **Current Task** — What the AI last recorded as the active task
- **Warnings & Recommendations** — Actionable alerts about missing data, capacity limits, or corruption

### Using Prompt Pack with AI chat

If you’re working in an AI IDE/chat that doesn’t automatically read Memix state, you can:

1. Open **Brain Monitor → Advanced → Prompt Pack**
2. Optionally use **View** to inspect the payload in the modal
3. Click **Copy**
4. Paste into your AI chat as the first message for the session

This dramatically reduces back-and-forth and prevents the AI from guessing project context.

> Note: Some Redis providers do not expose an explicit memory cap via Redis commands.
> In that case, the Redis dataset max may be shown as an estimate so the progress bar remains meaningful.

### Export Brain

```
Memix: Export Brain
```

Saves your entire brain to a `.json` file in your workspace. This is perfect for:

- **Backups** before major changes
- **Migrating** brain data to a different machine
- **Sharing** brain context with a colleague

### Import Brain

```
Memix: Import Brain
```

Restores brain data from a previously exported `.json` file. A file picker dialog lets you choose the backup file, and all keys are imported into your current Redis brain.

### Health Check

```
Memix: Health Check
```

Runs a comprehensive diagnostic on your brain. It checks for:

- **Missing required keys** (identity, session state, patterns)
- **Oversized entries** approaching capacity limits
- **Corrupted or invalid data** that may confuse your AI
- **Stale data** that hasn't been updated in 72+ hours

Results are shown as an information / warning / error notification, depending on severity.

### Prune Brain

```
Memix: Prune Brain
```

Intelligently trims oversized brain data to keep it fast and within your token budget. Pruning targets:

- Old session log entries
- Duplicate or redundant pattern data
- Entries that exceed the configured maximum key size

**When to use:** When health checks show capacity warnings, or periodically to maintain optimal performance.

### Clear Brain

```
Memix: Clear Brain
```

**Permanently deletes** all brain data for the current project. A confirmation dialog prevents accidental data loss. This is a destructive action — use Export Brain first if you want a backup.

### Team Sync Setup

```
Memix: Team Sync Setup
```

Enables collaborative brain sharing. Enter a **Team ID** (a shared identifier your teammates also use), then choose an action:

- **Push to team** — Upload your brain data to the shared team namespace
- **Pull from team** — Download brain data that teammates have pushed
- **Merge decisions** — Intelligently merge architectural decisions from the team brain into your local brain

Your Team ID is stored securely in your OS keychain. Share it privately with teammates.

### Recover Corrupted Brain

```
Memix: Recover Corrupted Brain
```

Attempts to repair brain data that has become corrupted (e.g., invalid JSON, broken references). It resets malformed entries to valid empty states without deleting healthy data.

### End Session

```
Memix: End Session
```

Marks the current working session as complete. This:

1. Saves your session score (how productive the session was)
2. Logs the session summary to the session log
3. Increments the session counter
4. Resets progress trackers for the next session

**When to use:** At the end of a coding session, before closing your editor.

### Clear Stored Secrets

```
Memix: Clear Stored Secrets
```

Removes your stored Redis URL and Team ID from the operating system's secure keychain. After running this, you'll be prompted to re-enter your credentials on the next connection.

**When to use:** If you need to switch Redis instances or want to fully reset your Memix credentials.

---

## What If the AI Forgets to Save?

Some generated rules files include recommended phrases you can type in chat to remind an AI assistant to reload or update Memix context. These are prompt conventions, not native voice-command features built into the extension or daemon.

| Say This | What Happens |
|---|---|
| **save brain** | Prompts the assistant to refresh or persist Memix state if its workflow supports that |
| **brain status** | Prompts the assistant to summarize current Memix context |
| **recap** | Prompts the assistant to restate the important current context |
| **follow protocol** | Prompts the assistant to re-check the generated Memix rules |
| **reboot brain** | Prompts the assistant to reload Memix context after interruptions |
| **end session** | Prompts the assistant to wrap up the current session cleanly |
