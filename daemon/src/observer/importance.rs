use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::{tarjan_scc, toposort};
use petgraph::Direction;
use serde::{Deserialize, Serialize};

/// Structural importance scores computed from the dependency graph.
/// Uses the same graph algorithms that power Cargo's dependency resolver.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportanceScores {
    /// Betweenness centrality: how many shortest paths pass through each file.
    /// High score = "load-bearing" file that many dependency chains flow through.
    pub betweenness: HashMap<String, f64>,

    /// PageRank-style score: files that are imported by many important files score higher.
    pub pagerank: HashMap<String, f64>,

    /// Strongly connected components (Tarjan's algorithm).
    /// Each inner Vec is a group of files in a circular dependency cluster.
    pub scc_groups: Vec<Vec<String>>,

    /// Files in circular dependencies (flattened from scc_groups for quick lookup).
    pub circular_files: Vec<String>,

    /// Topological ordering — files ordered by dependency depth.
    /// Foundation files appear first, leaf files appear last.
    pub topological_order: Vec<String>,

    /// Top N most structurally important files by combined score.
    pub top_files: Vec<(String, f64)>,
}

/// Build a petgraph DiGraph from raw adjacency edges.
pub fn build_digraph(
    edges_out: &HashMap<String, std::collections::HashSet<String>>,
) -> (DiGraph<String, ()>, HashMap<String, NodeIndex>) {
    let mut graph = DiGraph::<String, ()>::new();
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

    // Collect all unique nodes
    for (source, targets) in edges_out {
        if !node_map.contains_key(source) {
            let idx = graph.add_node(source.clone());
            node_map.insert(source.clone(), idx);
        }
        for target in targets {
            if !node_map.contains_key(target) {
                let idx = graph.add_node(target.clone());
                node_map.insert(target.clone(), idx);
            }
        }
    }

    // Add edges
    for (source, targets) in edges_out {
        if let Some(&src_idx) = node_map.get(source) {
            for target in targets {
                if let Some(&tgt_idx) = node_map.get(target) {
                    graph.add_edge(src_idx, tgt_idx, ());
                }
            }
        }
    }

    (graph, node_map)
}

/// Compute betweenness centrality via Brandes' algorithm.
/// Returns a score per node normalized to [0.0, 1.0].
fn compute_betweenness(
    graph: &DiGraph<String, ()>,
    node_map: &HashMap<String, NodeIndex>,
) -> HashMap<String, f64> {
    let node_count = graph.node_count();
    if node_count < 2 {
        return node_map.keys().map(|k| (k.clone(), 0.0)).collect();
    }

    // Brandes' algorithm for unweighted directed graphs — O(V × E)
    let mut centrality: HashMap<NodeIndex, f64> = graph.node_indices().map(|n| (n, 0.0)).collect();

    for s in graph.node_indices() {
        // BFS from s
        let mut stack: Vec<NodeIndex> = Vec::new();
        let mut predecessors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
        let mut sigma: HashMap<NodeIndex, f64> = graph.node_indices().map(|n| (n, 0.0)).collect();
        let mut dist: HashMap<NodeIndex, i64> = graph.node_indices().map(|n| (n, -1)).collect();
        
        *sigma.get_mut(&s).unwrap() = 1.0;
        *dist.get_mut(&s).unwrap() = 0;
        
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            stack.push(v);
            let d_v = dist[&v];
            for neighbor in graph.neighbors_directed(v, Direction::Outgoing) {
                if dist[&neighbor] < 0 {
                    queue.push_back(neighbor);
                    *dist.get_mut(&neighbor).unwrap() = d_v + 1;
                }
                if dist[&neighbor] == d_v + 1 {
                    *sigma.get_mut(&neighbor).unwrap() += sigma[&v];
                    predecessors.entry(neighbor).or_default().push(v);
                }
            }
        }

        // Accumulation
        let mut delta: HashMap<NodeIndex, f64> = graph.node_indices().map(|n| (n, 0.0)).collect();
        while let Some(w) = stack.pop() {
            if let Some(preds) = predecessors.get(&w) {
                for &v in preds {
                    let d = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                    *delta.get_mut(&v).unwrap() += d;
                }
            }
            if w != s {
                *centrality.get_mut(&w).unwrap() += delta[&w];
            }
        }
    }

    // Normalize to [0, 1]
    let max_val = centrality.values().cloned().fold(0.0_f64, f64::max);
    let normalizer = if max_val > 0.0 { max_val } else { 1.0 };

    let reverse_map: HashMap<NodeIndex, String> = node_map.iter().map(|(k, &v)| (v, k.clone())).collect();
    centrality
        .into_iter()
        .filter_map(|(idx, score)| {
            reverse_map.get(&idx).map(|name| (name.clone(), score / normalizer))
        })
        .collect()
}

