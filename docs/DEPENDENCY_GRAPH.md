# Dependency Graph

## Overview

The **Dependency Graph** is a live directed adjacency structure that maps how every source file in the workspace depends on every other. It is updated on every file save, consulted by the context compiler for dead-context elimination, used by the skeleton index to populate `depends_on` and `depended_by` fields, and analyzed by the importance scoring system to identify structurally critical files.

## Structure

The graph maintains two parallel adjacency maps rather than a single one. `edges_out` maps each file path to the set of file paths it imports or depends on — its outgoing edges. `edges_in` maps each file path to the set of file paths that import it — its incoming edges. Maintaining both directions explicitly avoids the need for full graph traversal when answering either "what does this file depend on?" or "what depends on this file?", making both queries O(1).

The `set_dependencies` method is the primary mutation path. It first removes all of a file's existing outgoing edges (and their corresponding reverse entries in `edges_in`), then adds the fresh set. This ensures that when a file's imports change — for instance, when an old import is removed — the graph does not accumulate stale edges. The explicit cleanup step before adding new edges is what keeps the graph consistent across refactors.

## Petgraph Integration

The `to_petgraph` method converts the storage-optimized adjacency maps into a `petgraph::DiGraph` on demand. This conversion is not done on every file save — it would be wasteful to rebuild a full petgraph structure for every single edit. Instead, it is done on demand when graph-algorithm-level analysis is needed, such as when the importance scores are computed or when a route handler needs topological ordering.

The conversion is straightforward: every file that appears in either `edges_out` or `edges_in` is given a node, and each edge in `edges_out` becomes a directed edge in the petgraph. Nodes are string file paths. The returned `HashMap<String, NodeIndex>` allows callers to look up a specific node index by file path, and the inverse mapping (built internally) allows results to be mapped back to file paths after algorithm execution.

## Importance Scoring

The `importance_scores` method delegates to `compute_importance` in `observer/importance.rs`, passing the `edges_out` map and a `top_n` parameter. This produces an `ImportanceScores` struct covering betweenness centrality (Brandes' algorithm), PageRank, strongly connected components (Tarjan's algorithm), topological ordering, and a combined top-N ranking.

Betweenness centrality is the most computationally expensive metric, running in O(V × E) time using Brandes' algorithm. For a project with 500 files and typical sparsity, this is still sub-millisecond on modern hardware. The result is normalized to [0, 1] across the graph and combined with PageRank using a 0.6/0.4 weighting to produce the final importance score. Files with high betweenness are "load-bearing" — many dependency paths pass through them — and receive priority boosts in the context compiler.

## Blast Radius

The `compute_blast_radius` function in `observer/importance.rs` performs a forward BFS through `edges_in` (the reverse dependency graph) to find all files transitively affected if a given file changes. The result includes each affected file's path, its depth from the source (how many hops of dependency separate it from the changed file), and the `via` field indicating which file is the immediate cause of its inclusion.

The critical path reconstruction traces the longest dependency chain from source to the most deeply affected file. A `visited_in_reconstruction` HashSet prevents the reconstruction from entering an infinite loop on projects with circular dependencies — a correctness fix for graphs with cycles that the BFS expansion handles separately through the `seen` set.

## Key File

`daemon/src/observer/graph.rs` contains the `DependencyGraph` struct with all mutation methods, the petgraph conversion, and the importance scoring delegation. `daemon/src/observer/importance.rs` contains `compute_importance`, `compute_blast_radius`, `build_digraph`, `compute_betweenness`, and `compute_pagerank` with their full test suites.

## Tests

The importance test suite validates betweenness centrality rankings on a linear chain (middle nodes must outrank endpoints), cycle detection via the SCC groups (a three-node cycle must be detected), the star topology PageRank behavior (a hub node must outrank leaf nodes), and blast radius calculation at various depth limits.