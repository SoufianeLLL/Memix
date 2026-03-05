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
- ✅ Built-in voice commands (save brain, recap, reboot, etc.)
- ✅ Persistent Task Tracking — tasks never get lost across sessions

---

## Near-Term — v0.2.0

### Hybrid Redis API (Performance)
Replace file-based brain sync with a local HTTP API served by the extension. The AI calls `curl localhost:PORT/brain/...` to read/write brain data directly through the `ioredis` backend — combining Redis-speed reads with full validation and zero MCP dependency.

### Cross-IDE Brain Portability
Use the same brain across Cursor, Windsurf, Antigravity, and VS Code simultaneously. Brain data stays in sync regardless of which editor you open.

### Context Budget Optimizer
Intelligently compress brain context to fit within the AI's token limit. Instead of dumping all brain data, dynamically select the most relevant keys based on the user's current file, recent activity, and task type.

### Smart Auto-Pruning
Automatically detect when brain data becomes stale (e.g., session logs older than 30 days, resolved issues, outdated file maps) and prune or archive them without user intervention.

### Multi-Project Brain
Support switching between multiple project brains in the same workspace (monorepos). Each sub-project gets its own brain namespace while sharing common patterns and decisions.

---

## Mid-Term — v0.3.0

### Conflict-Aware Team Sync
Real-time brain sync with conflict detection. When two teammates push conflicting decisions or patterns, surface a merge UI instead of silently overwriting.

### AI Behavior Scoring
Track how well the AI follows the brain protocol across sessions. Score compliance on: boot sequence completion, auto-save frequency, footer inclusion, error recovery. Show trends in the Brain Monitor panel.

### Pattern Learning Engine
Automatically detect recurring user corrections and promote them to first-class patterns. If the user says "don't use try/catch" three times, Memix auto-adds it to `patterns.json` with high priority.

### Brain Diff & History
Version-control brain changes. Show a diff of what changed between sessions, allow rollback to any previous brain state, and visualize brain evolution over time.

---

## Long-Term — v1.0.0

### Brain Templates / Marketplace
Pre-built brain templates for common project types (Next.js app, Python API, React Native, etc.) that come with curated patterns, decisions, and file maps. Share and install brain templates from a community marketplace.

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
| "AI forgets tasks mid-conversation" | **Persistent task tracker** | ✅ Shipped |
| "AI ignores my rules files" | Guard template with anti-drift enforcement | ✅ Shipped |
| "Context window is too small" | Context budget optimizer | 🔜 v0.2.0 |
| "No memory between chat sessions" | Session log + continuity | ✅ Shipped |
| "AI can't remember architectural decisions" | Decisions key (append-only) | ✅ Shipped |
| "My teammate's AI doesn't know our patterns" | Team Sync | ✅ Shipped |
| "AI breaks things it previously fixed" | Known issues tracker | ✅ Shipped |
| "File reads slow, I want Redis speed" | Hybrid HTTP API | 🔜 v0.2.0 |

---

## Contributing Ideas

Have a feature request or pain point that Memix should solve? [Open an issue](https://github.com/SoufianeLLL/Memix/issues) with the `feature-request` label.

---

<p align="center">
  Made with ❤️ by <a href="https://www.linkedin.com/in/loudaini">Soufiane Loudaini</a> · <a href="https://digitalvize.com">DigitalVize LLC</a>
</p>
