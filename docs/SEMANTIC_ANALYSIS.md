# Semantic Analysis (OXC)

## Overview

The **OXC Semantic Analysis** pass is Layer 2 of Memix's three-layer structural intelligence pipeline, running immediately after tree-sitter AST parsing for TypeScript and JavaScript files. Where tree-sitter tells you _where_ functions and imports are in the syntax tree, OXC tells you _what_ those imports actually resolve to — which exact file, and which exact symbol definition within that file.

This distinction matters enormously for the quality of the call graph. Without semantic resolution, the call graph contains only nominal edges: "function A calls something named validate." With OXC, the same edge becomes resolved: "function A at line 47 of `src/server.ts` calls `validate` which is defined at line 12 of `src/lib/input-validation.ts`." The resolved graph is what enables accurate blast radius computation, structural regression detection across file boundaries, and the causal chain analysis that makes the context compiler genuinely useful.

## What OXC Provides That Tree-Sitter Cannot

Tree-sitter is intentionally a language-agnostic parser. It produces a concrete syntax tree for any language it supports, but it does not understand module systems, type systems, or import resolution semantics. It cannot follow an `import { validate } from './lib'` statement and tell you what file `./lib` resolves to, let alone whether `validate` in that file is exported under the same name or re-exported from elsewhere.

OXC was designed from the ground up as a TypeScript-first toolchain in Rust, with a module resolver that understands TypeScript's full import resolution algorithm: `tsconfig.json` `paths` and `baseUrl` mappings, barrel file re-exports, `package.json` `exports` conditions, and the standard Node.js module resolution cascade. When OXC resolves an import, the resolved path is the actual file on disk that would be loaded at runtime.

OXC also performs scope analysis that tree-sitter does not. It builds a full scope tree for every file, tracking where each variable is declared, where each name refers to an imported symbol versus a locally declared one, and how closures capture outer scope names. This scope model is what allows OXC to answer the question "when this identifier `validate` is used in a call expression, is it the imported `validate` from `./lib`, or a locally declared function with the same name?"

## How the Analysis Works

When a TypeScript or JavaScript file is saved, and OXC is enabled (controlled by the `MEMIX_OXC_ENABLED` environment variable, default true), the `analyze_file` function in `observer/oxc_semantic.rs` is called with the file path, source code, and optional workspace root.

OXC parses the source code with its own parser (faster and stricter than tree-sitter for TypeScript), builds a semantic model, and then runs two extraction passes over the resulting AST.

The first pass builds a `ModuleIndex` by walking every `ImportDeclaration` statement in the program body. For each import, it calls the `Resolver` to turn the specifier string into a resolved file path, then records which local names bind to which exported symbols from which resolved file. Named imports, default imports, and namespace imports are all handled distinctly. The `MEMIX_OXC_RESOLVE_NODE_MODULES` environment variable (default false) controls whether imports that resolve into `node_modules` are included — they are excluded by default because they generate noise in the call graph without adding useful project-specific information.

The second pass walks all call expressions in the AST recursively, collecting them as `ResolvedCall` values. For each call, it uses the `ModuleIndex` built in the first pass to determine whether the callee is an imported symbol (and thus has a known source file), a locally declared function (and thus resolves to the current file), or an unknown runtime value (which stays unresolved). The result is a list of calls with as much resolution information as is statically determinable.

## Unresolved Import Detection

A valuable side effect of the resolution pass is detection of unresolved relative imports — import statements that begin with `.` or `/` but cannot be resolved to an existing file. These typically indicate a file that was renamed or deleted without updating all its importers. When OXC finds such imports, the daemon creates a warning `MemoryEntry` with the `dead-import` tag containing the specifier string and file path. This feeds into the known issues pipeline and surfaces in the debug panel.

Imports from bare specifiers (like `react`, `lodash`, or any package name without a path prefix) that fail to resolve are not flagged as warnings, because failure to resolve a package import usually means the package isn't installed rather than a structural issue in the project.

## The Source Cache

OXC's `resolve_symbol_line` function determines the line number of a symbol's declaration in its source file. To do this, it reads and parses the target file. Because a single file being saved might call dozens of imported symbols across several different files, naively reading each target file from disk for each call would produce dozens of synchronous disk reads per file-save event.

A `source_cache` parameter — a `HashMap<String, String>` scoped to each call to `analyze_file` — prevents repeated reads. Before reading from disk, the function checks the cache. The cache is pre-populated with the source code of the file currently being analyzed (since it's common for functions in a file to call each other, and those calls would also try to resolve the current file). The cache is stack-allocated and dropped when `analyze_file` returns, so there is no memory accumulation across different file-save events.

## Integration with the Call Graph

The results of OXC analysis are converted into `ResolvedEdge` values before being passed to `CallGraph::update_file`. If OXC successfully resolved a callee's file and symbol, the `ResolvedEdge` carries `callee_file` and `callee_line` information. If resolution failed (dynamic calls, external APIs, or cases where the scope analysis didn't yield a binding), a `ResolvedEdge::new_unresolved` is used, which carries only the symbol name and leaves the file and line fields empty.

The call graph's dual-index architecture (described in the Call Graph documentation) is specifically designed to handle this mixture of resolved and unresolved edges gracefully. Resolved edges populate the `exact_callers` index for precise queries; unresolved edges populate only `symbol_callers` for name-based fallback queries. A call site that starts as nominal can become resolved on the next file save once OXC succeeds in resolving its import, and the call graph will automatically reflect the improvement.

## Graceful Fallback

If OXC analysis fails — due to a parse error, an unsupported syntax construct, or any other exception — the daemon falls back to the tree-sitter-only import extraction path (`extract_imports` in `observer/imports.rs`). OXC parse errors are logged at the `trace` level rather than `warn` or `error`, because some TypeScript files use experimental syntax or decorators that OXC's strict parser rejects. These files still get structural analysis from tree-sitter; they just don't get the resolved import layer.

The `is_oxc_supported` function gates analysis to `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, and `.cjs` files. Rust, Python, Go, and other languages handled by tree-sitter do not have an OXC pass, since OXC is a TypeScript/JavaScript-specific tool. For those languages, Layer 1 and Layer 3 still apply — structural extraction and semantic embeddings are language-agnostic.

## Key File

`daemon/src/observer/oxc_semantic.rs` contains the complete analysis pipeline including `analyze_file`, `build_module_index`, `collect_local_symbols`, `extract_calls_from_ast`, `resolve_call_target`, and `resolve_symbol_line` with its source cache integration.