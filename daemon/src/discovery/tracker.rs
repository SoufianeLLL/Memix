//! Command execution tracking for pattern discovery.
//!
//! Records command executions with timestamps, exit codes, and token savings.
//! Provides data for the discovery engine to detect new patterns.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use sqlx::SqlitePool;
use sqlx::Row;

/// Command execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    /// Unique ID
    pub id: String,
    /// The command string
    pub command: String,
    /// Base command (first word)
    pub base_command: String,
    /// Arguments (remaining words)
    pub args: Vec<String>,
    /// Exit code
    pub exit_code: i32,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Tokens in raw output
    pub raw_tokens: usize,
    /// Tokens in filtered output
    pub filtered_tokens: usize,
    /// Whether a filter was applied
    pub filter_applied: Option<String>,
    /// Working directory
    pub cwd: Option<String>,
    /// Project ID
    pub project_id: Option<String>,
}

/// Aggregated command statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandStats {
    /// Base command
    pub command: String,
    /// Total executions
    pub total_executions: usize,
    /// Successful executions (exit code 0)
    pub successful: usize,
    /// Failed executions
    pub failed: usize,
    /// Average raw tokens
    pub avg_raw_tokens: f64,
    /// Average filtered tokens
    pub avg_filtered_tokens: f64,
    /// Average savings percentage
    pub avg_savings_pct: f64,
    /// Total tokens saved
    pub total_tokens_saved: usize,
    /// Most common arguments
    pub common_args: Vec<(String, usize)>,
    /// First seen
    pub first_seen: DateTime<Utc>,
    /// Last seen
    pub last_seen: DateTime<Utc>,
}

/// Command tracker with in-memory cache and SQLite persistence
pub struct CommandTracker {
    /// In-memory command frequency
    frequency: HashMap<String, usize>,
    /// In-memory argument patterns
    arg_patterns: HashMap<String, HashMap<String, usize>>,
    /// Recent commands (last 1000)
    recent: Vec<CommandRecord>,
    /// SQLite pool for persistence
    pool: Option<SqlitePool>,
    /// Database path
    db_path: PathBuf,
}

impl CommandTracker {
    /// Create a new tracker (sync version for non-async contexts)
    pub fn new() -> Result<Self> {
        let data_dir = dirs::data_local_dir()
            .map(|d| d.join("memix"))
            .unwrap_or_else(|| PathBuf::from("/tmp/memix"));
        
        std::fs::create_dir_all(&data_dir)?;
        
        let db_path = data_dir.join("command_history.db");
        
        Ok(Self {
            frequency: HashMap::new(),
            arg_patterns: HashMap::new(),
            recent: Vec::with_capacity(1000),
            pool: None,
            db_path,
        })
    }
    
    /// Initialize async database connection
    pub async fn init_pool(&mut self) -> Result<()> {
        let db_url = format!("sqlite:{}?mode=rwc", self.db_path.display());
        let pool = SqlitePool::connect(&db_url).await?;
        
        // Create tables
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS command_history (
                id TEXT PRIMARY KEY,
                command TEXT NOT NULL,
                base_command TEXT NOT NULL,
                args TEXT,
                exit_code INTEGER,
                timestamp TEXT NOT NULL,
                raw_tokens INTEGER,
                filtered_tokens INTEGER,
                filter_applied TEXT,
                cwd TEXT,
                project_id TEXT
            )
        "#)
        .execute(&pool)
        .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_base_command ON command_history(base_command)")
            .execute(&pool)
            .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_timestamp ON command_history(timestamp)")
            .execute(&pool)
            .await?;
        
        // Load recent records into memory
        let rows = sqlx::query(
            "SELECT id, command, base_command, args, exit_code, timestamp, raw_tokens, filtered_tokens, filter_applied, cwd, project_id 
             FROM command_history ORDER BY timestamp DESC LIMIT 1000"
        )
        .fetch_all(&pool)
        .await?;
        
        for row in rows.into_iter().rev() {
            let record = CommandRecord {
                id: row.get(0),
                command: row.get(1),
                base_command: row.get(2),
                args: serde_json::from_str(&row.get::<String, _>(3)).unwrap_or_default(),
                exit_code: row.get(4),
                timestamp: DateTime::parse_from_rfc3339(&row.get::<String, _>(5))
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                raw_tokens: row.get::<i64, _>(6) as usize,
                filtered_tokens: row.get::<i64, _>(7) as usize,
                filter_applied: row.get(8),
                cwd: row.get(9),
                project_id: row.get(10),
            };
            
            *self.frequency.entry(record.base_command.clone()).or_insert(0) += 1;
            for arg in &record.args {
                let patterns = self.arg_patterns
                    .entry(record.base_command.clone())
                    .or_insert_with(HashMap::new);
                *patterns.entry(arg.clone()).or_insert(0) += 1;
            }
            self.recent.push(record);
        }
        
        self.pool = Some(pool);
        Ok(())
    }
    
