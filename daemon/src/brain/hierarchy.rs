use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::brain::schema::MemoryEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyLayer {
    pub project_id: String,
    pub entries: HashMap<String, MemoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyResolution {
    pub entry_id: String,
    pub resolved_from: Vec<String>,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct BrainHierarchy {
    pub layers: Vec<HierarchyLayer>,
}

impl BrainHierarchy {
    pub fn new(layers: Vec<HierarchyLayer>) -> Self {
        Self { layers }
    }

    pub fn resolve(&self, entry_id: &str) -> Option<HierarchyResolution> {
        for layer in &self.layers {
            if let Some(entry) = layer.entries.get(entry_id) {
                return Some(HierarchyResolution {
                    entry_id: entry_id.to_string(),
                    resolved_from: vec![layer.project_id.clone()],
                    value: parse_entry_content(entry),
                });
            }
        }
        None
    }

    pub fn resolve_merged(&self, entry_id: &str) -> Option<HierarchyResolution> {
        let mut resolved_from = Vec::new();
        let mut merged: Option<Value> = None;
        for layer in self.layers.iter().rev() {
            let Some(entry) = layer.entries.get(entry_id) else {
                continue;
            };
            let value = parse_entry_content(entry);
            merged = Some(match merged {
                Some(existing) => merge_values(existing, value),
                None => value,
            });
            resolved_from.push(layer.project_id.clone());
        }
        merged.map(|value| HierarchyResolution {
            entry_id: entry_id.to_string(),
            resolved_from,
            value,
        })
    }
}

fn parse_entry_content(entry: &MemoryEntry) -> Value {
    serde_json::from_str(&entry.content).unwrap_or_else(|_| Value::String(entry.content.clone()))
}

fn merge_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                let next = match base_map.remove(&key) {
                    Some(existing) => merge_values(existing, value),
                    None => value,
                };
                base_map.insert(key, next);
            }
            Value::Object(base_map)
        }
        (Value::Array(mut base_array), Value::Array(overlay_array)) => {
            let mut seen: HashSet<String> = HashSet::new();
            for item in &base_array {
                if let Ok(s) = serde_json::to_string(item) {
                    seen.insert(s);
                }
            }
            for item in overlay_array {
                if let Ok(s) = serde_json::to_string(&item) {
                    if seen.insert(s) {
                        base_array.push(item);
                    }
                } else if !base_array.contains(&item) {
                    base_array.push(item);
                }
            }
            Value::Array(base_array)
        }
        (_, overlay) => overlay,
    }
}
