use crate::brain::schema::{MemoryEntry, MemoryKind};
use crate::token::engine::TokenEngine;
use std::collections::HashMap;

pub struct BrainOrchestrator {
    #[allow(dead_code)]
    context_cache: HashMap<String, Vec<String>>,
    loading_order: Vec<MemoryKind>,
}

impl BrainOrchestrator {
    pub fn new() -> Self {
        Self {
            context_cache: HashMap::new(),
            loading_order: vec![
                MemoryKind::Fact,
                MemoryKind::Pattern,
                MemoryKind::Decision,
                MemoryKind::Context,
                MemoryKind::Warning,
                MemoryKind::Negative,
            ],
        }
    }

    pub fn assemble_context(&self, entries: &[MemoryEntry], max_tokens: usize) -> Vec<MemoryEntry> {
        let mut selected = Vec::new();
        let mut current_tokens = 0;
        let mut by_kind: HashMap<MemoryKind, Vec<&MemoryEntry>> = HashMap::new();

        for entry in entries {
            by_kind.entry(entry.kind.clone()).or_default().push(entry);
        }

        for kind in &self.loading_order {
            if let Some(entries_for_kind) = by_kind.get(kind) {
                for entry in entries_for_kind {
                    let entry_tokens = self.estimate_tokens(&entry.content);
                    if current_tokens + entry_tokens <= max_tokens {
                        selected.push((*entry).clone());
                        current_tokens += entry_tokens;
                    }
                }
            }
        }

        selected
    }

    pub fn preload_for_file(&self, _file_path: &str) -> Vec<MemoryKind> {
        vec![
            MemoryKind::Pattern,
            MemoryKind::Decision,
            MemoryKind::Warning,
        ]
    }

    pub fn get_negative_memories(&self, entries: &[MemoryEntry]) -> Vec<MemoryEntry> {
        entries
            .iter()
            .filter(|e| e.kind == MemoryKind::Negative)
            .cloned()
            .collect()
    }

    pub fn get_active_decisions(&self, entries: &[MemoryEntry]) -> Vec<MemoryEntry> {
        entries
            .iter()
            .filter(|e| e.kind == MemoryKind::Decision && e.superseded_by.is_none())
            .cloned()
            .collect()
    }

    pub fn get_conflicts<'a>(
        &self,
        entry: &MemoryEntry,
        all_entries: &'a [MemoryEntry],
    ) -> Vec<&'a MemoryEntry> {
        all_entries
            .iter()
            .filter(|e| entry.contradicts.contains(&e.id))
            .collect()
    }

    fn estimate_tokens(&self, content: &str) -> usize {
        TokenEngine::count_tokens(content).unwrap_or_else(|_| content.len() / 4)
    }
}

impl Default for BrainOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}
