# Context Orchestrator

> The layer that turns raw developer questions into structurally-enriched prompts before they reach any AI model.

## Problem It Solves

When a developer asks an AI "why is my license validation failing?", the AI has no structural knowledge of the codebase. It discovers relevant files through expensive tool calls — each one is a full API round-trip that carries the entire conversation history. A 10-step discovery costs ~55 context-window-loads of tokens.

The Orchestrator eliminates this by front-loading the discovery work locally, before the question reaches the model. The AI receives a single rich prompt containing all structural context it needs, enabling a one-shot answer with zero tool calls.

---

## Architecture

### Input: OrchestrateRequest

```rust
pub struct OrchestrateRequest {
    pub prompt: String,           // The raw question
    pub project_id: String,       // Project to compile for
    pub active_file: String,      // Current editor file (dependency anchor)
    pub context_budget: usize,    // Max tokens (default: 3000)
    pub task_type: Option<String>,// newfeature, bugfix, refactor, review
    pub max_depth: Option<usize>, // Graph traversal depth (default: 3)
}
```

### Processing Pipeline

1. **Context Compiler Invocation** — Passes the prompt as a query hint for relevance ranking
2. **Section Assembly** — Groups compiled sections by kind (code, history, rules, skeleton)
3. **Prompt Rendering** — Wraps context in a structured header for AI consumption

### Output: OrchestrateResponse

```rust
pub struct OrchestrateResponse {
    pub enhanced_prompt: String,      // Final enriched prompt
    pub sections_used: usize,         // How many sections included
    pub compiled_tokens: usize,      // Actual token count
    pub naive_estimate: usize,       // What raw paste would cost
    pub compression_ratio: f64,      // Efficiency multiplier
    pub relevant_files: Vec<String>, // Files whose skeletons were used
}
```

---

## Query-Aware Context Selection

The orchestrator uses the developer's prompt to boost relevant sections:

1. **Term Extraction** — Splits prompt into meaningful words (≥3 chars)
2. **Content Matching** — Sections containing query terms get +0–10 priority boost
3. **Budget Fitting** — DP knapsack selects optimal sections within token budget

### Example

Query: `"Are we using any registration system?"`

- Sections containing "registration", "system", "auth" → priority boost
- Irrelevant sections (e.g., generic AGENTS.md instructions) → filtered out
- Result: Only code skeletons and rules relevant to auth/registration are included

---

## Rules Pruning Strategy

The orchestrator excludes generic AI instruction files (AGENTS.md) from context compilation. These are agent prompts, not project-specific coding conventions.

**Included rule files:**
- `.windsurfrules`
- `.cursorrules`
- `.github/copilot-instructions.md`
- `.rules/*.md`, `.rules/*.mdc`, `.rules/*.toml`

**Excluded:**
- `AGENTS.md` — Generic agent protocol, rarely relevant to specific queries

---

## Output Format

```
MEMIX STRUCTURAL CONTEXT — {n} sections

### Active Context
{active file and task type}

### Code Structure
{skeletons for relevant files}

### Project Rules & Conventions
{pruned rules matching query}

---

QUESTION:
{original prompt}
```

---

## Integration Points

- **Daemon API:** `POST /api/v1/context/orchestrate`
- **Extension:** Called automatically when user sends prompt to AI
- **Brain Entries:** Uses decisions, patterns, and session state for enrichment

---

## Performance Characteristics

| Metric | Value |
|--------|-------|
| Typical compression | 3–10× vs raw paste |
| Token budget default | 3000 |
| Max graph depth | 3 hops from active file |
| Compilation time | <100ms for most projects |

---

## Related Documentation

- [Context Compiler](./CONTEXT_COMPILER.md)
- [Dependency Graph](./DEPENDENCY_GRAPH.md)
- [Code Skeleton Index](./CODE_SKELETON_INDEX.md)
