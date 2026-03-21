# Call Graph

## Overview

The **Call Graph** is a live in-memory data structure that tracks function-to-function call relationships across the entire codebase, enriched with precise file-level resolution when possible. It is updated incrementally on every file save and serves as the backbone for FuSI entry enrichment, causal chain analysis, and structural risk assessment.

## Problem & Solution

### Problem

When an AI agent looks at a function, it doesn't know which other functions call it, where those callers live, or which files those calls resolve to. Nominal call information (just function names) is not enough for accurate impact analysis — the same name can exist in multiple files, and without file resolution, the graph is ambiguous.

### Solution

A dual-index directed graph that tracks both **exact** call targets (when OXC semantic analysis resolves the callee's file and line number) and **nominal** call targets (when only the name is known). When an exact caller index is populated for a symbol, it takes precedence; the nominal index serves as a graceful fallback for dynamic dispatch, external libraries, and cases where OXC was not available or did not resolve the target.

## Architecture

The `CallGraph` struct maintains three internal indexes, all updated atomically per file save:

- `calls` maps each `(file, caller_symbol)` pair to a list of `ResolvedEdge` values describing what that symbol calls — including the callee's file path and line number when semantic resolution succeeded.
- `exact_callers` maps each `(callee_file, callee_symbol)` pair to the list of `CallerSite` structs that call it. This index is only populated when OXC resolved the callee's file.
- `symbol_callers` maps a bare callee symbol name to all call sites that reference it by name. This is the fallback path when file resolution is unavailable.

When querying callers of a symbol, the graph tries `exact_callers` first, falling back to `symbol_callers` only if the exact index is empty for that target. This ensures the best available information is always returned without requiring callers to know which tier they're querying.

## ResolvedEdge

The central improvement over a nominal call graph is the `ResolvedEdge` struct:

```
callee_file:   the file path where the callee is defined (empty if unresolved)
callee_symbol: the symbol name being called
callee_line:   the line number in callee_file (0 if unresolved)
is_method:     whether this is a method call (obj.method()) vs a direct call
```

When OXC analyzes a TypeScript or JavaScript file and successfully resolves an import to its source file, the edges for calls to symbols from that import are populated with full file and line information. This turns a nominal dependency graph into a resolved one, where every edge carries enough information to navigate to the exact declaration.

## Lifecycle

On each file save, the graph's `update_file` method is called with the file's path and a list of `(caller_symbol, Vec<ResolvedEdge>)` pairs. The method first removes all existing entries from `exact_callers` and `symbol_callers` that originated from this file, ensuring no stale edges accumulate across edits. It then inserts the fresh edges, populating both indexes as appropriate for each edge's resolution status.

When a file is deleted, `remove_file` performs the same stale-edge cleanup from both reverse indexes and then removes the file's entry from `calls`. This ensures the graph remains consistent even across rename and delete operations.

## Causal Context

The `causal_context_for_file` method builds a `FileCausalContext` struct that aggregates all outgoing and incoming call information for every symbol defined in a file. This struct is consumed by the context compiler and the orchestrator to produce call chain summaries like "function A calls B and C, and is itself called by D in file X." The `total_outgoing_edges` and `total_incoming_edges` fields give a quick measure of how "central" a file is in the call graph without requiring a full graph traversal.

## Sharing Between Event Loop and HTTP Handlers

Unlike in earlier versions where the call graph was local to the event loop task, it is now wrapped in an `Arc<Mutex<CallGraph>>` and stored in `AppState`. This allows HTTP endpoints to query causal context, blast radius data, and caller/callee information on demand through the daemon's API without needing to rebuild the graph from storage.

## Key File

`daemon/src/observer/call_graph.rs` — full implementation including `ResolvedEdge`, `CallerSite`, `SymbolCausalContext`, `FileCausalContext`, and all graph mutation and query methods.

## Tests

The test suite covers all core behaviors: basic update and query, update-replaces-previous (verifying stale edges are removed), file removal cleaning up reverse edges, the exact-caller path for fully resolved edges, and the empty-graph edge cases.