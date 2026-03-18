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

impl IdeType {
    pub fn detect() -> Self {
        // This will be called from extension with IDE info passed in
        // Default to VSCode-compatible format
        Self::Vscode
    }
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
                rules_file: "memix-brain.mdc".to_string(),
                guard_file: "memix-guard.mdc".to_string(),
                supports_multiple_files: true,
            },
            IdeType::Windsurf => Self {
                ide,
                rules_dir: ".".to_string(),
                rules_file: ".windsurfrules".to_string(),
                guard_file: ".windsurfrules-guard".to_string(),
                supports_multiple_files: false,
            },
            IdeType::ClaudeCode => Self {
                ide,
                rules_dir: ".".to_string(),
                rules_file: "CLAUDE.md".to_string(),
                guard_file: "CLAUDE-GUARD.md".to_string(),
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
    pub fn generate_brain_template(project_id: &str, _redis_url: Option<&str>) -> String {
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
You may propose updates by writing a file at `.memix/brain/pending.json`.

Schema:
{{
  "project_id": "<PROJECT_ID>",
  "upserts": [ <MemoryEntry JSON objects> ],
  "deletes": [ "<entry_id>", "<entry_id>" ]
}}

After the daemon merges the update, it will clear `pending.json` and may write `.memix/brain/pending.ack.json`.

## MEMORY SCHEMA (brain keys)
All updates append/update, never delete unless specified.

### 'identity.json' – what the project IS (rare changes)
Update: Only when project scope fundamentally changes.
JSON Format:
{{
  "name": "My App",
  "purpose": "SaaS for invoice management",
  "tech_stack": ["Next.js", "TypeScript", "Prisma"],
  "architecture": "App Router, Server Components",
  "repo_structure": {{ "src/app/": "pages", "src/components/": "React" }}
}}

### 'session_state.json' – current work snapshot
Update: After EVERY completed task or significant progress.
JSON Format:
{{
  "last_updated": "2026-02-28T14:30:00Z",
  "session_number": 12,
  "current_task": "PDF export",
  "progress": ["Created pdf.ts", "Added API route"],
  "blockers": ["Multi‑page layout broken"],
  "next_steps": ["Fix layout", "Add tests"],
  "modified_files": ["src/lib/pdf.ts", "src/app/api/export/route.ts"],
  "important_context": "Use jsPDF, not Puppeteer"
}}

### 'decisions.json' – WHY we chose X over Y. Prevents re-debating solved problems.
Update: Append new entries. NEVER delete old ones.
JSON Format:
[
  {{
    "date": "2026-01-18",
    "decision": "Use jsPDF",
    "reason": "Lightweight, serverless friendly",
    "alternatives_considered": ["Puppeteer", "react-pdf"]
  }}
]

### 'patterns.json' – project conventions & preferences
Update: When new patterns are established or user corrects the AI.
JSON Format:
{{
  "code_style": ["Use 'use server' in separate files", "Result pattern"],
  "naming": ["Components: PascalCase", "Utilities: camelCase"],
  "preferences": ["User hates try/catch", "Verbose comments for complex logic"]
}}

### 'file_map.json' – What key files do so the AI doesn't need to re-read them.
Update: When significant files are created or changed.
JSON Format:
{{
  "src/lib/pdf.ts": "jsPDF generator, exports generateInvoicePDF()",
  "src/lib/auth.ts": "NextAuth config with GitHub/Google"
}}

### 'known_issues.json' – bugs & tech debt & warnings
Update: When issues are discovered or resolved.
JSON Format:
[
  {{
    "status": "OPEN",
    "issue": "PDF layout breaks >20 items",
    "file": "src/lib/pdf.ts",
    "notes": "Need page breaks"
  }}
]

### 'tasks.json' – persistent task tracker - never get lost
Update: When tasks are created, started, completed, or blocked.

⚠️ CRITICAL RULES:
- NEVER overwrite or replace the entire tasks structure
- NEVER remove tasks — only change their status
- ALWAYS append new tasks to the existing list
- Each task gets a unique ID (t1, t2, t3...) — IDs are PERMANENT
- When creating a new task list, set it as current_list and ADD tasks to the lists array

JSON Format:
{{
  "current_list": "Sprint 3 – Auth & Payments",
  "lists": [
    {{
      "name": "Sprint 3 – Auth & Payments",
      "created": "2026-03-05T01:00:00Z",
      "tasks": [
        {{
          "id": "t1",
          "title": "Create login page",
          "status": "completed",
          "created": "2026-03-05T01:00:00Z",
          "completed_at": "2026-03-05T01:45:00Z"
        }}
      ]
    }}
  ]
}}
- **IDs:** permanent, unique (t1, t2, …).
- **Statuses:** pending → in_progress → completed | blocked.
- **Transitions:** update status, add 'completed_at' or 'blocked_reason'.
- **New list:** append to 'lists', set 'current_list'. Never delete old lists.
- **When task completed naturally:** update tasks.json, add new follow‑up tasks.

When the user asks you to create a task list:
1. Read the current .memix/brain/tasks.json
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
[
  {{
    "session": 11,
    "date": "2025-01-14",
    "summary": "Built invoice CRUD.",
    "files_changed": ["src/actions/invoice.ts", "src/components/InvoiceForm.tsx"]
  }}
]

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
{{
  "session": 12,
  "tasks_completed": 3,
  "tasks_started": 1,
  "bugs_found": 1,
  "bugs_fixed": 0,
  "decisions_made": 2,
  "files_modified": 5,
  "brain_updates": 8
}}

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
            r#"# Memix Agent Protocol

## Overview
You are working with Memix, an autonomous AI memory bridge that provides persistent context across sessions.

## Your Capabilities
- Access to a persistent "brain" that remembers project architecture, decisions, and patterns
- Automatic tracking of your work across sessions
- Real-time code analysis and semantic understanding
- Predictive context loading before you ask

## Brain Access
The brain contains these key/value documents (often represented as JSON):
- **identity.json** - Project purpose, tech stack, architecture
- **session_state.json** - Current task, progress, blockers
- **decisions.json** - Architecture decisions and rationale
- **patterns.json** - Coding conventions and preferences
- **file_map.json** - Key file purposes and dependencies
- **known_issues.json** - Tracked bugs and technical debt
- **tasks.json** - Persistent task lists
- **session_log.json** - Historical session records

## Automatic Features
Memix automatically:
1. Watches your code changes in real-time
2. Computes semantic diffs (not just line changes)
3. Maintains a dependency graph of your codebase
4. Detects your intent from edit patterns
5. Pre-loads relevant context before you open chat
6. Tracks session activity for debugging

## Commands
- `brain status` - Get brain summary
- `save brain` - Force save current state
- `recap` - Verbal summary from memory
- `end session` - Save and log session

## Protocol
1. Read brain files at session start
2. Update brain after significant work
3. End every response with BRAIN CHECK footer
4. If interrupted, re-sync brain before continuing

---
Project: {project_id}"#,
            project_id = project_id
        )
    }

    pub fn generate_for_ide(
        project_id: &str,
        redis_url: Option<&str>,
        ide: IdeType,
        workspace_root: &str,
    ) -> RulesGenerationResult {
        let config = IdeRulesConfig::for_ide(ide);
        
        let mut brain_content = Self::generate_brain_template(project_id, redis_url);
        let guard_content = Self::generate_guard_template(project_id, &config.guard_file);
        
        // Add frontmatter for IDEs that require it
        let brain_file = match ide {
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
            guard_content: brain_file,
            workspace_root: workspace_root.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesGenerationResult {
    pub config: IdeRulesConfig,
    pub brain_content: String,
    pub guard_content: String,
    pub workspace_root: String,
}

impl RulesGenerationResult {
    pub fn write_files(&self) -> std::io::Result<()> {
        use std::fs;
        use std::path::{Path, PathBuf};
        
        // Canonicalize workspace root to ensure clean base path
        let workspace_root = Path::new(&self.workspace_root).canonicalize()?;
        
        // Strictly validate rules_dir to prevent absolute paths and parent directory traversal
        let rules_dir_path = Path::new(&self.config.rules_dir);
        if rules_dir_path.is_absolute() || rules_dir_path.components().any(|c| matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_)
        )) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "rules directory must be a clean relative path without traversal patterns",
            ));
        }

        // Validate rules_dir
        let mut rules_dir = workspace_root.join(&self.config.rules_dir);
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
        
        // Validate brain_path
        let brain_path = rules_dir.join(&self.config.rules_file);
        // We can't canonicalize brain_path yet because it might not exist, 
        // but it's safe because it's just a filename appended to a validated rules_dir
        // and we verify the file name doesn't contain path separators
        if self.config.rules_file.contains('/') || self.config.rules_file.contains('\\') {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid rules file name"
            ));
        }
        
        fs::write(&brain_path, &self.brain_content)?;
        
        if self.config.supports_multiple_files {
            if self.config.guard_file.contains('/') || self.config.guard_file.contains('\\') {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid guard file name"
                ));
            }
            let guard_path = rules_dir.join(&self.config.guard_file);
            fs::write(&guard_path, &self.guard_content)?;
            
            // Add companion link to brain file
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
        
        // Add to .gitignore
        Self::add_to_gitignore(&workspace_root, &self.config)?;
        
        Ok(())
    }
    
    fn add_to_gitignore(workspace_root: &std::path::Path, config: &IdeRulesConfig) -> std::io::Result<()> {
        use std::fs::{File, OpenOptions};
        use std::io::Write;
        
        let gitignore_path = workspace_root.join(".gitignore");
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
