<p align="center">
  <img src="media/icon.png" width="120" alt="Memix Logo" />
</p>

<h1 align="center">Memix — Persistent Memory for AI Coding Assistants</h1>

<p align="center">
  <b>Never re-explain your project again.</b><br/>
  Memix gives your AI coding assistant a <em>long-term brain</em> that persists across sessions, so it remembers your architecture, decisions, patterns, and preferences — even after you close your editor.
</p>

<p align="center">
  <a href="https://marketplace.visualstudio.com/items?itemName=digitalvizellc.memix"><img src="https://img.shields.io/visual-studio-marketplace/v/digitalvizellc.memix?label=VS%20Marketplace&color=0078D4&logo=visual-studio-code" alt="VS Marketplace" /></a>
  <a href="https://marketplace.visualstudio.com/items?itemName=digitalvizellc.memix"><img src="https://img.shields.io/visual-studio-marketplace/d/digitalvizellc.memix?color=4EC9B0" alt="Downloads" /></a>
  <a href="https://github.com/SoufianeLLL/Memix/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Proprietary-red" alt="License" /></a>
</p>

---

## 🧠 The Problem

Every time you start a new conversation with an AI assistant (GitHub Copilot, Cursor, Windsurf, Antigravity, etc.), it starts completely **blank**. It has no idea:

- What your project does
- Which architectural decisions you've made
- What patterns you follow
- What files have been modified recently
- What bugs you've fixed before

You end up **re-explaining the same context over and over again**, wasting time and getting worse results.

## 💡 The Solution

**Memix** solves this by creating a structured, persistent **"brain"** for every project. This brain lives in a Redis database you control and is injected into your AI assistant's context automatically via rules files — so every new session starts with full project awareness.

Think of it like giving your AI assistant a **notebook** that it writes in and reads from before every conversation.

---

## ✨ Key Features

| Feature | Description |
|---|---|
| 🧠 **Persistent Brain** | Project identity, session logs, decisions, patterns, file maps, and known issues — all stored and recalled automatically. |
| 🔄 **Session Continuity** | Pick up exactly where you left off. Your AI knows what you worked on last session, what files changed, and what's next. |
| 👥 **Team Sync** | Share brain data across teammates so the whole team's AI assistants stay aligned on architecture and decisions. |
| 🛡️ **Health Monitoring** | Built-in health checks detect stale data, oversized keys, corruption, and missing configuration — with one-click recovery. |
| ✂️ **Smart Pruning** | Automatically trims old session logs and oversized data to keep your brain fast and within token budgets. |
| 📦 **Import / Export** | Back up your brain to a JSON file or restore it on a different machine in seconds. |
| 🔐 **Secure by Design** | Credentials are stored in your OS keychain (macOS Keychain, Windows Credential Manager, Linux libsecret) — never in plaintext config files. |
| 🎯 **IDE Auto-Detection** | Automatically detects whether you're using VS Code, Cursor, Windsurf, or Antigravity and generates the correct rules file format. |
| 📊 **Brain Monitor Panel** | A visual sidebar showing brain health, memory usage, session status, and quick actions — all in one place. |

---

## 🚀 Getting Started

### Prerequisites

