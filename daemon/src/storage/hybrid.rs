// Hybrid storage: SQLite for speed + Redis for optional cross-machine sync.
// SQLite is always used as the primary fast local store.
// Redis is used only when configured for team sync / cross-machine sharing.

use std::path::PathBuf;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::brain::schema::MemoryEntry;
use crate::config::AppConfig;
use super::{StorageBackend, RedisStats, TeamSyncReport};
use super::sqlite::SqliteStorage;
use super::redis::RedisStorage;

/// Hybrid storage that uses SQLite for fast local operations
/// and optionally syncs to Redis for cross-machine/team features.
pub struct HybridStorage {
    /// Primary: fast local SQLite storage
    sqlite: SqliteStorage,
    /// Optional: Redis for cross-machine sync (None if not configured)
    redis: Option<Arc<RedisStorage>>,
    /// Whether Redis sync is enabled
    redis_enabled: RwLock<bool>,
}

impl HybridStorage {
    /// Create hybrid storage. Redis is optional based on config.
    pub async fn new(config: &AppConfig) -> Result<Self> {
        // Use workspace_root if available, otherwise use a fallback that won't be created
        // IMPORTANT: The fallback should NOT create a 'default' subdirectory
        let data_dir = config.workspace_root.clone()
            .map(|ws| PathBuf::from(&ws).join(".memix"))
            .unwrap_or_else(|| {
                // Fallback to user home .memix (not .memix/default)
                dirs::home_dir()
                    .map(|h| h.join(".memix"))
                    .unwrap_or_else(|| PathBuf::from(".memix"))
            });
        
        // SQLite is always created (but doesn't create data_dir until needed)
        let sqlite = SqliteStorage::new(data_dir).await?;
        
        // Redis is optional - only if URL is configured
        let redis = match RedisStorage::new(config).await {
            Ok(r) => {
                tracing::info!("HybridStorage: Redis backend available for sync");
                Some(Arc::new(r))
            }
            Err(e) => {
                tracing::info!("HybridStorage: Redis not configured or unavailable, running in local-only mode: {}", e);
                None
            }
        };
        
        Ok(Self {
            sqlite,
            redis,
            redis_enabled: RwLock::new(true),
        })
    }
    
    /// Get reference to SQLite storage (for workspace root management)
    pub fn as_sqlite(&self) -> &SqliteStorage {
        &self.sqlite
    }
    
    /// Get brain database file size for a project
    pub async fn brain_db_size(&self, project_id: &str) -> u64 {
        self.sqlite.get_brain_db_size(project_id).await
    }
    
    /// Returns self as Any for downcasting
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    /// Enable or disable Redis sync (can be toggled at runtime)
    pub async fn set_redis_enabled(&self, enabled: bool) {
        let mut flag = self.redis_enabled.write().await;
        *flag = enabled;
    }
    
    /// Check if Redis is available and enabled
    async fn should_sync(&self) -> bool {
        let enabled = *self.redis_enabled.read().await;
        enabled && self.redis.is_some()
    }
    
    /// Sync a single entry to Redis (fire-and-forget)
    async fn sync_to_redis(&self, project_id: &str, entry: &MemoryEntry) {
        if !self.should_sync().await {
            return;
        }
        
        if let Some(ref redis) = self.redis {
            let entry = entry.clone();
            let project_id = project_id.to_string();
            let redis = Arc::clone(redis);
            tokio::spawn(async move {
                let _ = redis.upsert_entry(&project_id, entry).await;
            });
        }
    }
    
    /// Sync a delete to Redis (fire-and-forget)
    async fn sync_delete_to_redis(&self, project_id: &str, entry_id: &str) {
        if !self.should_sync().await {
            return;
        }
        
        if let Some(ref redis) = self.redis {
            let project_id = project_id.to_string();
            let entry_id = entry_id.to_string();
            let redis = Arc::clone(redis);
            tokio::spawn(async move {
                let _ = redis.delete_entry(&project_id, &entry_id).await;
            });
        }
    }
    
    /// Pull latest from Redis to SQLite (for startup or manual sync)
    pub async fn pull_from_redis(&self, project_id: &str) -> Result<u64> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(0),
        };
        
        let redis_entries = redis.get_entries(project_id).await?;
        let mut pulled = 0u64;
        
        for entry in redis_entries {
            self.sqlite.upsert_entry(project_id, entry).await?;
            pulled += 1;
        }
        
        tracing::info!("HybridStorage: Pulled {} entries from Redis for {}", pulled, project_id);
        Ok(pulled)
    }
    
    /// Push all local entries to Redis (for initial sync)
    pub async fn push_to_redis(&self, project_id: &str) -> Result<u64> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(0),
        };
        
        let local_entries = self.sqlite.get_entries(project_id).await?;
        let mut pushed = 0u64;
        
        for entry in local_entries {
            redis.upsert_entry(project_id, entry).await?;
            pushed += 1;
        }
        
        tracing::info!("HybridStorage: Pushed {} entries to Redis for {}", pushed, project_id);
        Ok(pushed)
    }
}

