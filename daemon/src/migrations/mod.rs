use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;

use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};
use crate::storage::StorageBackend;

const SCHEMA_VERSION: u32 = 1;
const SCHEMA_MARKER_ID: &str = "memix_schema_version.json";

#[derive(Debug, Serialize)]
pub struct MigrationRun {
    pub id: String,
    pub applied: bool,
    pub details: String,
}

#[derive(Debug, Serialize)]
pub struct MigrationReport {
    pub project_id: String,
    pub schema_version: u32,
    pub migrated_entries: u64,
    pub runs: Vec<MigrationRun>,
}

pub async fn run_project_migrations(
    storage: Arc<dyn StorageBackend + Send + Sync>,
    project_id: &str,
) -> Result<MigrationReport> {
    let entries = storage.get_entries(project_id).await?;

    // Migration m001: backfill vector embeddings by rewriting existing entries.
    // This is intentionally idempotent.
    let mut migrated_entries: u64 = 0;
    for entry in entries {
        if entry.id == SCHEMA_MARKER_ID {
            continue;
        }
        storage.upsert_entry(project_id, entry).await?;
        migrated_entries = migrated_entries.saturating_add(1);
    }

    let now = Utc::now();
    let schema_marker = MemoryEntry {
        id: SCHEMA_MARKER_ID.to_string(),
        project_id: project_id.to_string(),
        kind: MemoryKind::Fact,
        content: serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "migrated_at": now,
            "migrated_entries": migrated_entries,
            "migration": "m001_backfill_embeddings"
        })
        .to_string(),
        tags: vec!["system".to_string(), "schema".to_string(), "migration".to_string()],
        source: MemorySource::UserManual,
        superseded_by: None,
        contradicts: vec![],
		parent_id: None,
		caused_by: vec![],
		enables: vec![],
        created_at: now,
        updated_at: now,
        access_count: 0,
        last_accessed_at: None,
    };
    storage.upsert_entry(project_id, schema_marker).await?;

    Ok(MigrationReport {
        project_id: project_id.to_string(),
        schema_version: SCHEMA_VERSION,
        migrated_entries,
        runs: vec![MigrationRun {
            id: "m001_backfill_embeddings".to_string(),
            applied: true,
            details: format!(
                "Rewrote {} entries to backfill vectors and refreshed schema marker",
                migrated_entries
            ),
        }],
    })
}
