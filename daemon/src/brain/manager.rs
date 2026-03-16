use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

pub struct BrainManager {
    #[allow(dead_code)]
    max_entries_per_project: usize,
    max_entry_size_bytes: usize,
}

impl BrainManager {
    pub fn new() -> Self {
        Self {
            max_entries_per_project: 1000,
            max_entry_size_bytes: 51200,
        }
    }

    pub fn create_entry(
        &self,
        project_id: &str,
        kind: MemoryKind,
        content: String,
        tags: Vec<String>,
        source: MemorySource,
    ) -> Result<MemoryEntry> {
        if content.len() > self.max_entry_size_bytes {
            anyhow::bail!(
                "Entry content exceeds maximum size of {} bytes",
                self.max_entry_size_bytes
            );
        }

        let now = Utc::now();
        Ok(MemoryEntry {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            kind,
            content,
            tags,
            source,
            superseded_by: None,
            contradicts: vec![],
			parent_id: None,
			caused_by: vec![],
			enables: vec![],
            created_at: now,
            updated_at: now,
            access_count: 0,
            last_accessed_at: None,
        })
    }

    pub fn update_entry(&self, entry: &mut MemoryEntry, new_content: String) -> Result<()> {
        if new_content.len() > self.max_entry_size_bytes {
            anyhow::bail!(
                "Entry content exceeds maximum size of {} bytes",
                self.max_entry_size_bytes
            );
        }

        entry.content = new_content;
        entry.updated_at = Utc::now();
        Ok(())
    }

    pub fn mark_accessed(&self, entry: &mut MemoryEntry) {
        entry.access_count = entry.access_count.saturating_add(1);
        entry.last_accessed_at = Some(Utc::now());
    }

    pub fn link_superseded(&self, entry: &mut MemoryEntry, superseded_by_id: String) {
        entry.superseded_by = Some(superseded_by_id);
        entry.updated_at = Utc::now();
    }

    pub fn add_contradiction(&self, entry: &mut MemoryEntry, contradicts_id: String) {
        if !entry.contradicts.contains(&contradicts_id) {
            entry.contradicts.push(contradicts_id);
            entry.updated_at = Utc::now();
        }
    }

    pub fn resolve_contradiction(&self, entry: &mut MemoryEntry, contradicts_id: &str) {
        entry.contradicts.retain(|id| id != contradicts_id);
        entry.updated_at = Utc::now();
    }

    pub fn get_entry_age_hours(&self, entry: &MemoryEntry) -> i64 {
        let now = Utc::now();
        let duration = now.signed_duration_since(entry.created_at);
        duration.num_hours()
    }

    pub fn is_entry_stale(&self, entry: &MemoryEntry, hours_threshold: i64) -> bool {
        self.get_entry_age_hours(entry) > hours_threshold
    }
}

impl Default for BrainManager {
    fn default() -> Self {
        Self::new()
    }
}