#[async_trait]
impl StorageBackend for HybridStorage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    async fn get_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
        // Always read from SQLite (fast local)
        self.sqlite.get_entries(project_id).await
    }
    
    async fn get_entry(&self, project_id: &str, entry_id: &str) -> Result<MemoryEntry> {
        self.sqlite.get_entry(project_id, entry_id).await
    }
    
    async fn upsert_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
        // Write to SQLite first (fast)
        self.sqlite.upsert_entry(project_id, entry.clone()).await?;
        
        // Sync to Redis in background (non-blocking)
        self.sync_to_redis(project_id, &entry).await;
        
        Ok(())
    }
    
    async fn search_entries(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>> {
        self.sqlite.search_entries(project_id, query).await
    }
    
    async fn search_similar(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>> {
        // Use Redis for embedding search if available, fallback to SQLite text search
        if self.should_sync().await {
            if let Some(ref redis) = self.redis {
                return redis.search_similar(project_id, query).await;
            }
        }
        self.sqlite.search_similar(project_id, query).await
    }
    
    async fn delete_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
        // Delete from SQLite
        self.sqlite.delete_entry(project_id, entry_id).await?;
        
        // Sync delete to Redis
        self.sync_delete_to_redis(project_id, entry_id).await;
        
        Ok(())
    }
    
    async fn purge_project(&self, project_id: &str) -> Result<()> {
        // Purge from SQLite
        self.sqlite.purge_project(project_id).await?;
        
        // Purge from Redis if available
        if let Some(ref redis) = self.redis {
            let _ = redis.purge_project(project_id).await;
        }
        
        Ok(())
    }
    
    async fn export_project_to_json(&self, project_id: &str) -> Result<u64> {
        self.sqlite.export_project_to_json(project_id).await
    }
    
    async fn import_project_from_json(&self, project_id: &str) -> Result<u64> {
        let imported = self.sqlite.import_project_from_json(project_id).await?;
        
        // Sync imported entries to Redis
        if imported > 0 && self.should_sync().await {
            let _ = self.push_to_redis(project_id).await;
        }
        
        Ok(imported)
    }
    
    async fn redis_stats(&self) -> Result<RedisStats> {
        if let Some(ref redis) = self.redis {
            redis.redis_stats().await
        } else {
            Ok(RedisStats {
                used_bytes: 0,
                max_bytes: None,
            })
        }
    }
    
    async fn list_projects(&self) -> Result<Vec<String>> {
        // List from SQLite (local projects)
        self.sqlite.list_projects().await
    }
    
    async fn sync_team_project(
        &self,
        project_id: &str,
        team_id: &str,
        actor_id: &str,
        shared_secret: &str,
    ) -> Result<TeamSyncReport> {
        // Team sync requires Redis
        if let Some(ref redis) = self.redis {
            let report = redis.sync_team_project(project_id, team_id, actor_id, shared_secret).await?;
            
            // Sync pulled entries to SQLite
            if report.pulled_entries > 0 {
                let _ = self.pull_from_redis(project_id).await;
            }
            
            Ok(report)
        } else {
            anyhow::bail!("Team sync requires Redis backend to be configured");
        }
    }
    
    // ─── Skeleton Index methods ──────────────────────────────────────
    
    async fn upsert_skeleton_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
        // Skeletons are local-only (not synced to Redis)
        self.sqlite.upsert_skeleton_entry(project_id, entry).await
    }
    
    async fn get_skeleton_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
        self.sqlite.get_skeleton_entries(project_id).await
    }
    
    async fn delete_skeleton_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
        self.sqlite.delete_skeleton_entry(project_id, entry_id).await
    }
    
    async fn purge_skeleton_entries(&self, project_id: &str) -> Result<usize> {
        self.sqlite.purge_skeleton_entries(project_id).await
    }
    
    async fn skeleton_stats(&self, project_id: &str) -> Result<(usize, usize, usize, usize)> {
        self.sqlite.skeleton_stats(project_id).await
    }
    
    async fn embed_text(&self, text: &str) -> Vec<f32> {
        // Use Redis for embeddings if available (has embedding cache)
        if self.should_sync().await {
            if let Some(ref redis) = self.redis {
                return redis.embed_text(text).await;
            }
        }
        self.sqlite.embed_text(text).await
    }
}