1. **A Redis instance** — Memix needs a Redis database to store your project's brain. You can use:
   - A free cloud instance from [Redis Cloud](https://redis.io/try-free/), [Upstash](https://upstash.com/), or [Aiven](https://aiven.io/)
   - A local Redis server (`brew install redis` on macOS, `docker run redis` anywhere)

2. **VS Code 1.107+** (or any compatible fork like Cursor, Windsurf, or Antigravity)

### Install

1. Open **VS Code** (or your preferred fork)
2. Go to the **Extensions** tab (`Cmd+Shift+X` / `Ctrl+Shift+X`)
3. Search for **"Memix"**
4. Click **Install**

### First-Time Setup

1. **Connect to Redis** — Open the Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`) and run:
   ```
   Memix: Connect Redis
   ```
   You'll be prompted to enter your Redis connection URL (e.g., `redis://localhost:6379` or `redis://:yourpassword@your-host:6379`). This URL is securely stored in your OS keychain — it is **never** written to any config file.

2. **Initialize Your Brain** — Run:
   ```
   Memix: Initialize Brain
   ```
   You'll be asked to name your project (or press Enter to auto-generate a name). Memix creates the foundational brain structure and generates an AI rules file tailored to your IDE.

3. **That's it!** The Memix icon appears in your Activity Bar. Click it to open the **Brain Monitor** panel.

---

## 📖 Commands Reference

All commands are accessible via the **Command Palette** (`Cmd+Shift+P` / `Ctrl+Shift+P`) or from the **Brain Monitor** panel's action dropdown.

### 🔌 Connect Redis

```
Memix: Connect Redis
```

Connects Memix to your Redis database. On first use, you'll be prompted for your connection URL. The URL is securely stored in your operating system's native keychain — so you only need to enter it once, and it's never saved in plaintext.

**When to use:** The first time you set up Memix, or after running "Clear Stored Secrets."

---

### 🧠 Initialize Brain

```
Memix: Initialize Brain
```

Creates the foundational data structure in Redis for your project. You'll be prompted to enter a **Project Name** — this becomes the identity of your brain. Memix also auto-generates a rules file so your AI assistant can read the brain context automatically.

**When to use:** Once per project, the very first time you use Memix in a workspace.

**What it creates:**
- **Identity** — Your project's name and purpose
- **Session State** — Tracks your current task, session number, and last activity
- **Patterns** — A place to record recurring patterns, preferences, and architectural rules

---

### 🔌 Disconnect Redis

```
Memix: Disconnect Redis
```

Disconnects Memix from the current Redis instance. Your brain data is safely preserved in Redis — disconnecting simply stops the live connection.

---

### 📊 Open Debug Panel

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

---

### 📤 Export Brain

```
Memix: Export Brain
```

Saves your entire brain to a `.json` file in your workspace. This is perfect for:

- **Backups** before major changes
- **Migrating** brain data to a different machine
- **Sharing** brain context with a colleague

---

### 📥 Import Brain

```
Memix: Import Brain
```

Restores brain data from a previously exported `.json` file. A file picker dialog lets you choose the backup file, and all keys are imported into your current Redis brain.

---

### 🩺 Health Check

```
Memix: Health Check
```

Runs a comprehensive diagnostic on your brain. It checks for:

- **Missing required keys** (identity, session state, patterns)
- **Oversized entries** approaching capacity limits
- **Corrupted or invalid data** that may confuse your AI
- **Stale data** that hasn't been updated in 72+ hours

Results are shown as an information / warning / error notification, depending on severity.

---

### ✂️ Prune Brain

```
Memix: Prune Brain
```

Intelligently trims oversized brain data to keep it fast and within your token budget. Pruning targets:

- Old session log entries
- Duplicate or redundant pattern data
- Entries that exceed the configured maximum key size

**When to use:** When health checks show capacity warnings, or periodically to maintain optimal performance.

---

### 🗑️ Clear Brain

```
Memix: Clear Brain
```

**Permanently deletes** all brain data for the current project. A confirmation dialog prevents accidental data loss. This is a destructive action — use Export Brain first if you want a backup.

---

### 🔄 Team Sync Setup

```
Memix: Team Sync Setup
```

Enables collaborative brain sharing. Enter a **Team ID** (a shared identifier your teammates also use), then choose an action:

- **Push to team** — Upload your brain data to the shared team namespace
- **Pull from team** — Download brain data that teammates have pushed
- **Merge decisions** — Intelligently merge architectural decisions from the team brain into your local brain

Your Team ID is stored securely in your OS keychain. Share it privately with teammates.

---

### 🔧 Recover Corrupted Brain

```
Memix: Recover Corrupted Brain
```

Attempts to repair brain data that has become corrupted (e.g., invalid JSON, broken references). It resets malformed entries to valid empty states without deleting healthy data.

---

### ⏹️ End Session

```
Memix: End Session
```

Marks the current working session as complete. This:

1. Saves your session score (how productive the session was)
2. Logs the session summary to the session log
3. Increments the session counter
4. Resets progress trackers for the next session

**When to use:** At the end of a coding session, before closing your editor.

---

### 🔐 Clear Stored Secrets

```
Memix: Clear Stored Secrets
```

Removes your stored Redis URL and Team ID from the operating system's secure keychain. After running this, you'll be prompted to re-enter your credentials on the next connection.

**When to use:** If you need to switch Redis instances or want to fully reset your Memix credentials.

---

## ⚙️ Settings

Configure Memix through VS Code's Settings UI (`Cmd+,` / `Ctrl+,`) or your `settings.json`:

| Setting | Type | Default | Description |
|---|---|---|---|
| `memix.projectId` | `string` | Auto-generated | A unique identifier for your project's brain. Auto-detected from your workspace folder name if empty. |
| `memix.maxBrainSizeKB` | `number` | `512` | Maximum brain size in KB before auto-pruning triggers. |
| `memix.maxTokenBudget` | `number` | `4000` | Maximum number of tokens to inject into your AI assistant's context. |
| `memix.autoSave` | `boolean` | `true` | Automatically save brain state when files are modified. |
| `memix.autoGenerateRules` | `boolean` | `true` | Automatically generate AI rules files when opening a project. |

> **Note:** Your Redis connection URL and Team ID are stored securely in your operating system's keychain — they do **not** appear in Settings.

---

## 🔐 Security

Memix takes security seriously:

- **Credentials** (Redis URL, Team ID) are stored using the VS Code `SecretStorage` API, which delegates to your operating system's native secure credential store (macOS Keychain, Windows Credential Manager, Linux libsecret).
- **Secret detection** is built into the brain validator — Memix will **block writes** that contain API keys, private keys, tokens, or passwords.
- **No telemetry** — Memix does not collect, transmit, or phone-home any data. Everything stays between your editor and your Redis instance.
- **Your data, your infrastructure** — Memix never touches third-party servers. Your brain lives in the Redis instance you provide and control.

For more details, see our [Security Policy](SECURITY.md).

---

## 🤝 Team Sync — How It Works

Team Sync lets multiple developers share architectural context through a common Redis namespace:

1. **One teammate initializes** — Runs `Initialize Brain` and sets up the project brain
2. **Everyone connects** — Each person runs `Team Sync Setup` with the same Team ID
3. **Push & Pull** — Members push their local brain updates to the shared namespace, and pull others' updates
4. **Merge Decisions** — Architectural decisions made by different team members are merged intelligently without conflicts

This ensures every AI assistant on the team has the same understanding of the project's architecture, patterns, and conventions.

---

## 🔗 How Memix Works with Your AI Assistant

Memix generates a **rules file** specific to your IDE:

| IDE | Rules File |
|---|---|
| VS Code / GitHub Copilot | `.github/copilot-instructions.md` |
| Cursor | `.cursor/rules/*.mdc` |
| Windsurf | `.windsurfrules` |
| Antigravity | `.gemini/settings.json` |

This rules file tells your AI assistant to read the brain context from Redis at the start of every conversation. Your AI gets instant access to:

- Project identity and purpose
- Current session state and progress
- Architecture decisions and rationale
- Known issues and patterns
- File map and project structure
- Previous session history

**No plugins or AI-side configuration required** — it works through the IDE's native rules/instructions mechanism.

---

## 🌍 Language Support

Memix works in **any programming language** and with **any project type**. The brain stores metadata *about* your project, not the code itself. Whether you're building a React app, a Python API, a Rust library, or a mobile app — Memix enhances your AI assistant's understanding of the project context.

The extension UI and commands are currently in **English**. Multi-language UI support is planned for future releases.

---

## 📋 Requirements

| Requirement | Details |
|---|---|
| **Editor** | VS Code 1.107+ (or compatible forks: Cursor, Windsurf, Antigravity) |
| **Runtime** | Node.js 18+ (bundled with VS Code) |
| **Database** | Any Redis 6+ instance (local or cloud) |
| **OS** | macOS, Windows, or Linux |

---

## 🐛 Known Issues

- The extension requires an active Redis connection for all brain operations. If Redis is unavailable, brain reads/writes will fail gracefully with error messages.
- Team Sync currently uses a simple push/pull model. Real-time collaboration and conflict resolution are planned for future versions.

---

## 📝 Release Notes

### 0.1.0 — Initial Release

- Persistent brain storage with Redis
- Session tracking and continuity
- Health monitoring and smart pruning
- Team Sync (push, pull, merge)
- Import / Export brain data
- Secure credential storage via OS keychain
- IDE auto-detection (VS Code, Cursor, Windsurf, Antigravity)
- Brain Monitor sidebar panel with real-time stats
- Auto-generated rules files for all supported IDEs

---

## 📜 License

Memix is **free to use** but is proprietary software. See the [LICENSE](LICENSE) file for details.

© 2026 DigitalVize LLC. All rights reserved.

---

## 🔗 Links

- [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=digitalvizellc.memix)
- [GitHub Repository](https://github.com/SoufianeLLL/Memix)
- [Report an Issue](https://github.com/SoufianeLLL/Memix/issues)
- [Security Policy](SECURITY.md)

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/SoufianeLLL">Soufiane Loudaini</a> · <a href="https://digitalvize.com">DigitalVize LLC</a>
</p>
