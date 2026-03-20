# Call Graph

## Overview

The **Call Graph** is an in-memory data structure that tracks function-to-function call relationships across the codebase. It's updated incrementally on each file save and used by the skeleton index to enrich function symbol entries with caller/callee information.

## Problem & Solution

### Problem
When an AI agent looks at a function, it doesn't know which other functions call it or which functions it calls. This makes refactoring advice, impact analysis, and dependency understanding shallow.

### Solution
An in-memory directed graph where:
- **Nodes** are function names (within their file context)
- **Edges** represent call relationships extracted from the AST

The graph is updated incrementally — only the saved file's call sites are recomputed.

## How It Works

### Call Site Extraction (`extract_call_sites`)
During AST parsing, the `extract_call_sites()` function walks tree-sitter nodes to find:
- `call_expression` nodes (e.g., `doSomething()`)
- Method calls (e.g., `obj.method()`)

Each function/method node is capped at **15 call sites** to prevent excessive data from deeply nested code.

### Graph Updates
On each file save:
1. The file's old entries are removed from the graph
2. New call symbols are extracted from the fresh AST features
3. `call_graph.update_file(path, symbols)` replaces the file's entries

### Graph Queries
The `CallGraph` supports:
- `callers_of(callee)` — who calls this function?
- `callees_of(caller)` — what does this function call?
- `file_functions(path)` — all functions defined in a file

### Rendering in FuSI
When generating Function Symbol Index entries, the call graph contributes:
- `calls: [list of callees]` — functions this symbol calls
- `called_by: [list of callers]` — functions that call this symbol (capped at 10)

## Key File

`daemon/src/observer/call_graph.rs`

## Data Structure

```rust
pub struct CallGraph {
    callers: HashMap<String, HashSet<String>>,  // callee → {callers}
    callees: HashMap<String, HashSet<String>>,  // caller → {callees}
    file_symbols: HashMap<String, Vec<String>>, // file → [defined functions]
}
```

## Tests

4 unit tests verify:
- Basic update + query
- File removal cleans up reverse edges
- Overwrite replaces old data
- Empty file is a no-op