/// Simple iterative PageRank — how many important files depend on each file.
fn compute_pagerank(
    graph: &DiGraph<String, ()>,
    node_map: &HashMap<String, NodeIndex>,
    iterations: usize,
    damping: f64,
) -> HashMap<String, f64> {
    let n = graph.node_count();
    if n == 0 {
        return HashMap::new();
    }

    let base = (1.0 - damping) / n as f64;
    let mut rank: HashMap<NodeIndex, f64> = graph.node_indices().map(|idx| (idx, 1.0 / n as f64)).collect();

    for _ in 0..iterations {
        let mut new_rank: HashMap<NodeIndex, f64> = graph.node_indices().map(|idx| (idx, base)).collect();
        for node in graph.node_indices() {
            let out_degree = graph.neighbors_directed(node, Direction::Outgoing).count();
            if out_degree == 0 {
                continue;
            }
            let share = rank[&node] * damping / out_degree as f64;
            for neighbor in graph.neighbors_directed(node, Direction::Outgoing) {
                *new_rank.get_mut(&neighbor).unwrap() += share;
            }
        }
        rank = new_rank;
    }

    // Normalize
    let max_val = rank.values().cloned().fold(0.0_f64, f64::max);
    let normalizer = if max_val > 0.0 { max_val } else { 1.0 };

    let reverse_map: HashMap<NodeIndex, String> = node_map.iter().map(|(k, &v)| (v, k.clone())).collect();
    rank.into_iter()
        .filter_map(|(idx, score)| {
            reverse_map.get(&idx).map(|name| (name.clone(), score / normalizer))
        })
        .collect()
}

/// Compute all structural importance metrics from a dependency graph.
pub fn compute_importance(
    edges_out: &HashMap<String, std::collections::HashSet<String>>,
    top_n: usize,
) -> ImportanceScores {
    if edges_out.is_empty() {
        return ImportanceScores::default();
    }

    let (graph, node_map) = build_digraph(edges_out);
    let reverse_map: HashMap<NodeIndex, String> = node_map.iter().map(|(k, &v)| (v, k.clone())).collect();

    // Betweenness centrality (Brandes)
    let betweenness = compute_betweenness(&graph, &node_map);

    // PageRank (20 iterations, 0.85 damping)
    let pagerank = compute_pagerank(&graph, &node_map, 20, 0.85);

    // Strongly connected components (Tarjan) — finds circular dependencies
    let raw_sccs = tarjan_scc(&graph);
    let scc_groups: Vec<Vec<String>> = raw_sccs
        .into_iter()
        .filter(|scc: &Vec<NodeIndex>| scc.len() > 1) // only actual cycles
        .map(|scc: Vec<NodeIndex>| {
            scc.into_iter()
                .filter_map(|idx| reverse_map.get(&idx).cloned())
                .collect()
        })
        .collect();

    let circular_files: Vec<String> = scc_groups.iter().flat_map(|g: &Vec<String>| g.iter().cloned()).collect();

    // Topological sort (only meaningful if no cycles in the subgraph)
    let topological_order: Vec<String> = match toposort(&graph, None) {
        Ok(order) => {
            let order_vec: Vec<NodeIndex> = order;
            order_vec
                .into_iter()
                .filter_map(|idx| reverse_map.get(&idx).cloned())
                .collect()
        },
        Err(_) => Vec::new(), // graph has cycles — topo sort not possible
    };

    // Combined score: 0.6 × betweenness + 0.4 × pagerank
    let mut combined: Vec<(String, f64)> = node_map
        .keys()
        .map(|name: &String| {
            let b = betweenness.get(name).copied().unwrap_or(0.0);
            let p = pagerank.get(name).copied().unwrap_or(0.0);
            (name.clone(), b * 0.6 + p * 0.4)
        })
        .collect();
    combined.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_files: Vec<(String, f64)> = combined.into_iter().take(top_n).collect();

    ImportanceScores {
        betweenness,
        pagerank,
        scc_groups,
        circular_files,
        topological_order,
        top_files,
    }
}

