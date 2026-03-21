use std::collections::{HashMap, HashSet};
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};

use crate::observer::importance::{compute_importance, ImportanceScores};

/// A live dependency adjacency matrix mapping exactly how files rely on each other.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// file path -> list of file paths that this file imports / relies on
    pub edges_out: HashMap<String, HashSet<String>>,
    
    /// file path -> list of file paths that import / rely on THIS file.
    pub edges_in: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            edges_out: HashMap::new(),
            edges_in: HashMap::new(),
        }
    }

    pub fn set_dependencies(&mut self, source: &str, targets: &[String]) {
        if let Some(old_targets) = self.edges_out.remove(source) {
            for old_target in old_targets {
                if let Some(sources) = self.edges_in.get_mut(&old_target) {
                    sources.remove(source);
                    if sources.is_empty() {
                        self.edges_in.remove(&old_target);
                    }
                }
            }
        }

        for target in targets {
            self.add_dependency(source, target);
        }
    }

    /// Add a directed dependency edge indicating `source` relies on `target`
    pub fn add_dependency(&mut self, source: &str, target: &str) {
        self.edges_out
            .entry(source.to_string())
            .or_default()
            .insert(target.to_string());
            
        self.edges_in
            .entry(target.to_string())
            .or_default()
            .insert(source.to_string());
    }

    /// Delete all dependency edges associated with a file (e.g. when it is removed)
    pub fn remove_file(&mut self, file: &str) {
        // Remove outgoing edges
        if let Some(targets) = self.edges_out.remove(file) {
            for target in targets {
                if let Some(sources) = self.edges_in.get_mut(&target) {
                    sources.remove(file);
                }
            }
        }

        // Remove incoming edges
        if let Some(sources) = self.edges_in.remove(file) {
            for source in sources {
                if let Some(targets) = self.edges_out.get_mut(&source) {
                    targets.remove(file);
                }
            }
        }
    }

    /// Convert the storage-optimized adjacency maps into a petgraph graph on demand.
    pub fn to_petgraph(&self) -> (DiGraph<String, ()>, HashMap<String, NodeIndex>) {
        let mut graph = DiGraph::<String, ()>::new();
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        let ensure_node =
            |path: &str,
             graph: &mut DiGraph<String, ()>,
             node_map: &mut HashMap<String, NodeIndex>|
             -> NodeIndex {
                if let Some(idx) = node_map.get(path) {
                    *idx
                } else {
                    let idx = graph.add_node(path.to_string());
                    node_map.insert(path.to_string(), idx);
                    idx
                }
            };

        for (source, targets) in &self.edges_out {
            let src_idx = ensure_node(source, &mut graph, &mut node_map);
            for target in targets {
                let tgt_idx = ensure_node(target, &mut graph, &mut node_map);
                graph.add_edge(src_idx, tgt_idx, ());
            }
        }

        for file in self.edges_in.keys() {
            let _ = ensure_node(file, &mut graph, &mut node_map);
        }

        (graph, node_map)
    }

    /// Structural importance scores for the current graph.
    pub fn importance_scores(&self, top_n: usize) -> ImportanceScores {
        compute_importance(&self.edges_out, top_n)
    }
}
