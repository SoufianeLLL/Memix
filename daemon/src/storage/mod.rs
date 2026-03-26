use crate::config::AppConfig;
use crate::brain::schema::MemoryEntry;
use crate::sync::team::TeamBrainSnapshot;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RedisStats {
    pub used_bytes: u64,
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TeamSyncReport {
    pub project_id: String,
    pub team_id: String,
    pub recovered_from_gap: bool,
    pub recovered_entries: u64,
    pub pushed_entries: u64,
    pub pulled_entries: u64,
    pub applied_operations: u64,
    pub merged_entries: u64,
    pub conflict_entries: u64,
    pub actor_id: String,
    pub cursor: i64,
    pub team_namespace: String,
    pub team_brain: TeamBrainSnapshot,
}

pub mod redis;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn get_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>>;
    async fn get_entry(&self, project_id: &str, entry_id: &str) -> Result<MemoryEntry>;
    async fn upsert_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()>;
    async fn search_entries(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>>;
    async fn search_similar(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>>;
    async fn delete_entry(&self, project_id: &str, entry_id: &str) -> Result<()>;
    async fn purge_project(&self, project_id: &str) -> Result<()>;
    
    /// Get the current project ID from storage (first available project)
    async fn get_project_id(&self) -> Option<String> {
        None // Default implementation
    }
    
    async fn embed_text(&self, _text: &str) -> Vec<f32> {
        Vec::new() // Default empty implementation
    }

    /// Export all brain entries for a project to the daemon-managed JSON mirror directory.
    /// Returns the number of entries written.
    async fn export_project_to_json(&self, project_id: &str) -> Result<u64>;

    /// Import brain entries for a project from the daemon-managed JSON mirror directory.
    /// Returns the number of entries imported.
    async fn import_project_from_json(&self, project_id: &str) -> Result<u64>;

    /// Returns Redis memory usage stats if supported by the backend.
    async fn redis_stats(&self) -> Result<RedisStats>;

    /// List known project ids stored by the backend.
    async fn list_projects(&self) -> Result<Vec<String>>;

    /// Synchronize project entries with a shared team namespace using the secure team transport.
    async fn sync_team_project(
        &self,
        project_id: &str,
        team_id: &str,
        actor_id: &str,
        shared_secret: &str,
    ) -> Result<TeamSyncReport>;

    // ─── Skeleton Index methods (isolated storage) ───────────────────

    /// Upsert a skeleton entry (FSI or FuSI) into the isolated skeleton hash.
    async fn upsert_skeleton_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
        let _ = (project_id, entry);
        Ok(())
    }

    /// Get all skeleton entries (FSI + FuSI) for a project.
    async fn get_skeleton_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
        let _ = project_id;
        Ok(Vec::new())
    }

    /// Delete a specific skeleton entry by ID.
    async fn delete_skeleton_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
        let _ = (project_id, entry_id);
        Ok(())
    }

    /// Purge all skeleton entries for a project (clear the entire index).
    async fn purge_skeleton_entries(&self, project_id: &str) -> Result<usize> {
        let _ = project_id;
        Ok(0)
    }

    /// Returns (fsi_count, fusi_count, total, size_bytes) for the skeleton index.
    async fn skeleton_stats(&self, project_id: &str) -> Result<(usize, usize, usize, usize)> {
        let _ = project_id;
        Ok((0, 0, 0, 0))
    }
}

/// Factory function deciding which backend to boot based on config.
pub async fn initialize_storage(
    config: &AppConfig,
) -> Result<Arc<dyn StorageBackend + Send + Sync>> {
    Ok(Arc::new(redis::RedisStorage::new(config).await?))
}
