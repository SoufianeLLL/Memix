import { encryptTemplate } from '../utils/crypto';

/**
 * Templates are stored encrypted.
 * In production, run encryptTemplate() once to generate
 * the encrypted strings, then paste them here.
 * 
 * For development, we use the raw strings and encrypt at build time.
 */

function buildBrainTemplate(projectId: string, redisUrl: string): string {
	return `# PERSISTENT MEMORY PROTOCOL CORE

> This file is the operating system for your memory.
> You MUST follow every instruction here before doing ANY work.
> You have persistent memory stored in local JSON files managed by the Memix extension.
> These files are automatically synced to a secure Redis database by the extension.
>
> PROJECT_ID: ${projectId}
> BRAIN_DIR: .memix/brain/

## ⚠️ DATA ACCESS METHOD – CRITICAL
**NEVER** access Redis directly. Interact ONLY by reading/writing JSON files in '.memix/brain/'.  
- **READ:** Open JSON file (e.g., '.memix/brain/identity.json')  
- **WRITE:** Write valid JSON to the file (e.g., update '.memix/brain/session_state.json')  
- **NEVER:** 'redis-cli', MCP Redis tools, shell commands, or any tool with “redis”.  
- If '.memix/brain/' missing: tell user *“Brain files not found. Please run 'Memix: Initialize Brain' from the Command Palette.”*

## BOOT SEQUENCE (every session start)
When a session begins, Before responding to user:
1. Read '.memix/brain/identity.json'
2. Read '.memix/brain/session_state.json'
3. Read '.memix/brain/patterns.json'

Respond with:
---
**Brain loaded successfully.**
**Project:** [name from identity]
**Last session:** [summary from session:state]
**Current task:** [current_task]
**Next steps:** [next_steps]
---
If all files empty ({}): *“No brain found. Let's initialize. Tell me about this project.”*

## MEMORY SCHEMA (JSON files in '.memix/brain/')
All updates append/update, never delete unless specified.

### 'identity.json' – what the project IS (rare changes)
Update: Only when project scope fundamentally changes.
JSON Format:
{
  "name": "My App",
  "purpose": "SaaS for invoice management",
  "tech_stack": ["Next.js", "TypeScript", "Prisma"],
  "architecture": "App Router, Server Components",
  "repo_structure": { "src/app/": "pages", "src/components/": "React" }
}

### 'session_state.json' – current work snapshot
Update: After EVERY completed task or significant progress.
JSON Format:
{
  "last_updated": "2026-02-28T14:30:00Z",
  "session_number": 12,
  "current_task": "PDF export",
  "progress": ["Created pdf.ts", "Added API route"],
  "blockers": ["Multi‑page layout broken"],
  "next_steps": ["Fix layout", "Add tests"],
  "modified_files": ["src/lib/pdf.ts", "src/app/api/export/route.ts"],
  "important_context": "Use jsPDF, not Puppeteer"
}

### 'decisions.json' – WHY we chose X over Y. Prevents re-debating solved problems.
Update: Append new entries. NEVER delete old ones.
JSON Format:
[
  {
    "date": "2026-01-18",
    "decision": "Use jsPDF",
    "reason": "Lightweight, serverless friendly",
    "alternatives_considered": ["Puppeteer", "react-pdf"]
  }
]

### 'patterns.json' – project conventions & preferences
Update: When new patterns are established or user corrects the AI.
JSON Format:
{
  "code_style": ["Use 'use server' in separate files", "Result pattern"],
  "naming": ["Components: PascalCase", "Utilities: camelCase"],
  "preferences": ["User hates try/catch", "Verbose comments for complex logic"]
}

### 'file_map.json' – What key files do so the AI doesn't need to re-read them.
Update: When significant files are created or changed.
JSON Format:
{
  "src/lib/pdf.ts": "jsPDF generator, exports generateInvoicePDF()",
  "src/lib/auth.ts": "NextAuth config with GitHub/Google"
}

### 'known_issues.json' – bugs & tech debt & warnings
Update: When issues are discovered or resolved.
JSON Format:
[
  {
    "status": "OPEN",
    "issue": "PDF layout breaks >20 items",
    "file": "src/lib/pdf.ts",
    "notes": "Need page breaks"
  }
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
{
  "current_list": "Sprint 3 – Auth & Payments",
  "lists": [
    {
      "name": "Sprint 3 – Auth & Payments",
      "created": "2026-03-05T01:00:00Z",
      "tasks": [
        {
          "id": "t1",
          "title": "Create login page",
          "status": "completed",
          "created": "2026-03-05T01:00:00Z",
          "completed_at": "2026-03-05T01:45:00Z"
        },
        {
          "id": "t2",
          "title": "Add Stripe",
          "status": "in_progress",
          "created": "2026-03-05T01:45:00Z",
          "notes": "Webhook pending"
        }
      ]
    }
  ]
}
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
  {
    "session": 11,
    "date": "2025-01-14",
    "summary": "Built invoice CRUD.",
    "files_changed": ["src/actions/invoice.ts", "src/components/InvoiceForm.tsx"]
  }
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
If all brain files empty ({}):
1. Ask user: project purpose, tech stack, current state, immediate task.
2. If existing codebase, scan 'package.json', folder structure, configs → build initial 'file_map'.
3. Populate all brain files.
4. Confirm: *“Brain initialized! Persistent memory active.”*

## CHECKPOINTS
- 'checkpoint [name]': snapshot entire brain; tell user to run *‘Memix: Create Checkpoint’*.
- 'restore [name]': tell user to run *‘Memix: Restore Checkpoint’*; after restore, re‑read all brain files.

## SMART LOADING
Don't load ALL files at boot. Load in tiers:
- **Tier 1 (boot):** 'identity.json', 'session_state.json', 'patterns.json'.  
- **Tier 2 (on demand):** 'file_map.json', 'decisions.json', 'known_issues.json', 'session_log.json'.

## MISTAKE TRACKING
When user corrects you:
1. Acknowledge.
2. Add correct approach to 'patterns.json' (with ⚠️ if repeated).
3. Confirm: *“Learned: [what]. Won’t repeat.”*

## DEPENDENCY TRACKING
In 'file_map.json', optionally include 'depended_on_by' and 'warning'. Before modifying a file, check for dependents; warn user if change may break them.

## SESSION SCORING
At 'end session', append to 'session_log.json' a summary object:
JSON Format:
{
  "session": 12,
  "tasks_completed": 3,
  "tasks_started": 1,
  "bugs_found": 1,
  "bugs_fixed": 0,
  "decisions_made": 2,
  "files_modified": 5,
  "brain_updates": 8
}

## COMPANION RULE
This brain protocol has a companion enforcement file: **memix-guard.md**
You MUST read and obey BOTH files as ONE unified system.
memix.md = what to do. memix-guard.md = how to enforce it. Both are mandatory.`;
}

