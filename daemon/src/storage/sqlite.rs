// SQLite-based local storage backend using sqlx for async support.
// Fast, offline, no setup required. Uses WAL mode for concurrent reads.
// Per-project database stored at {workspace_root}/.memix/brain.db

use std::path::PathBuf;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use anyhow::Result;

use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};
use super::{StorageBackend, RedisStats, TeamSyncReport};

/// SQLite storage backend - fast local storage with WAL mode.
/// Each project has its own database at {workspace_root}/.memix/brain.db
pub struct SqliteStorage {
    /// Connection pools keyed by project_id
    pools: RwLock<std::collections::HashMap<String, SqlitePool>>,
    /// Workspace roots keyed by project_id (for per-project database location)
    workspace_roots: RwLock<std::collections::HashMap<String, PathBuf>>,
    /// Fallback data directory for projects without workspace_root
    default_data_dir: PathBuf,
}

impl SqliteStorage {
    /// Create SQLite storage manager
    /// Note: default_data_dir is NOT created here - only when actually needed
    pub async fn new(default_data_dir: PathBuf) -> Result<Self> {
        // Don't create default_data_dir here - it may never be used
        // Directories are created on-demand in get_pool()
        
        Ok(Self {
            pools: RwLock::new(std::collections::HashMap::new()),
            workspace_roots: RwLock::new(std::collections::HashMap::new()),
            default_data_dir,
        })
    }
    
    /// Set workspace root for a project (called when workspace is registered)
    pub async fn set_workspace_root(&self, project_id: &str, workspace_root: PathBuf) {
        let mut roots = self.workspace_roots.write().await;
        roots.insert(project_id.to_string(), workspace_root);
        tracing::info!("SqliteStorage: set workspace root for {} -> {:?}", project_id, roots.get(project_id));
    }
    
    /// Get database path for a project: {workspace_root}/.memix/brain.db
    async fn get_db_path(&self, project_id: &str) -> PathBuf {
        let roots = self.workspace_roots.read().await;
        if let Some(root) = roots.get(project_id) {
            root.join(".memix").join("brain.db")
        } else {
            // Fallback to global location if no workspace root set
            self.default_data_dir.join(project_id).join("brain.db")
        }
    }
    
    /// Get the size of the brain database file for a project
    pub async fn get_brain_db_size(&self, project_id: &str) -> u64 {
        let db_path = self.get_db_path(project_id).await;
        match std::fs::metadata(&db_path) {
            Ok(metadata) => metadata.len(),
            Err(_) => 0,
        }
    }
    
    /// Get or create a connection pool for a project
    async fn get_pool(&self, project_id: &str) -> Result<SqlitePool> {
        // Check cache first
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(project_id) {
                return Ok(pool.clone());
            }
        }
        
