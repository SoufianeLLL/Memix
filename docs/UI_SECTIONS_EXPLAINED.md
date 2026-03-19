# Memix UI Sections Explained

This document explains the various Advanced sections available within the Memix VS Code Extension's Debug Panel. It covers what each section does, what features are linked to it, and how the data is calculated and updated.

### Integrity & Freshness
* **What it does:** Monitors the health and completeness of the Redis memory cache (the "brain"). It shows whether all required keys (like `identity.json`, `session_state.json`) exist and how "stale" the data is compared to the last update.
* **Linked Features:** Brain initialization and health checks. Features a "Restore baseline keys" button to fix missing required keys.
* **How it's updated:** Evaluated during the `healthCheck` command. The daemon checks Redis for specific keys and compares their `last_updated` timestamps against the current time to determine staleness.

### Session Log
* **What it does:** Displays a timeline of discrete working sessions, showing what tasks were completed, files changed, and decisions made in each session.
* **Linked Features:** The `session_log.json` brain key.
* **How it's updated:** Populated when a session ends or when the brain state is archived. The daemon reads the historical session log array from Redis and sends the last few entries to the extension UI.

### Daemon Timeline
* **What it does:** Provides a real-time, chronological view of background events and actions performed by the Memix daemon (e.g., "AST parsed", "Dependency graph updated", "Redis synchronized").
* **Linked Features:** Daemon internal telemetry.
* **How it's updated:** The extension subscribes to a telemetry/event stream from the daemon. As the daemon processes tasks, it emits timestamped events which are appended to this view.

### Observer Code DNA
* **What it does:** Acts as a high-level summary of your codebase's architectural footprint, complexity score, total files, indexed symbols, dependency depth, and dominant coding patterns.
* **Linked Features:** The daemon's Code Observer module (AST parser and dependency analyzer).
* **How it's updated:** The daemon continuously parses code changes in the background. It calculates complexity and type coverage, updating the `observerDna` payload sent to the Webview.

### Observer DNA OTel Export
* **What it does:** Allows developers to view or copy the active Code DNA analysis in a standardized OpenTelemetry (OTel) JSON format. This is useful for exporting Memix metrics to external observability platforms.
* **Linked Features:** Code DNA telemetry formatter.
* **How it's updated:** Generated dynamically from the current Observer Code DNA state whenever the user clicks "View JSON" or "Copy OTel".

### Predictive Intent
* **What it does:** Attempts to guess what the user is currently trying to accomplish, based on their active file, recent cursor movements, and edit history. It lists related files that the user is likely to touch next.
* **Linked Features:** The daemon's Intent Engine.
* **How it's updated:** As the user switches tabs or types, the extension sends active file context to the daemon. The daemon uses semantic similarity against the dependency graph to predict the broader intent and rationale.

### Git Archaeology
* **What it does:** Summarizes the repository's history by identifying top authors, frequently changed "hot files," and the overall repository root.
* **Linked Features:** Local `.git` integration within the daemon.
* **How it's updated:** The daemon executes git commands (like `git log`) in the background to calculate file churn and author contributions, sending the results to the Webview payload.

### Daemon Agents
* **What it does:** Monitors the status and reports of background Memix AI agents (e.g., agents running code reviews or proactive research).
* **Linked Features:** Background AI task runners.
* **How it's updated:** Polled from the daemon's agent registry, showing which agents are active and summarizing their most recent findings.

### Compiled Context
* **What it does:** Shows the exact payload of contextual memory that Memix is preparing to send to the LLM for the *next* chat prompt. This helps developers understand what the AI "knows" right now.
* **Linked Features:** Context compiler and Token manager.
* **How it's updated:** Re-compiled dynamically whenever the Active File or Brain State changes, concatenating identity, tasks, and relevant code snippets into a unified prompt context.

### Proactive Risk
* **What it does:** Warns developers about potential bugs, security flaws, or architectural anti-patterns that they are currently typing, before they even run their code.
* **Linked Features:** Real-time semantic analysis and `known_issues.json`.
* **How it's updated:** The daemon analyzes real-time AST changes against a database of known anti-patterns and rules, surfacing high-confidence risk signals.

### Learning Layer
* **What it does:** Summarizes how the Memix AI is adapting to your specific coding style over time. It tracks model performance, prompt optimization metrics, and your personalized developer profile.
* **Linked Features:** Feedback loops and `patterns.json`.
* **How it's updated:** Updated after AI interactions. If the user corrects the AI, the success/failure rate is tracked and the developer profile is refined in Redis to avoid repeating mistakes.

### Hierarchy Resolution
* **What it does:** Maps out the hierarchical relationship of the currently active file within the broader project (e.g., what components import it, what services it depends on).
* **Linked Features:** Abstract Syntax Tree (AST) Dependency Graph.
* **How it's updated:** The daemon calculates the shortest paths between the active file and the project root or main entry points, providing a JSON mapping of its architectural position.
