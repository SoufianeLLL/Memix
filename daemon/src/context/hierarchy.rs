//! Hierarchy resolution for layered context inheritance.
//!
//! Supports monorepo-style parent/child context loading where
//! local layers can override or merge inherited layers.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Context layer in hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextLayer {
    /// Layer name
    pub name: String,
    /// Layer path
    pub path: String,
    /// Layer priority (higher = more specific/important)
    pub priority: u32,
    /// Parent layer reference
    pub parent: Option<String>,
    /// Layer content
    pub content: LayerContent,
    /// Override mode
    pub override_mode: OverrideMode,
}

/// Content of a layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerContent {
    /// Patterns
    pub patterns: Vec<String>,
    /// Decisions
    pub decisions: Vec<String>,
    /// Rules
    pub rules: Vec<String>,
    /// Known issues
    pub known_issues: Vec<String>,
    /// File map entries
    pub file_map: HashMap<String, String>,
}

/// How this layer interacts with parent layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverrideMode {
    /// Merge with parent (additive)
    Merge,
    /// Override parent (replace)
    Override,
    /// Extend parent (prepend)
    Extend,
}

/// Resolved context from hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedContext {
    /// All patterns (merged from hierarchy)
    pub patterns: Vec<String>,
    /// All decisions (merged from hierarchy)
    pub decisions: Vec<String>,
    /// All rules (merged from hierarchy)
    pub rules: Vec<String>,
    /// All known issues (merged from hierarchy)
    pub known_issues: Vec<String>,
    /// Merged file map
    pub file_map: HashMap<String, String>,
    /// Layers that contributed
    pub contributing_layers: Vec<String>,
    /// Resolution metadata
    pub metadata: ResolutionMetadata,
}

/// Resolution metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionMetadata {
    /// Number of layers resolved
    pub layers_resolved: usize,
    /// Number of conflicts resolved
    pub conflicts_resolved: usize,
    /// Resolution time (ms)
    pub resolution_time_ms: u64,
}

/// Hierarchy resolver
pub struct HierarchyResolver {
    /// Registered layers
    layers: HashMap<String, ContextLayer>,
    /// Layer order (sorted by priority)
    layer_order: Vec<String>,
}

impl HierarchyResolver {
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
            layer_order: Vec::new(),
        }
    }
    
    /// Register a layer
    pub fn register_layer(&mut self, layer: ContextLayer) {
        let name = layer.name.clone();
        self.layers.insert(name.clone(), layer);
        self.rebuild_order();
    }
    
    /// Remove a layer
    pub fn remove_layer(&mut self, name: &str) {
        self.layers.remove(name);
        self.rebuild_order();
    }
    
    /// Rebuild layer order by priority
    fn rebuild_order(&mut self) {
        let mut layers: Vec<_> = self.layers.iter().collect();
        layers.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
        self.layer_order = layers.into_iter().map(|(k, _)| k.clone()).collect();
    }
    
    /// Resolve context for a path
    pub fn resolve(&self, path: &str) -> Result<ResolvedContext> {
        let start = std::time::Instant::now();
        
        let mut patterns = Vec::new();
        let mut decisions = Vec::new();
        let mut rules = Vec::new();
        let mut known_issues = Vec::new();
        let mut file_map = HashMap::new();
        let mut contributing_layers = Vec::new();
        let mut conflicts_resolved = 0;
        
        // Find applicable layers (sorted by priority, highest first)
        let applicable: Vec<_> = self.layer_order.iter()
            .filter(|name| {
                self.layers.get(*name)
                    .map(|l| path.starts_with(&l.path) || l.path.is_empty())
                    .unwrap_or(false)
            })
            .collect();
        
        // Track what's been set for conflict detection
        let mut seen_patterns: HashSet<String> = HashSet::new();
        let mut seen_decisions: HashSet<String> = HashSet::new();
        
        for layer_name in applicable {
            let layer = match self.layers.get(layer_name) {
                Some(l) => l,
                None => continue,
            };
            
            contributing_layers.push(layer_name.clone());
            
            // Apply based on override mode
            match layer.override_mode {
                OverrideMode::Merge => {
                    // Additive merge
                    for p in &layer.content.patterns {
                        if seen_patterns.insert(p.clone()) {
                            patterns.push(p.clone());
                        } else {
                            conflicts_resolved += 1;
                        }
                    }
                    for d in &layer.content.decisions {
                        if seen_decisions.insert(d.clone()) {
                            decisions.push(d.clone());
                        } else {
                            conflicts_resolved += 1;
                        }
                    }
                    rules.extend(layer.content.rules.clone());
                    known_issues.extend(layer.content.known_issues.clone());
                    file_map.extend(layer.content.file_map.clone());
                }
                OverrideMode::Override => {
                    // Replace all
                    patterns = layer.content.patterns.clone();
                    decisions = layer.content.decisions.clone();
                    rules = layer.content.rules.clone();
                    known_issues = layer.content.known_issues.clone();
                    file_map = layer.content.file_map.clone();
                    
                    seen_patterns.clear();
                    seen_decisions.clear();
                    for p in &patterns {
                        seen_patterns.insert(p.clone());
                    }
                    for d in &decisions {
                        seen_decisions.insert(d.clone());
                    }
                }
                OverrideMode::Extend => {
                    // Prepend (higher priority first)
                    let mut new_patterns = layer.content.patterns.clone();
                    new_patterns.extend(patterns);
                    patterns = new_patterns;
                    
                    let mut new_decisions = layer.content.decisions.clone();
                    new_decisions.extend(decisions);
                    decisions = new_decisions;
                    
                    rules.splice(0..0, layer.content.rules.clone());
                    known_issues.splice(0..0, layer.content.known_issues.clone());
                    file_map.extend(layer.content.file_map.clone());
                }
            }
        }
        
        let resolution_time_ms = start.elapsed().as_millis() as u64;
        
        Ok(ResolvedContext {
            patterns,
            decisions,
            rules,
            known_issues,
            file_map,
            contributing_layers: contributing_layers.clone(),
            metadata: ResolutionMetadata {
                layers_resolved: contributing_layers.len(),
                conflicts_resolved,
                resolution_time_ms,
            },
        })
    }
    
    /// Get layer by name
    pub fn get_layer(&self, name: &str) -> Option<&ContextLayer> {
        self.layers.get(name)
    }
    
    /// Get all layer names
    pub fn get_layer_names(&self) -> Vec<&String> {
        self.layer_order.iter().map(|s| self.layers.get_key_value(s).map(|(k, _)| k)).flatten().collect()
    }
    
    /// Clear all layers
    pub fn clear(&mut self) {
        self.layers.clear();
        self.layer_order.clear();
    }
}

impl Default for HierarchyResolver {
    fn default() -> Self {
        Self::new()
    }
}