        // Create new pool - database stored at {workspace_root}/.memix/brain.db
        let db_path = self.get_db_path(project_id).await;
        let db_dir = db_path.parent().unwrap_or(&db_path);
        std::fs::create_dir_all(db_dir)?;
        
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
                    sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await?;
                    Ok(())
                })
            })
            .connect(&db_url)
            .await?;
        
        // Create schema
        Self::create_schema(&pool).await?;
        
        // Cache it
        {
            let mut pools = self.pools.write().await;
            pools.insert(project_id.to_string(), pool.clone());
        }
        
        Ok(pool)
    }
    
    async fn create_schema(pool: &SqlitePool) -> Result<()> {
        sqlx::query(r#"
            -- Brain entries — equivalent to the Redis HSET project_id entry_id json
            CREATE TABLE IF NOT EXISTS brain_entries (
                id          TEXT NOT NULL,
                project_id  TEXT NOT NULL,
                kind        TEXT NOT NULL,
                content     TEXT NOT NULL,
                tags        TEXT NOT NULL DEFAULT '[]',
                source      TEXT NOT NULL,
                superseded_by TEXT,
                contradicts TEXT NOT NULL DEFAULT '[]',
                parent_id   TEXT,
                caused_by   TEXT NOT NULL DEFAULT '[]',
                enables     TEXT NOT NULL DEFAULT '[]',
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TEXT,
                PRIMARY KEY (project_id, id)
            );
            
            CREATE INDEX IF NOT EXISTS idx_brain_kind ON brain_entries (project_id, kind);
            CREATE INDEX IF NOT EXISTS idx_brain_updated ON brain_entries (project_id, updated_at DESC);
            
            -- Skeleton index — equivalent to {project_id}_skeletons hash
            CREATE TABLE IF NOT EXISTS skeleton_entries (
                id          TEXT NOT NULL,
                project_id  TEXT NOT NULL,
                kind        TEXT NOT NULL,
                content     TEXT NOT NULL,
                path        TEXT,
                tags        TEXT NOT NULL DEFAULT '[]',
                updated_at  TEXT NOT NULL,
                PRIMARY KEY (project_id, id)
            );
            
            CREATE INDEX IF NOT EXISTS idx_skeleton_path ON skeleton_entries (project_id, path);
            CREATE INDEX IF NOT EXISTS idx_skeleton_kind ON skeleton_entries (project_id, kind);
            
            -- Embedding metadata — tracks which files have embeddings
            CREATE TABLE IF NOT EXISTS embedding_metadata (
                id          TEXT NOT NULL,
                project_id  TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                PRIMARY KEY (project_id, id)
            );
            
            -- Embedding vectors for semantic similarity search.
            -- Each row corresponds to a skeleton entry (FSI or FuSI).
            -- The vector BLOB is 384 × 4 = 1536 bytes of raw f32 little-endian floats.
            CREATE TABLE IF NOT EXISTS embedding_vectors (
                id              TEXT NOT NULL,
                project_id      TEXT NOT NULL,
                vector          BLOB NOT NULL,
                content_hash    INTEGER NOT NULL,
                computed_at     TEXT NOT NULL,
                PRIMARY KEY (project_id, id)
            );
            
            CREATE INDEX IF NOT EXISTS idx_embedding_project ON embedding_vectors (project_id, id);
        "#)
        .execute(pool)
        .await?;
        
        Ok(())
    }
    
    fn parse_kind(kind_str: &str) -> MemoryKind {
        match kind_str {
            "Fact" => MemoryKind::Fact,
            "Decision" => MemoryKind::Decision,
            "Warning" => MemoryKind::Warning,
            "Pattern" => MemoryKind::Pattern,
            "Context" => MemoryKind::Context,
            "Negative" => MemoryKind::Negative,
            _ => MemoryKind::Fact,
        }
    }
    
    fn parse_source(source_str: &str) -> MemorySource {
        match source_str {
            "UserManual" => MemorySource::UserManual,
            "AgentExtracted" => MemorySource::AgentExtracted,
            "FileWatcher" => MemorySource::FileWatcher,
            "GitArchaeology" => MemorySource::GitArchaeology,
            _ => MemorySource::AgentExtracted,
        }
    }
    
    fn row_to_entry(row: sqlx::sqlite::SqliteRow) -> MemoryEntry {
        let kind_str: String = row.get("kind");
        let source_str: String = row.get("source");
        let tags_json: String = row.get("tags");
        let contradicts_json: String = row.get("contradicts");
        let caused_by_json: String = row.get("caused_by");
        let enables_json: String = row.get("enables");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");
        let last_accessed_at_str: Option<String> = row.get("last_accessed_at");
        
        MemoryEntry {
            id: row.get("id"),
            project_id: row.get("project_id"),
            kind: Self::parse_kind(&kind_str),
            content: row.get("content"),
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            source: Self::parse_source(&source_str),
            superseded_by: row.get("superseded_by"),
            contradicts: serde_json::from_str(&contradicts_json).unwrap_or_default(),
            parent_id: row.get("parent_id"),
            caused_by: serde_json::from_str(&caused_by_json).unwrap_or_default(),
            enables: serde_json::from_str(&enables_json).unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            access_count: row.get("access_count"),
            last_accessed_at: last_accessed_at_str
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
        }
    }
    
    fn serialize_datetime(dt: &chrono::DateTime<chrono::Utc>) -> String {
        dt.to_rfc3339()
    }
}

