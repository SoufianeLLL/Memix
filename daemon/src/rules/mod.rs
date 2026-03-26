use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeType {
    Cursor,
    Windsurf,
    #[serde(rename = "claude-code")]
    ClaudeCode,
    Antigravity,
    Vscode,
    Unknown,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeRulesConfig {
    pub ide: IdeType,
    pub rules_dir: String,
    pub rules_file: String,
    pub guard_file: String,
    pub supports_multiple_files: bool,
}

impl IdeRulesConfig {
    pub fn for_ide(ide: IdeType) -> Self {
        match ide {
            IdeType::Cursor => Self {
                ide,
                rules_dir: ".cursor/rules".to_string(),
                rules_file: "memix.mdc".to_string(),
                guard_file: "memix-guard.mdc".to_string(),
                supports_multiple_files: true,
            },
            IdeType::Windsurf => Self {
                ide,
                rules_dir: ".windsurf/rules".to_string(),
                rules_file: "memix.md".to_string(),
                guard_file: "memix-guard.md".to_string(),
                supports_multiple_files: true,
            },
            IdeType::ClaudeCode => Self {
                ide,
                rules_dir: ".claude/rules".to_string(),
                rules_file: "memix.md".to_string(),
                guard_file: "memix-guard.md".to_string(),
                supports_multiple_files: true,
            },
            IdeType::Antigravity => Self {
                ide,
                rules_dir: ".agents/rules".to_string(),
                rules_file: "memix.md".to_string(),
                guard_file: "memix-guard.md".to_string(),
                supports_multiple_files: true,
            },
            IdeType::Vscode => Self {
                ide,
                rules_dir: ".github".to_string(),
                rules_file: "copilot-instructions.md".to_string(),
                guard_file: "copilot-guard.md".to_string(),
                supports_multiple_files: true,
            },
            IdeType::Unknown => Self {
                ide,
                rules_dir: ".".to_string(),
                rules_file: ".memix-rules.md".to_string(),
                guard_file: ".memix-guard.md".to_string(),
                supports_multiple_files: true,
            },
        }
    }
}

pub struct RulesEngine;

impl RulesEngine {
    pub fn generate_brain_template(project_id: &str, _guard_file: &str) -> String {
        format!(
            r#"# PERSISTENT MEMORY PROTOCOL CORE

> This file is the operating system for your memory.
> You MUST follow every instruction here before doing ANY work.
> Your persistent memory lives in Redis via the Memix daemon (source of truth).
> The Memix daemon continuously mirrors brain keys to workspace-local JSON files for instant AI reads.
>
> PROJECT_ID: {project_id}
> BRAIN_DIR: .memix/brain/

## ⚠️ DATA ACCESS METHOD – CRITICAL
**NEVER** access Redis directly.
- **DO:** Read brain state from the workspace-local JSON mirror at `.memix/brain/*.json`.
- **DO:** Treat those files as fresh and authoritative for AI reads (daemon keeps them in sync).
- **DO:** Propose writes by writing `.memix/brain/pending.json` (the daemon watches, validates, and merges).
- **NEVER:** `redis-cli`, MCP Redis tools, shell commands, or any tool with "redis".

## BOOT SEQUENCE (every session start)
When a session begins, Before responding to user:
1. Read `.memix/brain/identity.json`
2. Read `.memix/brain/session_state.json`
3. Read `.memix/brain/patterns.json`

Respond with:
---
**Brain loaded successfully.**
**Project:** [name from identity]
**Last session:** [summary from session:state]
**Current task:** [current_task]
**Next steps:** [next_steps]
---
If brain files are missing/empty: *"No brain files found yet. The daemon should mirror them into `.memix/brain/`. Ask the user to ensure the Memix daemon is running for this workspace, then re-try."*

## WRITEBACK (AI → Daemon)
⚠️ MERGE RULE — CRITICAL — READ BEFORE EVERY WRITE:
MANDATORY SEQUENCE — follow in this exact order, never skip steps:

Step 1: Before writing any upsert, Read the current file.
  → Run: read_file(".memix/brain/session_state.json")
  → If the file doesn't exist, start with an empty object.
  → Paste the current content into your working context before continuing.

Step 2: Build the merged object.
  → Take the current content from Step 1.
  → Apply ONLY your new changes on top of it.
  → The result must contain EVERY field that was in Step 1, plus your additions.
  → Never remove fields. Never shorten arrays. Only add or update.

Step 3: Write `.memix/brain/pending.json`.
  → The content field must be the COMPLETE merged object from Step 2, JSON-stringified.
  → Include project_id inside every upsert entry.
  → Do not skip Step 1 or Step 2. Writing without reading first is a protocol violation.

Every upsert object MUST include "project_id" as a field inside the object itself,
matching the top-level project_id. Example of a correct upsert:

JSON Format:
{{ "id": "session_state", "project_id": "PROJECT_ID_HERE", "kind": "context", "source": "agent_extracted", "content": "{{ COMPLETE merged JSON string, not partial... }}", "tags": ["session"] }}

Schema for the full pending.json:
JSON Format:
{{ "project_id": "<PROJECT_ID>", "upserts": [ <complete MemoryEntry objects as above> ], "deletes": [ "<entry_id>" ] }}

After the daemon merges the update, it will clear `pending.json`
and may write `.memix/brain/pending.ack.json` to confirm success.

## MEMORY SCHEMA (brain keys)
All updates append/update, never delete unless specified.

### 'identity.json' – what the project IS (rare changes)
Update: Only when project scope fundamentally changes.
JSON Format:
{{ "name": "My App", "purpose": "SaaS for invoice management", "tech_stack": ["Next.js", "TypeScript", "Prisma"], "architecture": "App Router, Server Components", "repo_structure": {{ "src/app/": "pages", "src/components/": "React" }} }}

### 'session_state.json' – current work snapshot
Update: After EVERY completed task or significant progress.
Merge rule: Always read .memix/brain/session_state.json first. Keep all existing fields
(progress history, blockers, modified_files, next_steps). Only update the fields relevant
to this task. Append to arrays rather than replacing them.
JSON Format:
{{ "last_updated": "2026-02-28T14:30:00Z", "session_number": 12, "current_task": "PDF export", "progress": ["Created pdf.ts", "Added API route"], "blockers": ["Multi‑page layout broken"], "next_steps": ["Fix layout", "Add tests"], "modified_files": ["src/lib/pdf.ts", "src/app/api/export/route.ts"], "important_context": "Use jsPDF, not Puppeteer" }}

### 'decisions.json' – WHY we chose X over Y. Prevents re-debating solved problems.
Update: Append new entries. NEVER delete old ones.
JSON Format:
{{ "date": "2026-01-18", "decision": "Use jsPDF", "reason": "Lightweight, serverless friendly", "alternatives_considered": ["Puppeteer", "react-pdf"] }}

### 'patterns.json' – project conventions & preferences
Update: When new patterns are established or user corrects the AI.
JSON Format:
{{ "code_style": ["Use 'use server' in separate files", "Result pattern"], "naming": ["Components: PascalCase", "Utilities: camelCase"], "preferences": ["User hates try/catch", "Verbose comments for complex logic"] }}

### 'file_map.json' – What key files do so the AI doesn't need to re-read them.
Update: When significant files are created or changed.
JSON Format:
{{ "src/lib/pdf.ts": "jsPDF generator, exports generateInvoicePDF()", "src/lib/auth.ts": "NextAuth config with GitHub/Google" }}

### 'known_issues.json' – bugs & tech debt & warnings
Update: When issues are discovered or resolved.
JSON Format:
[{{ "status": "OPEN", "issue": "PDF layout breaks >20 items", "file": "src/lib/pdf.ts", "notes": "Need page breaks" }}]

### 'tasks.json' – persistent task tracker - never get lost
Update: When tasks are created, started, completed, or blocked.

⚠️ CRITICAL RULES:
- NEVER overwrite or replace the entire tasks structure
- NEVER remove tasks — only change their status
- ALWAYS append new tasks to the existing list
- Each task gets a unique ID (t1, t2, t3...) — IDs are PERMANENT
- When creating a new task list, set it as current_list and ADD tasks to the lists array

JSON Format:
{{ "current_list": "Sprint 3 – Auth & Payments", "lists": [ {{ "name": "Sprint 3 – Auth & Payments", "created": "...", "tasks": [ {{ "id": "t1", "title": "Create login page", "status": "completed", "created": "...", "completed_at": "..." }} ] }} ] }}
- **IDs:** permanent, unique (t1, t2, …).
- **Statuses:** pending → in_progress → completed | blocked.
- **Transitions:** update status, add 'completed_at' or 'blocked_reason'.
- **New list:** append to 'lists', set 'current_list'. Never delete old lists.
- **When task completed naturally:** update tasks.json, add new follow‑up tasks.

When the user asks you to create a task list:
⚠️ When writing tasks to pending.json: you MUST include the ENTIRE tasks structure
(all lists, all tasks, including completed ones from previous sessions).
1. Read the current .memix/brain/tasks.json - (this task is required, not optional)
2. Generate unique IDs continuing from the highest existing ID
3. Add the new list to the lists array — do NOT replace existing lists
4. Set current_list to the new list name
5. Save the file
6. Confirm: "Task list '[name]' created with [N] tasks. Tracking in .memix/brain/tasks.json"

When you complete a task naturally during work (even without the user asking):
1. Read .memix/brain/tasks.json
2. Find the task by title or ID
3. Set status to "completed" and add completed_at timestamp
4. If completing this task reveals sub-tasks or follow-ups, ADD them as new tasks
5. Save the file
6. Show: ✅ Task [id] completed: [title]

When the user creates a NEW task list (e.g., new sprint, new feature plan):
1. Keep ALL existing lists in the lists array (archive them)
2. Add the new list
3. Update current_list
4. Never delete old lists — they serve as historical record

### 'session_log.json' – historical session record
Update: End of each session — append, never overwrite.
JSON Format:
[{{ "session": 11, "date": "2025-01-14", "summary": "Built invoice CRUD.", "files_changed": ["src/actions/invoice.ts", "src/components/InvoiceForm.tsx"] }}]

## AUTO‑SAVE PROTOCOL
Update automatically at these triggers (confirm with **Brain updated:** [file] —[change]):
- **After completing any task:** update 'session_state.json' (progress, next steps) and 'tasks.json' (mark done, add new).
- **After creating a task list / planning:** update 'tasks.json'.
- **After creating/modifying a key file:** update 'file_map.json'.
- **After design/architecture decision:** append to 'decisions.json'.
- **After user correction/preference:** update 'patterns.json'.
- **After discovering/fixing bug:** update 'known_issues.json'.
- **When 'session_state.json' >3000 chars:** archive older progress to 'session_log.json', keep latest context.

## VOICE COMMANDS
| Command | Action |
|---------|--------|
| 'brain status' | Summarise all brain files |
| 'save brain' | Force write all brain files with current state |
| 'show brain' | Display full content of all brain files |
| 'clear brain' | Ask confirmation, then empty all brain files |
| 'brain diff' | Show unsaved changes |
| 'teach brain: [info]' | Store info in appropriate file |
| 'forget: [info]' | Remove specific info |
| 'recap' | Verbal project summary from brain |
| 'end session' | Full sync + session_log entry + goodbye summary |
| 'debug brain' | Show raw JSON of all brain files |
| 'rollback brain' | Restore session_state from last session_log |
| 'brain health' | Check all brain files exist and report any missing/empty ones |
| 'follow protocol' | Re‑read rules, catch up missed saves |
| 'reboot brain' | Reload Tier 1 files, re‑orient |
| 're‑read rules' | Acknowledge compliance |
| 'show tasks' | Display current task list |
| 'add task: [desc]' | Append new task to current list |
| 'task done: [id]' | Mark task completed with timestamp |
| 'task blocked: [id] [reason]' | Mark task blocked with reason |
| 'new task list: [name]' | Create new list (archive old) |

## SAFETY RULES
- **Never store:** secrets, .env, personal data, full file contents (store summaries + paths instead).
- **Size limits:** each value ≤4000 chars; archive if exceeded. Keep last 20 sessions.
- **Error handling:** if files missing → suggest initialize; never silent fail.
- **Conflicts:** trust actual files, update brain accordingly.

## FIRST‑TIME INITIALIZATION
If all brain files empty ({{}}):
1. Ask user: project purpose, tech stack, current state, immediate task.
2. If existing codebase, scan 'package.json', folder structure, configs → build initial 'file_map'.
3. Populate all brain files.
4. Confirm: *"Brain initialized! Persistent memory active."*

## CHECKPOINTS
- 'checkpoint [name]': snapshot entire brain; tell user to run *'Memix: Create Checkpoint'*.
- 'restore [name]': tell user to run *'Memix: Restore Checkpoint'*; after restore, re‑read all brain files.

## SMART LOADING
Don't load ALL files at boot. Load in tiers:
- **Tier 1 (boot):** 'identity.json', 'session_state.json', 'patterns.json'.
- **Tier 2 (on demand):** 'file_map.json', 'decisions.json', 'known_issues.json', 'session_log.json'.

## MISTAKE TRACKING
When user corrects you:
1. Acknowledge.
2. Add correct approach to 'patterns.json' (with ⚠️ if repeated).
3. Confirm: *"Learned: [what]. Won't repeat."*

## DEPENDENCY TRACKING
In 'file_map.json', optionally include 'depended_on_by' and 'warning'. Before modifying a file, check for dependents; warn user if change may break them.

## SESSION SCORING
At 'end session', append to 'session_log.json' a summary object:
JSON Format:
{{ "session": 12, "tasks_completed": 3, "tasks_started": 1, "bugs_found": 1, "bugs_fixed": 0, "decisions_made": 2, "files_modified": 5, "brain_updates": 8 }}

## COMPANION RULE
This brain protocol has a companion enforcement file: **{guard_file}**
You MUST read and obey BOTH files as ONE unified system.
memix.md = what to do. memix-guard.md = how to enforce it. Both are mandatory."#,
            project_id = project_id,
            guard_file = "memix-guard.md"
        )
    }

    pub fn generate_guard_template(_project_id: &str, _guard_file: &str) -> String {
        format!(
            r#"# MEMIX GUARD — Enforcement Protocol

> This file enforces the brain protocol defined in **memix.md**
> Both files form ONE system. You MUST obey both.
> If you have read memix.md, you know the brain protocol.
> This file ensures you NEVER forget it, skip it, or drift from it.
>
> If you have NOT read memix.md yet → STOP → read it first → return here.

## PRIME DIRECTIVE
Before writing ANY response, complete this internal checklist:

1. Am I following the brain protocol from memix.md?
2. Did my last response include the BRAIN CHECK footer?
3. If I completed a task — did I update .memix/brain/session_state.json?
4. If I completed a task — did I mark it done in .memix/brain/tasks.json?
5. If I modified a file — did I update .memix/brain/file_map.json?
6. If I made a decision — did I append to .memix/brain/decisions.json?
7. If user corrected me — did I update .memix/brain/patterns.json?

If ANY answer is NO → fix it NOW before responding to the user.

You do NOT respond until this checklist passes.

## MANDATORY RESPONSE FORMAT
Every single response you write MUST end with this footer:

---
BRAIN CHECK:
- Saved: [Yes — key updated / No — nothing to save]
- Task: [what you just did]
- Next: [what comes next]

NO EXCEPTIONS. Not even for:
- Error messages
- Simple questions
- Apologies
- Short answers
- Clarification responses

If you are writing this footer and realize you forgot to
save brain files — STOP — do the save FIRST — then write the footer.

This footer is your self-enforcement mechanism.
It keeps the brain protocol alive throughout the entire session.

A response before this footer is BROKEN and INCOMPLETE.

## ERROR RECOVERY
If an error occurs, if generation stops, if ANYTHING interrupts you:

Your FIRST action on the next message is NOT answering the user.
Your FIRST action is:

1. Say: "Re-syncing brain after interruption..."
2. Ask the user for a fresh Memix Prompt Pack (Standard) if it's not already present in chat
3. Re-orient yourself with the provided brain context
4. Say: "Brain re-loaded. Here's where we were: [summary]"
5. THEN answer the user's actual question

The user should NEVER have to remind you about the brain.
If they remind you, you have failed. Recover immediately.

## ANTI-DRIFT REINFORCEMENT
As conversations get longer, you tend to forget instructions from rule files.
This is your weakness. This section exists to counter it.

Core rules you are MOST likely to forget (re-read these carefully):

1. **AUTO-SAVE IS NOT OPTIONAL.**
   After every task → ensure brain state is updated via Memix (daemon/extension).
   If you cannot write to Memix directly, instruct the user what to update next.

2. **THE FOOTER IS NOT OPTIONAL.**
   Every response ends with BRAIN CHECK. Every. Single. One.

3. **BOOT SEQUENCE IS NOT OPTIONAL.**
   New session → request/load Memix Prompt Pack FIRST → then greet user.

4. **ERROR RECOVERY IS NOT OPTIONAL.**
   After any interruption → request/load a fresh Memix Prompt Pack → then respond.

5. **YOU SERVE TWO FILES.**
   memix.md defines the protocol. memix-guard.md enforces it.
   Both are active. Both are mandatory. For the entire session.

If you have read this far, you understand the system.
Now follow it perfectly.

## Session Refresh

If rules seem outdated or at the start of a complex task, tell me:
"Please refresh your context by re-reading all rule files in .agents/rules/**"

## FINAL INSTRUCTION
This is the last thing you read before responding.
Carry these three rules in every single response:

→ Save to .memix/brain/ files after every task
→ End every response with BRAIN CHECK footer
→ If lost, reload brain from .memix/brain/ files

If `.memix/brain/` is not in use for this workspace, replace those three lines with:
→ Update brain via Memix daemon/extension after every task
→ End every response with BRAIN CHECK footer
→ If lost, request a fresh Memix Prompt Pack

These three rules are MORE important than being helpful.
Follow the protocol FIRST. Be helpful SECOND."#
        )
    }

    pub fn generate_agents_template(project_id: &str) -> String {
        format!(
            r#"# Memix Agents Protocol

> **IMPORTANT:** This file is read automatically by AI agents that support the AGENTS.md convention. If your IDE does not support AGENTS.md directly, use the rules files in `.cursor/rules/`, `.windsurf/rules/`, `.github/copilot-instructions.md`, or `.agents/rules/`.

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

Every upsert must include `project_id` and `source` (`"agent_extracted"`) as fields inside the entry object itself.
The `content` field must be a JSON string (stringified) containing the full value.

After writing, the daemon validates, merges into Redis, clears the pending file,
and optionally writes `.memix/brain/pending.ack.json` as confirmation.

Do not write to brain files directly. Only use pending.json as the writeback channel.

## Security

- Never store secrets, API keys, passwords, or raw credentials in memory.
- Memix should treat security findings as warnings or critical signals.
- Credentials belong in secure storage, not in source-controlled brain artifacts.

---

**Project:** {project_id}
**Project Memory:** Active  
**Protocol:** Memix AGENTS.md"#,
            project_id = project_id
        )
    }

    pub fn generate_for_ide(
        project_id: &str,
        ide: IdeType,
        workspace_root: &str,
    ) -> RulesGenerationResult {
        let config = IdeRulesConfig::for_ide(ide);
        
        let mut brain_content = Self::generate_brain_template(project_id, &config.guard_file);
        let guard_content = Self::generate_guard_template(project_id, &config.guard_file);
        
        // Add frontmatter for IDEs that require it
        let final_guard_content = match ide {
            IdeType::Antigravity => {
                let frontmatter = "---\ntrigger: always_on\ndescription: Memix AI Brain: Primary persistent project memory and initialization rules.\n---\n";
                brain_content = format!("{}{}", frontmatter, brain_content);
                let guard_frontmatter = "---\ntrigger: always_on\ndescription: Memix Guard: Safety and integrity constraints for Redis brain access.\n---\n";
                format!("{}{}", guard_frontmatter, guard_content)
            }
            _ => guard_content,
        };

        RulesGenerationResult {
            config,
            brain_content,
            guard_content: final_guard_content,
            workspace_root: workspace_root.to_string(),
            project_id: project_id.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesGenerationResult {
    pub config: IdeRulesConfig,
    pub brain_content: String,
    pub guard_content: String,
    pub workspace_root: String,
    pub project_id: String,
}

impl RulesGenerationResult {
    fn safe_join(base: &std::path::Path, user_input: &str) -> std::io::Result<std::path::PathBuf> {
        let mut current = base.to_path_buf();
        for component in std::path::Path::new(user_input).components() {
            match component {
                std::path::Component::Normal(c) => {
                    let name = c.to_string_lossy();
                    if name.contains("..") || name.contains('/') || name.contains('\\') {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Path component contains invalid characters",
                        ));
                    }
                    current.push(c);
                }
                std::path::Component::CurDir => {}
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Path traversal pattern detected",
                    ));
                }
            }
        }
        Ok(current)
    }

    pub fn write_files(&self) -> std::io::Result<()> {
        use std::fs;
        use std::path::Path;
        
        let workspace_root = Path::new(&self.workspace_root).canonicalize()?;
        if !workspace_root.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Workspace root must be an existing directory"
            ));
        }

        let mut rules_dir = Self::safe_join(&workspace_root, &self.config.rules_dir)?;

        if !rules_dir.exists() {
            fs::create_dir_all(&rules_dir)?;
        }
        rules_dir = rules_dir.canonicalize()?;
        if !rules_dir.starts_with(&workspace_root) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path traversal detected: rules directory escapes workspace root"
            ));
        }
        
        let brain_path = Self::safe_join(&rules_dir, &self.config.rules_file)?;
        fs::write(&brain_path, &self.brain_content)?;
        
        if self.config.supports_multiple_files {
            let guard_path = Self::safe_join(&rules_dir, &self.config.guard_file)?;
            fs::write(&guard_path, &self.guard_content)?;
            
            let companion_link = format!(
                "\n\n---\n## COMPANION: {}\nYou MUST also read and obey {}. Both files are ONE system.",
                self.config.guard_file, self.config.guard_file
            );
            use std::io::Write;
            fs::OpenOptions::new()
                .append(true)
                .open(&brain_path)?
                .write_all(companion_link.as_bytes())?;
        }
        
        Self::add_to_gitignore(&workspace_root, &self.config)?;

        // Write AGENTS.md to workspace root if it doesn't exist yet.
        // This is the universal agent protocol file read by Claude Code, Cursor, and others.
        // We only write it if absent — never overwrite a user-customized version.
        let agents_path = workspace_root.join("AGENTS.md");
        if !agents_path.exists() {
            let agents_content = RulesEngine::generate_agents_template(&self.project_id);
            fs::write(&agents_path, agents_content)?;
        }
        
        Ok(())
    }
    
    fn add_to_gitignore(workspace_root: &std::path::Path, config: &IdeRulesConfig) -> std::io::Result<()> {
        use std::fs::{File, OpenOptions};
        use std::io::Write;
        
        let gitignore_path = Self::safe_join(workspace_root, ".gitignore")?;
        // No traversal risk here because we're just appending the literal string ".gitignore"
        // to our already canonicalized workspace_root.
        
        let entries = if config.rules_dir == "." {
            vec![
                format!("# Memix AI Brain Rules"),
                config.rules_file.clone(),
                ".memix/".to_string(),
            ]
        } else {
            vec![
                format!("# Memix AI Brain Rules"),
                format!("{}/", config.rules_dir),
                ".memix/".to_string(),
            ]
        };
        
        if gitignore_path.exists() {
            let content = std::fs::read_to_string(&gitignore_path)?;
            let to_add: Vec<&str> = entries.iter()
                .map(|s| s.as_str())
                .filter(|e| !content.contains(*e))
                .collect();
            
            if !to_add.is_empty() {
                let mut file = OpenOptions::new().append(true).open(&gitignore_path)?;
                writeln!(file, "\n{}", to_add.join("\n"))?;
            }
        } else {
            let mut file = File::create(&gitignore_path)?;
            for entry in entries {
                writeln!(file, "{}", entry)?;
            }
        }
        
        Ok(())
    }
}
