# Getting Started

## Requirements

- **A Redis instance is required** — Memix stores your project brain in Redis.
- VS Code `1.107+` (or compatible forks).
- For the published daemon binaries, macOS `12+`, modern Linux x64, and Windows x64 are currently targeted.

**No plugins or AI-side configuration required** — it works through the IDE's native rules/instructions mechanism.

## Install

1. Install **Memix** from the VS Code Marketplace.
2. Open a workspace folder (Memix is workspace-scoped).

On first activation, the extension ensures the correct daemon binary is available for your platform. The daemon binary is downloaded into VS Code global storage, verified by checksum, and then launched locally.

## First-time setup

### 1) Connect to Redis

Run from the Command Palette:

- `Memix: Connect Redis`

You’ll be prompted for a Redis connection URL (for example `redis://localhost:6379`). The URL is stored securely using VS Code Secret Storage (OS keychain), not in plaintext settings.

### 2) Initialize the brain

Run:

- `Memix: Initialize Brain`

This will:

- Choose or generate a project identifier for the workspace.
- Generate AI rules files for the IDE you’re running (Cursor/Windsurf/Claude Code/Antigravity/VS Code compatible).
- Create the initial brain keys in Redis (identity, session state, patterns, tasks, etc.).

### 3) Open the Brain Monitor

Open the sidebar:

- `Memix: Open Debug Panel`

Or click the Memix status bar item.

## How it works (high level)

- Memix runs a **local Rust daemon** and communicates with it over:
	- a local **Unix socket** (`~/.memix/daemon.sock`)
	- and (optionally) **localhost TCP** (`http://127.0.0.1:3456` by default)
- The daemon provides APIs used by the extension:

  - Brain read/write/search (Redis-backed)
  - Rules file generation
  - Token counting utilities

- Memix generates **rules/instructions** files for your AI IDE so the assistant can query high-signal context from Memix (and keep prompts small).

## Development: Run the daemon separately (fast Rust iteration)

When developing the extension (F5), you can keep the daemon running as a standalone process so the extension does **not** spawn/stop it on every reload.

If you build the Rust daemon from source, make sure these prerequisites are installed first:

- Rust toolchain
- `protoc` / Protocol Buffers compiler
- Redis

Examples:

- macOS: `brew install protobuf`
- Ubuntu/Debian: `sudo apt-get install protobuf-compiler`
- Windows: install `protoc` before running the daemon build locally

### Option A: VS Code setting

Set:

`memix.dev.externalDaemon: true`

Optional (if you want TCP instead of socket):

`memix.dev.daemonHttpUrl: "http://127.0.0.1:3456"`

### Option B: .env / environment variables (dev-only)

In development mode, Memix will try to load a `.env` file from:

- the extension folder (`extension/.env`)
- or the workspace root (`<your-workspace>/.env`)

Supported env vars:

- `MEMIX_DEV_EXTERNAL_DAEMON=true`
- `MEMIX_DAEMON_HTTP_URL=http://127.0.0.1:3456`
- `MEMIX_PORT=3456`

Then run the daemon in a separate terminal (from `daemon/`):

- `cargo run`

## Brain data (what gets stored)

Memix stores structured brain keys such as:

- `identity`
- `session:state`
- `patterns`
- `tasks`
- `session:log`
- `decisions`
- `file_map`
- `known_issues`

## Brain Monitor (sidebar) actions

In the **Brain Monitor** panel, the Actions dropdown provides:

- **Refresh**: Re-reads brain data, recomputes size, and updates the UI.
- **Health Check**: Runs consistency checks (required keys, invalid shapes, staleness, and oversized entries).
- **Detect Conflicts**: Requests conflict detection (may report none depending on build).
- **Initialize Brain**: Runs first-time brain initialization for the workspace.
- **Export Brain**: Writes a JSON backup export into your workspace.
- **Import Brain…**: Imports a previously exported JSON backup.
- **Team Sync…**: Team sync setup (availability depends on build).
- **Prune Stale Data**: Trims oversized / stale session log data to keep the brain fast.
- **Recover Corruption**: Restores missing/invalid required brain keys to safe defaults.
- **Clear Brain**: Permanently deletes the current project brain in Redis (destructive).

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

### Advanced tab

The **Advanced** tab includes:

- **Brain Key Coverage** — A checklist/table of brain keys, their sizes, taxonomy, and readiness state.
	- Required keys are clearly separated from recommended/generated/system keys.
	- If required baseline keys are missing, use **Restore baseline keys**.
	- Generated keys may appear as “Generated later” rather than hard failures.
- **Prompt Pack** — A ready-to-paste context bundle generated from high-signal brain keys (identity, session state, patterns, decisions, known issues, tasks, file map).
	- Includes an approximate token estimate.
	- Use **View** to open the full payload in a modal.
	- Click **Copy** to copy the Prompt Pack to your clipboard.
- **Observer DNA OTel Export** — OpenTelemetry-formatted observer export.
	- Use **View export** to inspect the full payload in a modal.
	- Use **Copy JSON** to copy it directly.
- **Daemon Agents** — Shows the currently loaded `AGENTS.md` configuration and recent agent reports.
- **Compiled Context** — Shows the daemon’s context-compiler output for the active file and inferred task type.
- **Proactive Risk** — Shows file risk scoring for the active file using dependency and known-issue signals.
- **Learning Layer** — Shows prompt optimization suggestions, model-performance summaries, and the cross-project developer profile.
- **Hierarchy Resolution** — Shows the resolved value and layers used when loading hierarchical brain context.

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
