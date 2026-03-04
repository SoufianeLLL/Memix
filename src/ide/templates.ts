import { encryptTemplate } from '../utils/crypto';

/**
 * Templates are stored encrypted.
 * In production, run encryptTemplate() once to generate
 * the encrypted strings, then paste them here.
 * 
 * For development, we use the raw strings and encrypt at build time.
 */

function buildBrainTemplate(projectId: string, redisUrl: string): string {
	return `# PERSISTENT MEMORY PROTOCOL CORE — READ THIS FIRST

> This file is the operating system for your memory.
> You MUST follow every instruction here before doing ANY work.
> You have access to a Redis server called "memix-brain".
> This gives you persistent memory that survives across sessions.
>
> PROJECT_ID: ${projectId}
> BRAIN_PREFIX: brain:${projectId}


## BOOT SEQUENCE (run on EVERY session start)
When a session begins, BEFORE responding to the user's first message:

1. Call Redis tool → GET key: {BRAIN_PREFIX}:identity
2. Call Redis tool → GET key: {BRAIN_PREFIX}:session:state
3. Call Redis tool → GET key: {BRAIN_PREFIX}:patterns

After loading, respond with this format:
---
**Brain loaded successfully.**
**Project:** [name from identity]
**Last session:** [summary from session:state]
**Current task:** [current_task from session:state]
**Next steps:** [next_steps from session:state]
---

If ALL keys return null/empty → this is a brand new project.
Say: "No brain found. Let's initialize. Tell me about this project."


## MEMORY SCHEMA
All data is stored as JSON strings in Redis. Here are the keys:

### KEY: {BRAIN_PREFIX}:identity
Purpose: What this project IS. Rarely changes.
Update: Only when project scope fundamentally changes.

JSON Format:
{
	"name": "My App",
	"purpose": "A SaaS platform for invoice management",
	"tech_stack": ["Next.js", "TypeScript", "Prisma", "PostgreSQL", "Tailwind"],
	"architecture": "App Router, Server Components, API routes for webhooks",
	"repo_structure": {
		"src/app/": "Next.js app router pages",
		"src/components/": "React components",
		"src/lib/": "Utilities, DB client, helpers"
	}
}

### KEY: {BRAIN_PREFIX}:session:state
Purpose: Current work snapshot. THE MOST IMPORTANT KEY.
Update: After EVERY completed task or significant progress.

JSON Format:
{
	"last_updated": "2026-02-28T14:30:00Z",
	"session_number": 12,
	"current_task": "Building the invoice PDF export feature",
	"progress": [
		"Created PDF generation utility in src/lib/pdf.ts",
		"Added /api/invoices/[id]/export route",
		"Frontend button triggers download"
	],
	"blockers": [
		"PDF styling breaks on multi-page invoices"
	],
	"next_steps": [
		"Fix multi-page PDF layout",
		"Add company logo to PDF header",
		"Write tests for PDF generation"
	],
	"modified_files": [
		"src/lib/pdf.ts",
		"src/app/api/invoices/[id]/export/route.ts",
		"src/components/InvoiceActions.tsx"
	],
	"important_context": "User prefers jsPDF over Puppeteer for PDF generation. Keep it lightweight."
}

### KEY: {BRAIN_PREFIX}:decisions
Purpose: WHY we chose X over Y. Prevents re-debating solved problems.
Update: Append new entries. NEVER delete old ones.
Format: JSON array of decision objects.

JSON Format:
[
	{
		"date": "2026-01-18",
		"decision": "Use jsPDF instead of Puppeteer for PDF generation",
		"reason": "Lighter weight, no headless browser needed, works in serverless",
		"alternatives_considered": ["Puppeteer", "react-pdf", "pdfkit"]
	},
	...
]

### KEY: {BRAIN_PREFIX}:patterns
Purpose: Coding conventions and rules specific to THIS project.
Update: When new patterns are established or user corrects the AI.

JSON Format:
{
	"code_style": [
		"Use 'use server' directive in separate action files, not inline",
		"Always use Result pattern: { success: true, data } | { success: false, error }",
		"Prefer named exports over default exports",
		"Use Zod for ALL input validation"
	],
	"naming": [
		"Components: PascalCase (InvoiceList.tsx)",
		"Utilities: camelCase (formatCurrency.ts)",
		"API routes: kebab-case folders",
		"DB models: PascalCase singular (Invoice, not Invoices)"
	],
	"preferences": [
		"User hates try/catch — use Result pattern",
		"User wants verbose comments on complex logic",
		"Always suggest tests after new utility functions"
	]
}

### KEY: {BRAIN_PREFIX}:file_map
Purpose: What key files do so the AI doesn't need to re-read them.
Update: When significant files are created or changed.

JSON Format:
{
	"src/lib/pdf.ts": "PDF generation using jsPDF. Exports generateInvoicePDF(invoice: Invoice): Buffer",
	"src/lib/auth.ts": "NextAuth config. Uses GitHub + Google providers. Session includes user.id and user.role",
	"src/actions/invoice.ts": "Server actions: createInvoice, updateInvoice, deleteInvoice, sendInvoice",
	"src/components/InvoiceForm.tsx": "Form with Zod validation. Uses react-hook-form. Handles create + edit modes."
}

### KEY: {BRAIN_PREFIX}:known_issues
Purpose: Track bugs, tech debt, and warnings.
Update: When issues are discovered or resolved.

JSON Format:
[
	{
		"status": "OPEN",
		"issue": "PDF layout breaks on invoices with more than 20 line items",
		"file": "src/lib/pdf.ts",
		"notes": "Need to implement page break logic"
	},
	{
		"status": "FIXED",
		"issue": "Auth redirect loop on /dashboard when session expires",
		"file": "src/middleware.ts",
		"fixed_date": "2025-01-13",
		"solution": "Added session check before redirect in middleware"
	}
]

### KEY: {BRAIN_PREFIX}:session:log
Purpose: Historical record of all sessions.
Update: End of each session — append, never overwrite.

JSON Format:
[
	{
		"session": 11,
		"date": "2025-01-14",
		"summary": "Built invoice CRUD operations. Created server actions, form component with validation, and list view with pagination.",
		"files_changed": ["src/actions/invoice.ts", "src/components/InvoiceForm.tsx", "src/components/InvoiceList.tsx"]
	},
	{
		"session": 12,
		"date": "2025-01-15",
		"summary": "Started PDF export feature. Basic generation works but multi-page layout needs fixing.",
		"files_changed": ["src/lib/pdf.ts", "src/app/api/invoices/[id]/export/route.ts"]
	}
]

## AUTO - SAVE PROTOCOL
You MUST update the brain automatically at these trigger points:

### After completing any task:
→ Update {BRAIN_PREFIX}:session: state with new progress and next steps

### After creating or significantly modifying a file:
→ Update {BRAIN_PREFIX}:file_map with the file's purpose

### After making a design / architecture decision:
→ Append to {BRAIN_PREFIX}:decisions

### After user corrects your approach or states a preference:
→ Update {BRAIN_PREFIX}:patterns

### After discovering a bug or fixing one:
→ Update {BRAIN_PREFIX}:known_issues

### When session:state exceeds 3000 characters:
→ Summarize older progress into {BRAIN_PREFIX}:session:log
→ Keep only the latest context in session: state

### AUTO - SAVE CONFIRMATION:
After every brain update, show a brief confirmation:
** Brain updated:** [key name] —[what changed]

Do NOT ask permission to save.Just save.The user expects it.


## VOICE COMMANDS
When the user says any of these phrases, perform the associated action:

| Command | Action |
| ---------| --------|
| 'brain status' | GET all brain keys, display summary of what's stored |
| 'save brain' | Force update ALL keys with current state |
| 'show brain' | GET and display the FULL content of every key |
| 'clear brain' | Ask for confirmation, then DELETE all { BRAIN_PREFIX }:* keys |
| 'brain diff' | Show what has changed since the last save |
| 'teach brain: [info]' | Store the provided info in the appropriate key |
| 'forget: [info]' | Remove specific info from the appropriate key |
| 'recap' | Give a full verbal summary of the project from brain |
| 'end session' | Full brain sync + session log entry + goodbye summary |
| 'debug brain' | Show raw JSON of all keys for troubleshooting |
| 'rollback brain' | Restore session:state from the last session:log entry |
| 'brain health' | Check all keys exist and report any missing/empty ones |
| 'follow protocol' | Re-read rules file, check if any saves were missed, catch up |
| 'reboot brain' | Full reload: GET all Tier 1 keys from Redis, re-orient, confirm |
| 're - read rules' | Acknowledge every section of this rules file and confirm compliance |

## SAFETY RULES

1. NEVER store in the brain:
   - API keys, tokens, passwords, secrets
   - .env file contents
   - Personal user data (emails, names, addresses)
   - Entire file contents (store summaries + paths instead)

2. Size limits:
   - Each key value: MAX 4000 characters
   - If a key exceeds this, summarize and archive old data
   - session:log: Keep last 20 sessions max, archive older ones

3. Error handling:
   - If Redis fails to connect → tell the user immediately
   - If a key returns null unexpectedly → warn and offer to rebuild
   - NEVER silently fail on brain operations

4. Conflicts:
   - If brain state contradicts what you see in actual files → 
     trust the FILES, update the brain
   - Always verify critical brain info against actual code


## FIRST-TIME INITIALIZATION
If this is a new project (all brain keys return null), follow this process:

1. Ask the user:
   - What is this project?
   - What tech stack are you using?
   - What's the current state? (new project vs existing codebase)
   - What are you working on right now?

2. If existing codebase, scan the project:
   - Read package.json for dependencies
   - Read the folder structure
   - Read key config files (tsconfig, prisma schema, etc.)
   - Build the initial file_map

3. Populate ALL brain keys with gathered information

4. Confirm:
   "Brain initialized! I now have persistent memory for this project.
   Everything we discuss and build will be remembered across sessions."


## CHECKPOINTS
When the user says 'checkpoint[name]':
1. Snapshot the ENTIRE brain state
2. Store as: {BRAIN_PREFIX}:checkpoint:[name]
3. Format: JSON with all current key values + timestamp

When the user says 'restore checkpoint[name]':
1. Load the checkpoint
2. Overwrite current state with checkpoint data
3. Confirm what was restored

Use this before risky refactors or major changes.


## SMART LOADING
Don't load ALL keys at boot. Load in tiers:

**Tier 1 — Always load (boot sequence):**
- {BRAIN_PREFIX}:identity
- {BRAIN_PREFIX}:session:state
- {BRAIN_PREFIX}:patterns

**Tier 2 — Load on demand (when relevant):**
- {BRAIN_PREFIX}:file_map → Load when asked about file structure
- {BRAIN_PREFIX}:decisions → Load when making architecture choices
- {BRAIN_PREFIX}:known_issues → Load when debugging
- {BRAIN_PREFIX}:session:log → Load when user asks for history


## MISTAKE TRACKING
When the user corrects you or says something like:
- "No, don't do it that way"
- "I told you before..."  
- "Wrong approach"
- "That's not how we do it here"

1. Acknowledge the correction
2. Add the correct approach to {BRAIN_PREFIX}:patterns
3. If it was a repeated mistake, add emphasis: "⚠️ IMPORTANT: ..."
4. **Confirm:** "Learned: [what you learned]. Won't make this mistake again."


## DEPENDENCY TRACKING
Maintain awareness of what depends on what:

In {BRAIN_PREFIX}:file_map, include dependency notes:
JSON Format:
{
    "src/lib/db.ts": {
        "purpose": "Prisma client singleton",
            "depended_on_by": ["all server actions", "all API routes"],
                "warning": "Changing this affects the entire backend"
    }
}

Before modifying any file, check file_map for dependency warnings.
If a change could break dependents, warn the user BEFORE making changes.


## SESSION SCORING
At the end of each session (when user says "end session"), include:

JSON FORMAT:
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

Append this to the session:log entry. Over time, this creates
a productivity dashboard the user can review.

---
## COMPANION RULE: memix-guard.md
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
3. If I completed a task — did I call Redis to update session:state?
4. If I modified a file — did I update file_map in Redis?
5. If I made a decision — did I append to decisions in Redis?
6. If user corrected me — did I update patterns in Redis?

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
save to Redis — STOP — do the save FIRST — then write the footer.

This footer is your self-enforcement mechanism.
It keeps the brain protocol alive throughout the entire session.

A response without this footer is BROKEN and INCOMPLETE.


## ERROR RECOVERY
If an error occurs, if generation stops, if ANYTHING interrupts you:

Your FIRST action on the next message is NOT answering the user.
Your FIRST action is:

1. Say: "Re-syncing brain after interruption..."
2. Call Redis → GET brain:{project}:session:state
3. Call Redis → GET brain:{project}:identity
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
   After every task → update Redis. Do not wait. Do not ask permission.
   Show: Brain updated: [key] — [what changed]

2. **THE FOOTER IS NOT OPTIONAL.**
   Every response ends with BRAIN CHECK. Every. Single. One.

3. **BOOT SEQUENCE IS NOT OPTIONAL.**
   New session → load brain from Redis FIRST → then greet user.

4. **ERROR RECOVERY IS NOT OPTIONAL.**
   After any interruption → reload brain from Redis → then respond.

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

→ Save to Redis after every task
→ End every response with BRAIN CHECK footer
→ If lost, call Redis to reload brain

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