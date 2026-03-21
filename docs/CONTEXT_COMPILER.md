# Context Compiler

## Overview

The **Context Compiler** is the daemon's core intelligence engine for building AI-ready context packets. It accepts a token budget and active file, traverses the structural indexes, and produces a prioritized, deduplicated set of context sections that fit within that budget — maximizing information value per token spent.

The compiler is the bridge between the daemon's rich structural knowledge (dependency graph, skeleton index, brain entries, session history, call graph, importance scores) and the lean context window that an AI model actually receives. Its output is the answer to the question: "given everything Memix knows about this project right now, what is the most valuable subset that fits in N tokens?"

## Problem & Solution

### Problem

Naively injecting all available information wastes tokens on irrelevant context, while being too selective causes the AI to miss critical structural relationships. Neither extreme serves the developer well, and both get worse as the project grows.

### Solution

A seven-pass compilation pipeline that progressively refines the candidate context from the full project graph down to a precisely budget-fitted set of sections. Each pass has a specific, bounded responsibility, and the passes are designed so that earlier passes reduce the work required by later ones.

## The Seven Passes

**Pass 1 — Dead Context Elimination** performs a BFS walk from the active file through the dependency graph, collecting all files reachable within `max_depth` hops (default 2). Files that have no path to the active file through imports or reverse-imports are excluded entirely. This pass also computes the `naive_token_estimate` — the sum of each relevant file's byte size multiplied by a 0.25 tokens-per-byte heuristic. This estimate represents what a developer would spend if they simply pasted every relevant file verbatim, and is used downstream to calculate tokens saved.

**Pass 2 — Skeleton Extraction** uses the AST parser to build a `CodeSkeleton` for each relevant file, capturing function signatures, type declarations, and exports. For the active file specifically, the two most complex functions (by cyclomatic complexity) have their full bodies included rather than just their signatures. This gives the AI enough detail to reason about the specific function under development while keeping all other files at summary level.

**Pass 3 — Brain Deduplication** checks each skeleton against the existing brain entries for the project. Skeletons whose content is substantially covered by three or more brain entries are truncated to a summary, on the principle that the AI should not receive the same information from two sources simultaneously. This pass reduces token usage on well-documented files while preserving detail for files the brain doesn't yet know about.

**Pass 4 — History Compaction** processes the session's flight recorder events into two distinct sections: a recent timeline covering the last four events (which preserves specificity about very recent actions) and an older aggregate summary (which preserves the count and pattern of earlier activity without repeating all the details). Keeping these as separate sections allows the budget-fitting pass to selectively include one or both based on available budget.

**Pass 5 — Rules Pruning** reads the workspace's rule files (AGENTS.md, .windsurfrules, and similar) and filters their content to lines that match task-type keywords. A bug-fix task sees warning, issue, debug, and safety lines; a refactor task sees dependency, decision, pattern, and architecture lines. This ensures the AI receives focused guidance rather than the entire rulebook, and is especially useful as project rule files grow over time.

**Pass 6 — Skeleton Index Injection** fetches FSI and FuSI entries from the skeleton Redis hash and adds them as ranked sections. FSI entries receive priority 85 and FuSI entries receive priority 78. Structural importance scores from the petgraph analysis are also applied here: files with high betweenness centrality and PageRank receive a priority boost of up to 15 points, ensuring load-bearing files in the dependency graph are more likely to survive budget fitting.

**Pass 7 — Budget Fitting (0/1 Knapsack)** solves the optimal section selection problem using dynamic programming. Each section has a token cost and a priority score; the knapsack algorithm maximizes total `priority × 100` value subject to the total token budget constraint. Sections not selected are tracked in `omitted_section_ids` for transparency. The algorithm is exact (not greedy), which matters at small budgets where a greedy approach might pick several medium-priority sections that together prevent a single high-priority section from fitting.

## Priority Hierarchy

The priority values determine relative importance during the knapsack pass. When budget is tight, higher-priority sections are selected first.

| Section Kind | Priority | Rationale |
|---|---|---|
| `active-context` | 100 | Always included — the active file and task framing |
| `code-skeleton` (active file) | 95 | Full structural detail for the file being edited |
| `causal-chain` | 95 | Call chain context when available |
| `skeleton-fsi` (importance-boosted) | Up to 100 | High-betweenness files get boosted |
| `skeleton-fsi` (baseline) | 85 | File-level structural summary |
| `history:recent` | 80 | Last 4 session events |
| `skeleton-fusi` | 78 | Function-level symbol detail |
| `code-skeleton` (dependency) | 72 | Dependency structural summaries |
| `rules` | 70 | Pruned project conventions |
| `history:summary` | 55 | Older session aggregate |

## Naive Token Estimate and Compression Ratio

Every compiled context response includes a `naive_token_estimate` field alongside `total_tokens`. This estimate is the sum of byte-to-token conversions for all files in the dependency frontier — the token cost of just pasting all relevant files verbatim. The ratio of `naive_token_estimate / total_tokens` is the compression ratio: how much more token-efficient the compiled context is versus the naive approach.

This ratio feeds directly into the Token Intelligence system. The difference between the naive estimate and the actual compiled token count is recorded as `estimated_tokens_saved`, which accumulates into both the session and lifetime totals visible in the debug panel.

## Metrics

Every compilation returns a `CompilePassMetrics` struct with: the number of relevant files discovered, code-skeleton sections built from the filesystem, skeleton index sections injected from Redis, files trimmed by brain dedup, history sections included, rules sections included, total sections ranked, and total sections fitted within budget. These metrics are surfaced in the debug panel under the Compiled Context section.

## Key File

`daemon/src/context/mod.rs` — the complete compiler implementation including all seven passes, the knapsack algorithm, skeleton rendering, and the rules pruning keyword tables.

## API Endpoint

`POST /api/v1/context/compile` accepts a JSON body with `project_id`, `active_file`, `token_budget`, an optional `task_type` (newfeature, bugfix, refactor, codereview), and an optional `max_depth`. It returns a `CompiledContext` with `budget`, `total_tokens`, `naive_token_estimate`, `explainability_summary`, `selected_sections`, `omitted_section_ids`, and `metrics`.