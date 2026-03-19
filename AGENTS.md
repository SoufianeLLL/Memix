# Memix Agents Protocol

> **IMPORTANT:** This file is read automatically by AI agents that support the AGENTS.md convention. If your IDE does not support AGENTS.md directly, use the rules files in `.cursor/rules/`, `.windsurfrules`, `.github/copilot-instructions.md`, or `.agents/rules/`.

## What Memix Is

Memix is an autonomous engineering intelligence layer.

It has two cooperating systems:

- The **Rust daemon** runs continuously, observes the workspace, executes background agents, compiles context, scores risk, and writes high-signal memory.
- The **LLM agent** is conversational and on-demand. It should consume the daemon’s prepared intelligence instead of rebuilding context from scratch each turn.

## Automatic Capabilities

When working with a project that has Memix:

### 1. Real-Time Code Observation
- The daemon watches the workspace continuously.
- File saves trigger AST parsing, semantic diffing, dependency graph updates, predictive intent refresh, and Code DNA updates.
- Observer state is available through daemon APIs and mirrored brain entries.

### 2. AGENTS Runtime
- The daemon supports an **AGENTS-driven runtime**.
- It parses the project agent spec and executes supported background agents on live file-save and session-start events.
- Current runtime surfaces include agent config inspection and recent agent reports.

### 3. Context Compiler
- Memix can compile a **budget-fit context packet** instead of sending raw, wasteful project state.
- The compiler uses relevant-file elimination, AST skeleton extraction, brain deduplication, history compaction, rules pruning, ranking, and budget fitting.
- This is the preferred path for token-sensitive workflows.

### 4. Proactive Risk Warnings
- Before invasive edits, Memix can assess file risk using dependents, prior breakage signals, known issues, Code DNA stability/hotness, and git archaeology.
- Use the proactive risk API before large refactors or edits to critical files.

### 5. Hierarchical Brain Resolution
- Memix supports **layered context inheritance** through hierarchy resolution APIs.
- Use this for monorepo-style parent/child context loading where a local layer can override or merge inherited layers.

### 6. Learning Layer
- Memix can record prompt outcomes, suggest better context composition for future tasks, compare model performance by task type, and aggregate a cross-project developer profile.
- This is meant to improve context selection over time, not to add auto-completion.

### 7. Code DNA
- Memix maintains an AST-derived Code DNA summary for the project.
- DNA includes architecture, complexity, hot/stable zones, explainability, and OpenTelemetry export.

## Scope Clarification

### Current Priority
- Observation
- Agent execution
- Context compilation
- Risk analysis
- Learning from prompt outcomes
- Hierarchical memory resolution

### Explicitly Not a Priority Right Now
- **Auto-completion generation**
- Copilot-style inline completion behavior

Memix may prepare better context for future assistance, but it is not currently optimizing for offering direct completion products.

## Operating Protocol

### Boot Sequence
1. Start with the highest-signal daemon or brain context available.
2. Prefer compiled or summarized context over broad raw reads.
3. Pull additional detail only when needed for the current task.
4. For risky edits, query proactive risk before making large changes.

### Context Retrieval Rules
- Prefer daemon APIs over large file dumps when an API already provides the answer.
- Prefer **context compilation** for task-focused prompt assembly.
- Prefer **agent reports**, **observer DNA**, **intent**, and **timeline** before reconstructing state manually.
- Prefer **hierarchy resolution** when working in layered or monorepo contexts.

### Token Discipline
- Prefer summaries over full file bodies.
- Prefer AST skeletons over full source when possible.
- Use exact token counting or optimization endpoints when fitting a hard budget.
- Avoid sending context the brain or compiled packet already covers.

## Daemon API

Memix exposes a local daemon with these core capabilities:

```text
GET  /health

GET  /api/v1/memory/:project_id
POST /api/v1/memory/:project_id
GET  /api/v1/memory/:project_id/search?q=...

POST /api/v1/tokens/count
POST /api/v1/tokens/optimize
POST /api/v1/context/compile

GET  /api/v1/observer/dna
GET  /api/v1/observer/dna/otel
GET  /api/v1/observer/graph
GET  /api/v1/observer/changes
GET  /api/v1/observer/intent
GET  /api/v1/observer/git

GET  /api/v1/session/current
GET  /api/v1/session/replay
GET  /api/v1/session/timeline

GET  /api/v1/agents/config
GET  /api/v1/agents/reports

GET  /api/v1/proactive/risk?project_id=...&file=...

POST /api/v1/learning/prompts/:project_id/record
GET  /api/v1/learning/prompts/:project_id/optimize?task_type=...
GET  /api/v1/learning/model-performance/:project_id
GET  /api/v1/learning/developer-profile

POST /api/v1/brain/hierarchy/resolve
```

## How Agents Should Use Memix

When you need project context, fetch the smallest high-value set first:

- Identity, purpose, and current task state
- Relevant patterns and decisions
- Recent observer changes
- Code DNA and intent snapshot
- Agent reports for the active workstream
- Proactive risk for files you are about to modify

Only pull raw file content when the daemon surfaces are insufficient.

## Memory Writeback

When you need to persist information to the brain, write `.memix/brain/pending.json`.

The merge rule is absolute: read the existing `.memix/brain/<key>.json` before every
write. Your upsert must be the complete merged object, not a diff. Partial writes
silently destroy fields that existed before.

Every upsert must include `project_id` as a field inside the entry object itself.
The `content` field must be a JSON string (stringified) containing the full value.

After writing, the daemon validates, merges into Redis, clears the pending file,
and optionally writes `.memix/brain/pending.ack.json` as confirmation.

Do not write to brain files directly. Only use pending.json as the writeback channel.

## Security

- Never store secrets, API keys, passwords, or raw credentials in memory.
- Memix should treat security findings as warnings or critical signals.
- Credentials belong in secure storage, not in source-controlled brain artifacts.

---

**Project Memory:** Active  
**Protocol:** Memix AGENTS.md
