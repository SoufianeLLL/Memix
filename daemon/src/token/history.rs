//! Token usage history with SQLite persistence.
//!
//! Tracks token usage over time for analytics and optimization.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use sqlx::SqlitePool;
use sqlx::Row;

/// Token usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsageRecord {
    /// Unique ID
    pub id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Project ID
    pub project_id: String,
    /// Model used
    pub model: String,
    /// Input tokens
    pub input_tokens: usize,
    /// Output tokens
    pub output_tokens: usize,
    /// Total tokens
    pub total_tokens: usize,
    /// Context type (prompt, completion, etc.)
    pub context_type: String,
    /// Whether caching was used
    pub cached: bool,
    /// Estimated cost (USD)
    pub cost: f64,
}

/// Aggregated token statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStats {
    /// Project ID
    pub project_id: String,
    /// Total input tokens
    pub total_input: usize,
    /// Total output tokens
    pub total_output: usize,
    /// Total tokens
    pub total_tokens: usize,
    /// Total cost
    pub total_cost: f64,
    /// Cache hit rate
    pub cache_hit_rate: f64,
    /// Average tokens per day
    pub avg_daily_tokens: f64,
    /// First recorded
    pub first_recorded: DateTime<Utc>,
    /// Last recorded
    pub last_recorded: DateTime<Utc>,
}

/// Token history tracker with SQLite persistence
pub struct TokenHistory {
    /// SQLite pool
    pool: Option<SqlitePool>,
    /// Database path
    db_path: PathBuf,
    /// In-memory cache of recent records
    recent: Vec<TokenUsageRecord>,
}

impl TokenHistory {
    /// Create a new token history tracker
    pub fn new() -> Result<Self> {
        let data_dir = dirs::data_local_dir()
            .map(|d| d.join("memix"))
            .unwrap_or_else(|| PathBuf::from("/tmp/memix"));
        
        std::fs::create_dir_all(&data_dir)?;
        
        let db_path = data_dir.join("token_history.db");
        
        Ok(Self {
            pool: None,
            db_path,
            recent: Vec::with_capacity(500),
        })
    }
    
    /// Initialize async database connection
    pub async fn init_pool(&mut self) -> Result<()> {
        let db_url = format!("sqlite:{}?mode=rwc", self.db_path.display());
        let pool = SqlitePool::connect(&db_url).await?;
        
        // Create tables
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS token_history (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                project_id TEXT NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER,
                output_tokens INTEGER,
                total_tokens INTEGER,
                context_type TEXT,
                cached INTEGER,
                cost REAL
            )
        "#)
        .execute(&pool)
        .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_token_project ON token_history(project_id)")
            .execute(&pool)
            .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_token_timestamp ON token_history(timestamp)")
            .execute(&pool)
            .await?;
        
        self.pool = Some(pool);
        Ok(())
    }
    
    /// Record token usage (sync - memory only)
    pub fn record(&mut self, record: TokenUsageRecord) -> Result<()> {
        self.recent.push(record);
        if self.recent.len() > 500 {
            self.recent.remove(0);
        }
        Ok(())
    }
    
    /// Record token usage (async - persists to SQLite)
    pub async fn record_async(&mut self, record: TokenUsageRecord) -> Result<()> {
        self.record(record.clone())?;
        
        if let Some(pool) = &self.pool {
            sqlx::query(
                "INSERT INTO token_history 
                 (id, timestamp, project_id, model, input_tokens, output_tokens, total_tokens, context_type, cached, cost)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
            )
            .bind(&record.id)
            .bind(record.timestamp.to_rfc3339())
            .bind(&record.project_id)
            .bind(&record.model)
            .bind(record.input_tokens as i64)
            .bind(record.output_tokens as i64)
            .bind(record.total_tokens as i64)
            .bind(&record.context_type)
            .bind(if record.cached { 1i64 } else { 0i64 })
            .bind(record.cost)
            .execute(pool)
            .await?;
        }
        
        Ok(())
    }
    