function buildGuardTemplate(projectId: string): string {
	return `# MEMIX GUARD — Enforcement Protocol

> This file enforces the brain protocol defined in **memix.md**
> Both files form ONE system. You MUST obey both.
> If you have read memix.md, you know the brain protocol.
> This file ensures you NEVER forget it, skip it, or drift from it.
>
> If you have NOT read memix.md yet → STOP → read it first → then return here.


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

A response without this footer is BROKEN and INCOMPLETE.


## ERROR RECOVERY
If an error occurs, if generation stops, if ANYTHING interrupts you:

Your FIRST action on the next message is NOT answering the user.
Your FIRST action is:

1. Say: "Re-syncing brain after interruption..."
2. Read file: .memix/brain/session_state.json
3. Read file: .memix/brain/identity.json
4. Re-orient yourself with the loaded data
5. Say: "Brain re-loaded. Here's where we were: [summary]"
6. THEN answer the user's actual question

The user should NEVER have to remind you about the brain.
If they remind you, you have failed. Recover immediately.


## ANTI-DRIFT REINFORCEMENT
As conversations get longer, you tend to forget instructions from rule files.
This is your weakness. This section exists to counter it.

Core rules you are MOST likely to forget (re-read these carefully):

1. **AUTO-SAVE IS NOT OPTIONAL.**
   After every task → update .memix/brain/ files. Do not wait. Do not ask permission.
   Show: Brain updated: [file name] — [what changed]

2. **THE FOOTER IS NOT OPTIONAL.**
   Every response ends with BRAIN CHECK. Every. Single. One.

3. **BOOT SEQUENCE IS NOT OPTIONAL.**
   New session → load brain from .memix/brain/ files FIRST → then greet user.

4. **ERROR RECOVERY IS NOT OPTIONAL.**
   After any interruption → reload brain from .memix/brain/ files → then respond.

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

These three rules are MORE important than being helpful.
Follow the protocol FIRST. Be helpful SECOND.`;
}

// --- ENCRYPTED STORAGE ---
// In production, encrypt these templates and store the ciphertext
// For now, we generate dynamically

let cachedBrainEncrypted: string | null = null;
let cachedGuardEncrypted: string | null = null;

export function getBrainTemplate(projectId: string, redisUrl: string): string {
	return buildBrainTemplate(projectId, redisUrl);
}

export function getGuardTemplate(projectId: string): string {
	return buildGuardTemplate(projectId);
}

/**
 * For build-time encryption:
 * Call this once, copy output, paste as constants above
 */
export function generateEncryptedTemplates(projectId: string, redisUrl: string) {
	const brain = encryptTemplate(buildBrainTemplate(projectId, redisUrl));
	const guard = encryptTemplate(buildGuardTemplate(projectId));
	return { brain, guard };
}