    /// Record a command execution (sync version - only updates memory)
    pub fn record(&mut self, record: CommandRecord) -> Result<()> {
        // Update in-memory frequency
        *self.frequency.entry(record.base_command.clone()).or_insert(0) += 1;
        
        // Update argument patterns
        for arg in &record.args {
            let patterns = self.arg_patterns
                .entry(record.base_command.clone())
                .or_insert_with(HashMap::new);
            *patterns.entry(arg.clone()).or_insert(0) += 1;
        }
        
        // Add to recent
        self.recent.push(record.clone());
        if self.recent.len() > 1000 {
            self.recent.remove(0);
        }
        
        Ok(())
    }
    
    /// Record a command execution (async version - persists to SQLite)
    pub async fn record_async(&mut self, record: CommandRecord) -> Result<()> {
        // Update memory first
        self.record(record.clone())?;
        
        // Persist to SQLite if pool is available
        if let Some(pool) = &self.pool {
            sqlx::query(
                "INSERT OR REPLACE INTO command_history 
                 (id, command, base_command, args, exit_code, timestamp, raw_tokens, filtered_tokens, filter_applied, cwd, project_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
            )
            .bind(&record.id)
            .bind(&record.command)
            .bind(&record.base_command)
            .bind(serde_json::to_string(&record.args)?)
            .bind(record.exit_code)
            .bind(record.timestamp.to_rfc3339())
            .bind(record.raw_tokens as i64)
            .bind(record.filtered_tokens as i64)
            .bind(&record.filter_applied)
            .bind(&record.cwd)
            .bind(&record.project_id)
            .execute(pool)
            .await?;
        }
        
        Ok(())
    }
    
    /// Get command frequency
    pub fn get_frequency(&self, command: &str) -> usize {
        self.frequency.get(command).copied().unwrap_or(0)
    }
    
    /// Get all command frequencies
    pub fn get_all_frequencies(&self) -> &HashMap<String, usize> {
        &self.frequency
    }
    
    /// Get argument patterns for a command
    pub fn get_arg_patterns(&self, command: &str) -> Option<&HashMap<String, usize>> {
        self.arg_patterns.get(command)
    }
    
    /// Get recent commands
    pub fn get_recent(&self, limit: usize) -> &[CommandRecord] {
        let start = self.recent.len().saturating_sub(limit);
        &self.recent[start..]
    }
    
    /// Get command statistics
    pub fn get_stats(&self, command: &str) -> Result<Option<CommandStats>> {
        let records: Vec<_> = self.recent.iter()
            .filter(|r| r.base_command == command)
            .collect();
        
        if records.is_empty() {
            return Ok(None);
        }
        
        let total_executions = records.len();
        let successful = records.iter().filter(|r| r.exit_code == 0).count();
        let failed = total_executions - successful;
        
        let avg_raw_tokens = records.iter().map(|r| r.raw_tokens).sum::<usize>() as f64 / total_executions as f64;
        let avg_filtered_tokens = records.iter().map(|r| r.filtered_tokens).sum::<usize>() as f64 / total_executions as f64;
        
        let total_raw: usize = records.iter().map(|r| r.raw_tokens).sum();
        let total_filtered: usize = records.iter().map(|r| r.filtered_tokens).sum();
        let avg_savings_pct = if total_raw > 0 {
            (total_raw - total_filtered) as f64 / total_raw as f64 * 100.0
        } else {
            0.0
        };
        
        let total_tokens_saved = total_raw.saturating_sub(total_filtered);
        
        let first_seen = records.iter().map(|r| r.timestamp).min().unwrap_or_else(Utc::now);
        let last_seen = records.iter().map(|r| r.timestamp).max().unwrap_or_else(Utc::now);
        
        Ok(Some(CommandStats {
            command: command.to_string(),
            total_executions,
            successful,
            failed,
            avg_raw_tokens,
            avg_filtered_tokens,
            avg_savings_pct,
            total_tokens_saved,
            common_args: vec![],
            first_seen,
            last_seen,
        }))
    }
    
    /// Get all command statistics
    pub fn get_all_stats(&self) -> Result<Vec<CommandStats>> {
        let mut stats_map: HashMap<String, Vec<&CommandRecord>> = HashMap::new();
        
        for record in &self.recent {
            stats_map.entry(record.base_command.clone()).or_default().push(record);
        }
        
        let mut stats: Vec<CommandStats> = stats_map.into_iter().map(|(command, records)| {
            let total_executions = records.len();
            let successful = records.iter().filter(|r| r.exit_code == 0).count();
            let failed = total_executions - successful;
            
            let avg_raw_tokens = records.iter().map(|r| r.raw_tokens).sum::<usize>() as f64 / total_executions as f64;
            let avg_filtered_tokens = records.iter().map(|r| r.filtered_tokens).sum::<usize>() as f64 / total_executions as f64;
            
            let total_raw: usize = records.iter().map(|r| r.raw_tokens).sum();
            let total_filtered: usize = records.iter().map(|r| r.filtered_tokens).sum();
            let avg_savings_pct = if total_raw > 0 {
                (total_raw - total_filtered) as f64 / total_raw as f64 * 100.0
            } else {
                0.0
            };
            
            let total_tokens_saved = total_raw.saturating_sub(total_filtered);
            
            let first_seen = records.iter().map(|r| r.timestamp).min().unwrap_or_else(Utc::now);
            let last_seen = records.iter().map(|r| r.timestamp).max().unwrap_or_else(Utc::now);
            
            CommandStats {
                command,
                total_executions,
                successful,
                failed,
                avg_raw_tokens,
                avg_filtered_tokens,
                avg_savings_pct,
                total_tokens_saved,
                common_args: vec![],
                first_seen,
                last_seen,
            }
        }).collect();
        
        stats.sort_by(|a, b| b.total_executions.cmp(&a.total_executions));
        
        Ok(stats)
    }
    
    /// Cleanup old records (keep last 90 days)
    pub async fn cleanup_old_records(&mut self) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(90);
        
        let original_len = self.recent.len();
        self.recent.retain(|r| r.timestamp > cutoff);
        
        // Rebuild frequency and patterns
        self.frequency.clear();
        self.arg_patterns.clear();
        
        for record in &self.recent {
            *self.frequency.entry(record.base_command.clone()).or_insert(0) += 1;
            for arg in &record.args {
                let patterns = self.arg_patterns
                    .entry(record.base_command.clone())
                    .or_insert_with(HashMap::new);
                *patterns.entry(arg.clone()).or_insert(0) += 1;
            }
        }
        
        // Delete from SQLite
        if let Some(pool) = &self.pool {
            sqlx::query("DELETE FROM command_history WHERE timestamp < ?1")
                .bind(cutoff.to_rfc3339())
                .execute(pool)
                .await?;
        }
        
        Ok(original_len.saturating_sub(self.recent.len()))
    }
}

impl Default for CommandTracker {
    fn default() -> Self {
        Self::new().expect("Failed to initialize command tracker")
    }
}
