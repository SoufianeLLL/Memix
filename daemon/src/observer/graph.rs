use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

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
}