    /// Get statistics for a project
    pub async fn get_stats(&self, project_id: &str) -> Result<Option<TokenStats>> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(None),
        };
        
        let row = sqlx::query(
            "SELECT 
                SUM(input_tokens) as total_input,
                SUM(output_tokens) as total_output,
                SUM(total_tokens) as total_tokens,
                SUM(cost) as total_cost,
                SUM(CASE WHEN cached = 1 THEN 1 ELSE 0 END) as cached_count,
                COUNT(*) as total_count,
                MIN(timestamp) as first_recorded,
                MAX(timestamp) as last_recorded
             FROM token_history WHERE project_id = ?1"
        )
        .bind(project_id)
        .fetch_optional(pool)
        .await?;
        
        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        
        let total_input: i64 = row.try_get("total_input")?;
        let total_output: i64 = row.try_get("total_output")?;
        let total_tokens: i64 = row.try_get("total_tokens")?;
        let total_count: i64 = row.try_get("total_count")?;
        
        if total_count == 0 {
            return Ok(None);
        }
        
        let first_str: String = row.try_get("first_recorded")?;
        let last_str: String = row.try_get("last_recorded")?;
        
        let first_recorded = DateTime::parse_from_rfc3339(&first_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let last_recorded = DateTime::parse_from_rfc3339(&last_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        
        let days = (last_recorded - first_recorded).num_days().max(1) as f64;
        
        Ok(Some(TokenStats {
            project_id: project_id.to_string(),
            total_input: total_input as usize,
            total_output: total_output as usize,
            total_tokens: total_tokens as usize,
            total_cost: row.try_get("total_cost")?,
            cache_hit_rate: {
                let cached: i64 = row.try_get("cached_count")?;
                if total_count > 0 { cached as f64 / total_count as f64 } else { 0.0 }
            },
            avg_daily_tokens: total_tokens as f64 / days,
            first_recorded,
            last_recorded,
        }))
    }
    
    /// Get all project statistics
    pub async fn get_all_stats(&self) -> Result<Vec<TokenStats>> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        
        let rows = sqlx::query(
            "SELECT 
                project_id,
                SUM(input_tokens) as total_input,
                SUM(output_tokens) as total_output,
                SUM(total_tokens) as total_tokens,
                SUM(cost) as total_cost,
                SUM(CASE WHEN cached = 1 THEN 1 ELSE 0 END) as cached_count,
                COUNT(*) as total_count,
                MIN(timestamp) as first_recorded,
                MAX(timestamp) as last_recorded
             FROM token_history 
             GROUP BY project_id
             ORDER BY total_tokens DESC"
        )
        .fetch_all(pool)
        .await?;
        
        let mut stats = Vec::new();
        for row in rows {
            let project_id: String = row.try_get("project_id")?;
            let total_input: i64 = row.try_get("total_input")?;
            let total_output: i64 = row.try_get("total_output")?;
            let total_tokens: i64 = row.try_get("total_tokens")?;
            let total_count: i64 = row.try_get("total_count")?;
            
            let first_str: String = row.try_get("first_recorded")?;
            let last_str: String = row.try_get("last_recorded")?;
            
            let first_recorded = DateTime::parse_from_rfc3339(&first_str)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let last_recorded = DateTime::parse_from_rfc3339(&last_str)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            
            let days = (last_recorded - first_recorded).num_days().max(1) as f64;
            
            stats.push(TokenStats {
                project_id,
                total_input: total_input as usize,
                total_output: total_output as usize,
                total_tokens: total_tokens as usize,
                total_cost: row.try_get("total_cost")?,
                cache_hit_rate: {
                    let cached: i64 = row.try_get("cached_count")?;
                    if total_count > 0 { cached as f64 / total_count as f64 } else { 0.0 }
                },
                avg_daily_tokens: total_tokens as f64 / days,
                first_recorded,
                last_recorded,
            });
        }
        
        Ok(stats)
    }
    
    /// Cleanup old records (keep last 90 days)
    pub async fn cleanup_old_records(&self) -> Result<usize> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(0),
        };
        
        let cutoff = Utc::now() - chrono::Duration::days(90);
        
        let result = sqlx::query("DELETE FROM token_history WHERE timestamp < ?1")
            .bind(cutoff.to_rfc3339())
            .execute(pool)
            .await?;
        
        Ok(result.rows_affected() as usize)
    }
}

impl Default for TokenHistory {
    fn default() -> Self {
        Self::new().expect("Failed to initialize token history")
    }
}
