use std::collections::HashMap;

/// Lightweight in-memory call graph.
/// Tracks which functions call which other functions.
/// Built incrementally as files are parsed — NOT persisted to Redis.
/// Rebuilt on daemon restart from file-save events.
pub struct CallGraph {
    /// file_path -> symbol_name -> list of symbols it calls
    calls: HashMap<String, HashMap<String, Vec<String>>>,
    /// symbol_name -> list of (file_path, caller_name) pairs
    callers: HashMap<String, Vec<(String, String)>>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            calls: HashMap::new(),
            callers: HashMap::new(),
        }
    }

    /// Called when a file is parsed. Replaces any previous call data for that file.
    pub fn update_file(&mut self, file_path: &str, symbols: Vec<(String, Vec<String>)>) {
        // Remove stale caller entries for this file
        for caller_list in self.callers.values_mut() {
            caller_list.retain(|(path, _)| path != file_path);
        }

        // Build new call map for this file
        let mut file_calls: HashMap<String, Vec<String>> = HashMap::new();
        for (symbol, callees) in symbols {
            for callee in &callees {
                self.callers
                    .entry(callee.clone())
                    .or_default()
                    .push((file_path.to_string(), symbol.clone()));
            }
            file_calls.insert(symbol, callees);
        }
        self.calls.insert(file_path.to_string(), file_calls);
    }

    /// Remove all call data for a deleted file.
    pub fn remove_file(&mut self, file_path: &str) {
        self.calls.remove(file_path);
        for caller_list in self.callers.values_mut() {
            caller_list.retain(|(path, _)| path != file_path);
        }
        // Clean up empty entries
        self.callers.retain(|_, v| !v.is_empty());
    }

    /// What does this symbol call?
    pub fn calls_from(&self, file_path: &str, symbol: &str) -> Vec<String> {
        self.calls
            .get(file_path)
            .and_then(|f| f.get(symbol))
            .cloned()
            .unwrap_or_default()
    }

    /// What calls this symbol?
    pub fn callers_of(&self, _file_path: &str, symbol: &str) -> Vec<String> {
        self.callers
            .get(symbol)
            .map(|v| v.iter().map(|(_, caller)| caller.clone()).collect())
            .unwrap_or_default()
    }

    /// Total number of tracked edges
    pub fn edge_count(&self) -> usize {
        self.calls.values().flat_map(|m| m.values()).map(|v| v.len()).sum()
    }

    /// Total number of files in the graph
    pub fn file_count(&self) -> usize {
        self.calls.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_graph_update_and_query() {
        let mut cg = CallGraph::new();
        cg.update_file("src/main.rs", vec![
            ("main".to_string(), vec!["run_server".to_string(), "init".to_string()]),
            ("run_server".to_string(), vec!["bind".to_string()]),
        ]);

        assert_eq!(cg.calls_from("src/main.rs", "main"), vec!["run_server", "init"]);
        assert_eq!(cg.calls_from("src/main.rs", "run_server"), vec!["bind"]);
        assert_eq!(cg.callers_of("src/main.rs", "run_server"), vec!["main"]);
        assert_eq!(cg.callers_of("src/main.rs", "init"), vec!["main"]);
        assert_eq!(cg.callers_of("src/main.rs", "bind"), vec!["run_server"]);
        assert_eq!(cg.file_count(), 1);
        assert_eq!(cg.edge_count(), 3);
    }

    #[test]
    fn test_call_graph_update_replaces_previous() {
        let mut cg = CallGraph::new();
        cg.update_file("src/main.rs", vec![
            ("main".to_string(), vec!["old_fn".to_string()]),
        ]);
        assert_eq!(cg.callers_of("", "old_fn"), vec!["main"]);

        // Replace with new data
        cg.update_file("src/main.rs", vec![
            ("main".to_string(), vec!["new_fn".to_string()]),
        ]);
        assert_eq!(cg.callers_of("", "new_fn"), vec!["main"]);
        assert!(cg.callers_of("", "old_fn").is_empty());
    }

    #[test]
    fn test_call_graph_remove_file() {
        let mut cg = CallGraph::new();
        cg.update_file("src/a.rs", vec![
            ("foo".to_string(), vec!["bar".to_string()]),
        ]);
        cg.update_file("src/b.rs", vec![
            ("baz".to_string(), vec!["bar".to_string()]),
        ]);
        assert_eq!(cg.callers_of("", "bar").len(), 2);

        cg.remove_file("src/a.rs");
        assert_eq!(cg.callers_of("", "bar"), vec!["baz"]);
        assert_eq!(cg.file_count(), 1);
    }

    #[test]
    fn test_call_graph_empty_queries() {
        let cg = CallGraph::new();
        assert!(cg.calls_from("nonexistent", "func").is_empty());
        assert!(cg.callers_of("nonexistent", "func").is_empty());
        assert_eq!(cg.file_count(), 0);
        assert_eq!(cg.edge_count(), 0);
    }
}