/// Compute blast radius: which files are transitively affected if `file_path` changes.
/// Uses forward BFS through the reverse dependency graph (edges_in).
pub fn compute_blast_radius(
    edges_in: &HashMap<String, std::collections::HashSet<String>>,
    file_path: &str,
    max_depth: usize,
) -> BlastRadius {
    let mut affected: Vec<AffectedFile> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(file_path.to_string());

    let mut queue = std::collections::VecDeque::new();
    queue.push_back((file_path.to_string(), 0usize));

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(dependents) = edges_in.get(&current) {
            for dep in dependents {
                if seen.insert(dep.clone()) {
                    affected.push(AffectedFile {
                        path: dep.clone(),
                        depth: depth + 1,
                        via: current.clone(),
                    });
                    queue.push_back((dep.clone(), depth + 1));
                }
            }
        }
    }

    affected.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));

    // Build the critical path (longest chain from source)
    let critical_path = if let Some(deepest) = affected.iter().max_by_key(|a| a.depth) {
        let mut path = vec![deepest.path.clone()];
        let mut current = deepest.via.clone();
        while current != file_path {
            path.push(current.clone());
            if let Some(entry) = affected.iter().find(|a| a.path == current) {
                current = entry.via.clone();
            } else {
                break;
            }
        }
        path.push(file_path.to_string());
        path.reverse();
        path
    } else {
        vec![file_path.to_string()]
    };

    BlastRadius {
        source: file_path.to_string(),
        affected_count: affected.len(),
        affected_files: affected,
        critical_path,
        max_depth,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadius {
    pub source: String,
    pub affected_count: usize,
    pub affected_files: Vec<AffectedFile>,
    /// The longest dependency chain from the source file
    pub critical_path: Vec<String>,
    pub max_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedFile {
    pub path: String,
    pub depth: usize,
    /// The immediate file that causes this file to be affected
    pub via: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn make_edges(pairs: &[(&str, &str)]) -> HashMap<String, HashSet<String>> {
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        for (src, tgt) in pairs {
            edges.entry(src.to_string()).or_default().insert(tgt.to_string());
        }
        edges
    }

    fn make_reverse_edges(pairs: &[(&str, &str)]) -> HashMap<String, HashSet<String>> {
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        for (src, tgt) in pairs {
            // Reverse: if A depends on B, then B's edges_in contains A
            edges.entry(tgt.to_string()).or_default().insert(src.to_string());
        }
        edges
    }

    #[test]
    fn test_betweenness_centrality() {
        // Linear chain: A -> B -> C -> D
        // B and C should have higher betweenness than A or D
        let edges = make_edges(&[("A", "B"), ("B", "C"), ("C", "D")]);
        let scores = compute_importance(&edges, 10);

        let b_score = scores.betweenness.get("B").copied().unwrap_or(0.0);
        let c_score = scores.betweenness.get("C").copied().unwrap_or(0.0);
        let a_score = scores.betweenness.get("A").copied().unwrap_or(0.0);
        let d_score = scores.betweenness.get("D").copied().unwrap_or(0.0);

        // B and C are on the critical path
        assert!(b_score > a_score, "B should have higher betweenness than A");
        assert!(c_score > d_score, "C should have higher betweenness than D");
    }

    #[test]
    fn test_cycle_detection() {
        // A -> B -> C -> A (cycle)
        let edges = make_edges(&[("A", "B"), ("B", "C"), ("C", "A")]);
        let scores = compute_importance(&edges, 10);

        assert!(!scores.scc_groups.is_empty(), "Should detect cycle");
        assert_eq!(scores.circular_files.len(), 3, "All 3 files in cycle");
    }

    #[test]
    fn test_no_cycles() {
        let edges = make_edges(&[("A", "B"), ("B", "C")]);
        let scores = compute_importance(&edges, 10);

        assert!(scores.scc_groups.is_empty(), "No cycles in linear chain");
        assert!(!scores.topological_order.is_empty(), "Topo sort should succeed");
    }

    #[test]
    fn test_star_topology_pagerank() {
        // D, E, F all import B. B imports A.
        // B should have high PageRank (many importers).
        let edges = make_edges(&[("D", "B"), ("E", "B"), ("F", "B"), ("B", "A")]);
        let scores = compute_importance(&edges, 10);

        let b_rank = scores.pagerank.get("B").copied().unwrap_or(0.0);
        let d_rank = scores.pagerank.get("D").copied().unwrap_or(0.0);
        assert!(b_rank > d_rank, "B should rank higher than D (more importers)");
    }

    #[test]
    fn test_blast_radius_linear() {
        // A -> B -> C. If C changes, B and A are affected (reverse edges).
        let pairs = &[("A", "B"), ("B", "C")];
        let reverse = make_reverse_edges(pairs);
        let blast = compute_blast_radius(&reverse, "C", 5);

        assert_eq!(blast.affected_count, 2);
        assert_eq!(blast.source, "C");
        // B is depth 1 (directly depends on C), A is depth 2
        assert!(blast.affected_files.iter().any(|f| f.path == "B" && f.depth == 1));
        assert!(blast.affected_files.iter().any(|f| f.path == "A" && f.depth == 2));
    }

    #[test]
    fn test_blast_radius_max_depth() {
        let pairs = &[("A", "B"), ("B", "C"), ("C", "D")];
        let reverse = make_reverse_edges(pairs);
        let blast = compute_blast_radius(&reverse, "D", 1);

        // Only depth 1 — just C
        assert_eq!(blast.affected_count, 1);
        assert_eq!(blast.affected_files[0].path, "C");
    }

    #[test]
    fn test_top_files_ordering() {
        // Hub topology: everything goes through B
        let edges = make_edges(&[
            ("A", "B"),
            ("C", "B"),
            ("D", "B"),
            ("B", "E"),
            ("B", "F"),
        ]);
        let scores = compute_importance(&edges, 3);

        assert!(!scores.top_files.is_empty());
        // B should be the top file
        assert_eq!(scores.top_files[0].0, "B", "Hub node B should rank first");
    }

    #[test]
    fn test_empty_graph() {
        let edges: HashMap<String, HashSet<String>> = HashMap::new();
        let scores = compute_importance(&edges, 10);

        assert!(scores.betweenness.is_empty());
        assert!(scores.top_files.is_empty());
        assert!(scores.scc_groups.is_empty());
    }
}