#[async_trait]
impl StorageBackend for SqliteStorage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    async fn get_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
        let pool = self.get_pool(project_id).await?;
        let rows = sqlx::query(
            "SELECT * FROM brain_entries ORDER BY updated_at DESC"
        )
        .fetch_all(&pool)
        .await?;
        
        let entries = rows.into_iter().map(Self::row_to_entry).collect();
        Ok(entries)
    }
    
    async fn get_entry(&self, project_id: &str, entry_id: &str) -> Result<MemoryEntry> {
        let pool = self.get_pool(project_id).await?;
        let row = sqlx::query(
            "SELECT * FROM brain_entries WHERE id = ?"
        )
        .bind(entry_id)
        .fetch_one(&pool)
        .await?;
        
        Ok(Self::row_to_entry(row))
    }
    
    async fn upsert_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
        let pool = self.get_pool(project_id).await?;
        let kind_str = format!("{:?}", entry.kind);
        let source_str = format!("{:?}", entry.source);
        let tags_json = serde_json::to_string(&entry.tags)?;
        let contradicts_json = serde_json::to_string(&entry.contradicts)?;
        let caused_by_json = serde_json::to_string(&entry.caused_by)?;
        let enables_json = serde_json::to_string(&entry.enables)?;
        let created_at_str = Self::serialize_datetime(&entry.created_at);
        let updated_at_str = Self::serialize_datetime(&entry.updated_at);
        let last_accessed_at_str = entry.last_accessed_at.as_ref().map(Self::serialize_datetime);
        
        sqlx::query(r#"
            INSERT OR REPLACE INTO brain_entries 
            (id, project_id, kind, content, tags, source, superseded_by, contradicts, parent_id, caused_by, enables, created_at, updated_at, access_count, last_accessed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#)
        .bind(&entry.id)
        .bind(&entry.project_id)
        .bind(&kind_str)
        .bind(&entry.content)
        .bind(&tags_json)
        .bind(&source_str)
        .bind(&entry.superseded_by)
        .bind(&contradicts_json)
        .bind(&entry.parent_id)
        .bind(&caused_by_json)
        .bind(&enables_json)
        .bind(&created_at_str)
        .bind(&updated_at_str)
        .bind(entry.access_count)
        .bind(&last_accessed_at_str)
        .execute(&pool)
        .await?;
        
        Ok(())
    }
    
    async fn search_entries(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>> {
        let pool = self.get_pool(project_id).await?;
        let pattern = format!("%{}%", query.to_lowercase());
        
        let rows = sqlx::query(
            "SELECT * FROM brain_entries WHERE LOWER(content) LIKE ? OR LOWER(id) LIKE ? ORDER BY updated_at DESC LIMIT 50"
        )
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&pool)
        .await?;
        
        let entries = rows.into_iter().map(Self::row_to_entry).collect();
        Ok(entries)
    }
    
    async fn search_similar(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>> {
        // For SQLite, fall back to text search (embedding similarity requires vector extension)
        self.search_entries(project_id, query).await
    }
    
    async fn delete_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
        let pool = self.get_pool(project_id).await?;
        sqlx::query("DELETE FROM brain_entries WHERE id = ?")
            .bind(entry_id)
            .execute(&pool)
            .await?;
        
        Ok(())
    }
    
    async fn purge_project(&self, project_id: &str) -> Result<()> {
        let pool = self.get_pool(project_id).await?;
        sqlx::query("DELETE FROM brain_entries")
            .execute(&pool)
            .await?;
        
        sqlx::query("DELETE FROM skeleton_entries")
            .execute(&pool)
            .await?;
        
        sqlx::query("DELETE FROM embedding_metadata")
            .execute(&pool)
            .await?;
        
        sqlx::query("DELETE FROM embedding_vectors")
            .execute(&pool)
            .await?;
        
        // VACUUM to shrink the database file and reclaim disk space
        sqlx::query("VACUUM")
            .execute(&pool)
            .await?;
        
        // Also clear the JSON mirror directory
        let mirror_dir = self.get_db_path(project_id).await.parent()
            .map(|p| p.join("brain"))
            .unwrap_or_else(|| self.default_data_dir.join(project_id).join("brain"));
        if mirror_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&mirror_dir) {
                tracing::warn!("Failed to clear mirror directory {:?}: {}", mirror_dir, e);
            }
        }
        
        Ok(())
    }
    
    async fn export_project_to_json(&self, project_id: &str) -> Result<u64> {
        let entries = self.get_entries(project_id).await?;
        // Mirror stored at {workspace_root}/.memix/brain/ (same location purge_project clears)
        let mirror_dir = self.get_db_path(project_id).await.parent()
            .map(|p| p.join("brain"))
            .unwrap_or_else(|| self.default_data_dir.join(project_id).join("brain"));
        std::fs::create_dir_all(&mirror_dir)?;
        
        let mut written: u64 = 0;
        for entry in entries {
            let file_path = mirror_dir.join(format!("{}.json", entry.id));
            let json = serde_json::to_string_pretty(&entry)?;
            std::fs::write(&file_path, json)?;
            written += 1;
        }
        
        Ok(written)
    }
    
    async fn import_project_from_json(&self, project_id: &str) -> Result<u64> {
        // Mirror stored at {workspace_root}/.memix/brain/ (same location purge_project clears)
        let mirror_dir = self.get_db_path(project_id).await.parent()
            .map(|p| p.join("brain"))
            .unwrap_or_else(|| self.default_data_dir.join(project_id).join("brain"));
        if !mirror_dir.exists() {
            return Ok(0);
        }
        
        let mut imported: u64 = 0;
        for entry in std::fs::read_dir(&mirror_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(json) = std::fs::read_to_string(&path) {
                    if let Ok(memory_entry) = serde_json::from_str::<MemoryEntry>(&json) {
                        self.upsert_entry(project_id, memory_entry).await?;
                        imported += 1;
                    }
                }
            }
        }
        
        Ok(imported)
    }
    
    async fn redis_stats(&self) -> Result<RedisStats> {
        // SQLite doesn't have Redis stats
        Ok(RedisStats {
            used_bytes: 0,
            max_bytes: None,
        })
    }
    
    async fn list_projects(&self) -> Result<Vec<String>> {
        // List projects by scanning data directories
        let mut projects = Vec::new();
        if self.default_data_dir.exists() {
            for entry in std::fs::read_dir(&self.default_data_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() && path.join("brain.db").exists() {
                    if let Some(name) = path.file_name() {
                        projects.push(name.to_string_lossy().to_string());
                    }
                }
            }
        }
        Ok(projects)
    }
    
    async fn sync_team_project(
        &self,
        _project_id: &str,
        _team_id: &str,
        _actor_id: &str,
        _shared_secret: &str,
    ) -> Result<TeamSyncReport> {
        // SQLite doesn't support team sync - handled by HybridStorage
        anyhow::bail!("Team sync requires Redis backend");
    }
    
    // ─── Skeleton Index methods ──────────────────────────────────────
    
    async fn upsert_skeleton_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
        let pool = self.get_pool(project_id).await?;
        let kind_str = format!("{:?}", entry.kind);
        let tags_json = serde_json::to_string(&entry.tags)?;
        let updated_at_str = Self::serialize_datetime(&entry.updated_at);
        
        // Extract path from content if it's a file skeleton
        let path: Option<String> = entry.content.lines()
            .find(|l| l.starts_with("path:"))
            .map(|l| l.strip_prefix("path:").unwrap_or("").trim().to_string());
        
        sqlx::query(r#"
            INSERT OR REPLACE INTO skeleton_entries (id, project_id, kind, content, path, tags, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        "#)
        .bind(&entry.id)
        .bind(project_id)
        .bind(&kind_str)
        .bind(&entry.content)
        .bind(&path)
        .bind(&tags_json)
        .bind(&updated_at_str)
        .execute(&pool)
        .await?;
        
        Ok(())
    }
    
    async fn get_skeleton_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
        let pool = self.get_pool(project_id).await?;
        let rows = sqlx::query(
            "SELECT id, project_id, kind, content, tags, NULL as source, NULL as superseded_by, '[]' as contradicts, NULL as parent_id, '[]' as caused_by, '[]' as enables, updated_at, updated_at as created_at, 0 as access_count, NULL as last_accessed_at FROM skeleton_entries"
        )
        .fetch_all(&pool)
        .await?;
        
        let entries = rows.into_iter().map(Self::row_to_entry).collect();
        Ok(entries)
    }
    
    async fn delete_skeleton_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
        let pool = self.get_pool(project_id).await?;
        sqlx::query("DELETE FROM skeleton_entries WHERE id = ?")
            .bind(entry_id)
            .execute(&pool)
            .await?;
        
        Ok(())
    }
    
    async fn purge_skeleton_entries(&self, project_id: &str) -> Result<usize> {
        let pool = self.get_pool(project_id).await?;
        let result = sqlx::query("DELETE FROM skeleton_entries")
            .execute(&pool)
            .await?;
        
        Ok(result.rows_affected() as usize)
    }
    
    async fn skeleton_stats(&self, project_id: &str) -> Result<(usize, usize, usize, usize)> {
        let pool = self.get_pool(project_id).await?;
        let row = sqlx::query(
            "SELECT COUNT(*) as total, COALESCE(SUM(LENGTH(content)), 0) as size_bytes FROM skeleton_entries"
        )
        .fetch_one(&pool)
        .await?;
        
        let total: i64 = row.get("total");
        let size_bytes: i64 = row.get("size_bytes");
        
        Ok((total as usize, 0, total as usize, size_bytes as usize))
    }
    
    async fn embed_text(&self, _text: &str) -> Vec<f32> {
        // SQLite doesn't have built-in embeddings - HybridStorage handles this with Redis
        Vec::new()
    }
}
