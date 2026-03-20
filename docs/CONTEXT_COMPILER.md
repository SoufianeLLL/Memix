# Context Compiler

## Overview

The **Context Compiler** is the daemon's core intelligence engine for building AI-ready context. It takes a token budget and produces a prioritized, deduplicated set of context sections that fit within that budget, maximizing information value.

## Problem & Solution

### Problem
AI models have limited context windows. Naively injecting all available information wastes tokens on irrelevant data, while being too selective causes the AI to miss critical context.

### Solution
A 7-pass compilation pipeline that:
1. Eliminates dead context (unreachable files)
2. Builds code skeletons (structural summaries)
3. Deduplicates against existing brain entries
4. Compacts session history
5. Prunes rules to task-relevant sections
6. Injects skeleton index entries (FSI/FuSI)
7. Uses dynamic-programming knapsack for optimal budget allocation

## Pipeline Passes

### Pass 1: Dead Context Elimination
BFS walk from the active file through the dependency graph. Only files within `max_depth` hops (default: 2) are considered relevant.

### Pass 2: Skeleton Extraction
For each relevant file, the AST parser extracts function signatures, types, and exports. The active file additionally includes the 2 most complex function bodies.

### Pass 3: Brain Dedup
Skeletons whose content is already well-covered by brain entries (≥3 coverage hits) are truncated to save tokens.

### Pass 4: History Compaction
Session history is split into an older summary (aggregate stats) and recent timeline (last 4 events), each as a separate section.

### Pass 5: Rules Pruning
Rule files (AGENTS.md, .windsurfrules) are filtered to lines matching task-type keywords (e.g., "pattern" for refactoring).

### Pass 6: Skeleton Index Injection
FSI and FuSI entries from the Redis skeleton hash are injected as ranked sections:

| Section Type | Priority | Description |
|-------------|----------|-------------|
| `skeleton-fsi` | 85 | File-level structural summary |
| `skeleton-fusi` | 78 | Function-level symbol detail |

### Pass 7: Budget Fitting (DP Knapsack)
All ranked sections compete for the token budget using a 0/1 knapsack algorithm that maximizes `priority × 100` value per token spent.

## Priority Hierarchy

| Section Kind | Priority | Meaning |
|-------------|----------|---------|
| `active-context` | 100 | Active file + task type |
| `code-skeleton` (active) | 95 | Code skeleton of active file |
| `skeleton-fsi` | 85 | File Skeleton Index entry |
| `skeleton-fusi` | 78 | Function Symbol Index entry |
| `history:recent` | 80 | Recent session events |
| `code-skeleton` (dep) | 72 | Code skeleton of dependency |
| `rules` | 70 | Project rules/conventions |
| `history:summary` | 55 | Older session summary |

## Metrics

The compiler returns `CompilePassMetrics` including:
- `relevant_files` — files within dependency reach
- `skeletons_built` — code-skeleton sections (filesystem-based)
- `skeleton_index_sections` — FSI/FuSI sections from Redis
- `deduplicated_files` — files trimmed due to brain coverage
- `fitted_sections` — sections selected within budget

## Key File

`daemon/src/context/mod.rs` — the entire compiler implementation.

## API Endpoint

`POST /api/v1/context/compile` with body:
```json
{
  "project_id": "my-project",
  "active_file": "/path/to/file.ts",
  "token_budget": 4000,
  "task_type": "bugfix",
  "max_depth": 2
}
```
