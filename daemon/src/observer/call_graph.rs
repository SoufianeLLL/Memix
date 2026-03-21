use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ResolvedEdge {
    pub callee_file: String,
    pub callee_symbol: String,
    pub callee_line: u32,
    pub is_method: bool,
}

impl ResolvedEdge {
    pub fn new_unresolved(symbol: &str) -> Self {
        Self {
            callee_file: String::new(),
            callee_symbol: symbol.to_string(),
            callee_line: 0,
            is_method: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CallerSite {
    pub caller_file: String,
    pub caller_symbol: String,
    pub call_line: u32,
    pub is_method: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolCausalContext {
    pub symbol: String,
    pub calls: Vec<ResolvedEdge>,
    pub called_by: Vec<CallerSite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCausalContext {
    pub file: String,
    pub symbols: Vec<SymbolCausalContext>,
    pub total_outgoing_edges: usize,
    pub total_incoming_edges: usize,
}

type ExactTargetKey = (String, String);

/// Lightweight in-memory call graph.
/// Tracks which functions call which other functions and preserves exact
/// callee-file information when semantic resolution is available.
pub struct CallGraph {
    calls: HashMap<String, HashMap<String, Vec<ResolvedEdge>>>,
    exact_callers: HashMap<ExactTargetKey, Vec<CallerSite>>,
    symbol_callers: HashMap<String, Vec<CallerSite>>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            calls: HashMap::new(),
            exact_callers: HashMap::new(),
            symbol_callers: HashMap::new(),
        }
    }

    pub fn update_file(&mut self, file_path: &str, symbols: Vec<(String, Vec<ResolvedEdge>)>) {
        for caller_list in self.exact_callers.values_mut() {
            caller_list.retain(|caller| caller.caller_file != file_path);
        }
        for caller_list in self.symbol_callers.values_mut() {
            caller_list.retain(|caller| caller.caller_file != file_path);
        }
        self.exact_callers.retain(|_, callers| !callers.is_empty());
        self.symbol_callers.retain(|_, callers| !callers.is_empty());

        let mut file_calls: HashMap<String, Vec<ResolvedEdge>> = HashMap::new();
        for (symbol, callees) in symbols {
            for callee in &callees {
                let caller = CallerSite {
                    caller_file: file_path.to_string(),
                    caller_symbol: symbol.clone(),
                    call_line: callee.callee_line,
                    is_method: callee.is_method,
                };

                if !callee.callee_file.is_empty() {
                    self.exact_callers
                        .entry((callee.callee_file.clone(), callee.callee_symbol.clone()))
                        .or_default()
                        .push(caller.clone());
                }

                self.symbol_callers
                    .entry(callee.callee_symbol.clone())
                    .or_default()
                    .push(caller);
            }
            file_calls.insert(symbol, callees);
        }

        self.calls.insert(file_path.to_string(), file_calls);
    }

    pub fn remove_file(&mut self, file_path: &str) {
        self.calls.remove(file_path);
        for caller_list in self.exact_callers.values_mut() {
            caller_list.retain(|caller| caller.caller_file != file_path);
        }
        for caller_list in self.symbol_callers.values_mut() {
            caller_list.retain(|caller| caller.caller_file != file_path);
        }
        self.exact_callers.retain(|_, callers| !callers.is_empty());
        self.symbol_callers.retain(|_, callers| !callers.is_empty());
    }

    pub fn calls_from(&self, file_path: &str, symbol: &str) -> Vec<ResolvedEdge> {
        self.calls
            .get(file_path)
            .and_then(|entries| entries.get(symbol))
            .cloned()
            .unwrap_or_default()
    }

    pub fn callers_of(&self, file_path: &str, symbol: &str) -> Vec<String> {
        self.caller_sites(file_path, symbol)
            .into_iter()
            .map(|caller| caller.caller_symbol)
            .collect()
    }

    pub fn caller_sites(&self, file_path: &str, symbol: &str) -> Vec<CallerSite> {
        let mut callers = self
            .exact_callers
            .get(&(file_path.to_string(), symbol.to_string()))
            .cloned()
            .unwrap_or_default();

        if callers.is_empty() {
            callers = self.symbol_callers.get(symbol).cloned().unwrap_or_default();
        }

        callers.sort_by(|a, b| {
            a.caller_file
                .cmp(&b.caller_file)
                .then_with(|| a.caller_symbol.cmp(&b.caller_symbol))
                .then_with(|| a.call_line.cmp(&b.call_line))
        });
        callers.dedup();
        callers
    }

    pub fn calls_from_file(&self, file_path: &str) -> Vec<(String, Vec<ResolvedEdge>)> {
        let mut symbols = self
            .calls
            .get(file_path)
            .map(|entries| {
                entries
                    .iter()
                    .map(|(symbol, calls)| (symbol.clone(), calls.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        symbols.sort_by(|a, b| a.0.cmp(&b.0));
        symbols
    }

    pub fn causal_context_for_file(&self, file_path: &str) -> FileCausalContext {
        let mut symbols = Vec::new();
        let mut total_outgoing_edges = 0usize;
        let mut total_incoming_edges = 0usize;

        for (symbol, calls) in self.calls_from_file(file_path) {
            total_outgoing_edges += calls.len();
            let called_by = self.caller_sites(file_path, &symbol);
            total_incoming_edges += called_by.len();
            symbols.push(SymbolCausalContext {
                symbol,
                calls,
                called_by,
            });
        }

        FileCausalContext {
            file: file_path.to_string(),
            symbols,
            total_outgoing_edges,
            total_incoming_edges,
        }
    }

    pub fn edge_count(&self) -> usize {
        self.calls
            .values()
            .flat_map(|entries| entries.values())
            .map(Vec::len)
            .sum()
    }

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
        cg.update_file(
            "src/main.rs",
            vec![
                (
                    "main".to_string(),
                    vec![
                        ResolvedEdge::new_unresolved("run_server"),
                        ResolvedEdge::new_unresolved("init"),
                    ],
                ),
                (
                    "run_server".to_string(),
                    vec![ResolvedEdge::new_unresolved("bind")],
                ),
            ],
        );

        assert_eq!(
            cg.calls_from("src/main.rs", "main"),
            vec![
                ResolvedEdge::new_unresolved("run_server"),
                ResolvedEdge::new_unresolved("init"),
            ]
        );
        assert_eq!(
            cg.calls_from("src/main.rs", "run_server"),
            vec![ResolvedEdge::new_unresolved("bind")]
        );
        assert_eq!(cg.callers_of("src/main.rs", "run_server"), vec!["main".to_string()]);
        assert_eq!(cg.callers_of("src/main.rs", "init"), vec!["main".to_string()]);
        assert_eq!(cg.callers_of("src/main.rs", "bind"), vec!["run_server".to_string()]);
        assert_eq!(cg.file_count(), 1);
        assert_eq!(cg.edge_count(), 3);
    }

    #[test]
    fn test_call_graph_update_replaces_previous() {
        let mut cg = CallGraph::new();
        cg.update_file(
            "src/main.rs",
            vec![("main".to_string(), vec![ResolvedEdge::new_unresolved("old_fn")])],
        );
        assert_eq!(cg.callers_of("", "old_fn"), vec!["main".to_string()]);

        cg.update_file(
            "src/main.rs",
            vec![("main".to_string(), vec![ResolvedEdge::new_unresolved("new_fn")])],
        );
        assert_eq!(cg.callers_of("", "new_fn"), vec!["main".to_string()]);
        assert!(cg.callers_of("", "old_fn").is_empty());
    }

    #[test]
    fn test_call_graph_remove_file() {
        let mut cg = CallGraph::new();
        cg.update_file(
            "src/a.rs",
            vec![("foo".to_string(), vec![ResolvedEdge::new_unresolved("bar")])],
        );
        cg.update_file(
            "src/b.rs",
            vec![("baz".to_string(), vec![ResolvedEdge::new_unresolved("bar")])],
        );
        assert_eq!(cg.callers_of("", "bar").len(), 2);

        cg.remove_file("src/a.rs");
        assert_eq!(cg.callers_of("", "bar"), vec!["baz".to_string()]);
        assert_eq!(cg.file_count(), 1);
    }

    #[test]
    fn test_exact_callers_for_resolved_edges() {
        let mut cg = CallGraph::new();
        cg.update_file(
            "src/a.ts",
            vec![(
                "main".to_string(),
                vec![ResolvedEdge {
                    callee_file: "src/lib.ts".to_string(),
                    callee_symbol: "helper".to_string(),
                    callee_line: 12,
                    is_method: false,
                }],
            )],
        );

        let callers = cg.caller_sites("src/lib.ts", "helper");
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].caller_file, "src/a.ts");
        assert_eq!(callers[0].caller_symbol, "main");
        assert_eq!(callers[0].call_line, 12);
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
