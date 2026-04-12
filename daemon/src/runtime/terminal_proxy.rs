//! Terminal command proxy for token-efficient output processing.
//!
//! Intercepts terminal commands and pipes output through TOML filters
//! to reduce token consumption by 60-90%.
//!
//! # Architecture
//!
//! 1. Command is received from AI agent or extension
//! 2. Command is matched against TOML filter registry
//! 3. If match found, output is filtered through 8-stage pipeline
//! 4. Filtered output is returned to caller
//!
//! # Usage
//!
//! ```ignore
//! use crate::runtime::TerminalProxy;
//!
//! let proxy = TerminalProxy::new();
//! let output = proxy.execute("git status").await?;
//! // Output is now compact and token-efficient
//! ```

use anyhow::{Context, Result};
use std::process::Command;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command as AsyncCommand;

use crate::token::{CompiledFilter, TOML_FILTER_REGISTRY};

/// Terminal proxy that intercepts and filters command output
pub struct TerminalProxy {
    /// Whether to apply filters (can be disabled for debugging)
    enabled: bool,
    /// Whether to save raw output on failure (tee mode)
    tee_on_failure: bool,
}

impl Default for TerminalProxy {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalProxy {
    pub fn new() -> Self {
        Self {
            enabled: true,
            tee_on_failure: true,
        }
    }

    /// Create a proxy with filtering disabled
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            tee_on_failure: false,
        }
    }

    /// Execute a command and return filtered output
    pub async fn execute(&self, command: &str) -> Result<ProxyResult> {
        let start = Instant::now();
        
        // Parse command into program and args
        let parts = shell_words::split(command)
            .with_context(|| format!("Failed to parse command: {}", command))?;
        
        if parts.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }
        
        let program = &parts[0];
        let args = &parts[1..];
        
        // Execute command
        let output = AsyncCommand::new(program)
            .args(args)
            .output()
            .await
            .with_context(|| format!("Failed to execute: {}", command))?;
        
        let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let raw_stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        // Combine stdout and stderr for filtering
        let raw_output = if raw_stderr.is_empty() {
            raw_stdout.clone()
        } else {
            format!("{}\n{}", raw_stdout, raw_stderr)
        };
        
        // Find and apply filter
        let (filtered_output, filter_name) = if self.enabled {
            if let Some(filter) = TOML_FILTER_REGISTRY.find_filter(command) {
                (filter.apply(&raw_output), Some(filter.name.clone()))
            } else {
                (raw_output.clone(), None)
            }
        } else {
            (raw_output.clone(), None)
        };
        
        // Calculate token savings
        let raw_tokens = estimate_tokens(&raw_output);
        let filtered_tokens = estimate_tokens(&filtered_output);
        let tokens_saved = raw_tokens.saturating_sub(filtered_tokens);
        
        // Tee raw output on failure
        if self.tee_on_failure && exit_code != 0 {
            self.save_tee(command, &raw_output, exit_code)?;
        }
        
        Ok(ProxyResult {
            output: filtered_output,
            exit_code,
            filter_applied: filter_name,
            raw_tokens,
            filtered_tokens,
            tokens_saved,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Execute a command synchronously (for non-async contexts)
    pub fn execute_sync(&self, command: &str) -> Result<ProxyResult> {
        let start = Instant::now();
        
        let parts = shell_words::split(command)
            .with_context(|| format!("Failed to parse command: {}", command))?;
        
        if parts.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }
        
        let output = Command::new(&parts[0])
            .args(&parts[1..])
            .output()
            .with_context(|| format!("Failed to execute: {}", command))?;
        
        let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let raw_stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        let raw_output = if raw_stderr.is_empty() {
            raw_stdout
        } else {
            format!("{}\n{}", raw_stdout, raw_stderr)
        };
        
        let (filtered_output, filter_name) = if self.enabled {
            if let Some(filter) = TOML_FILTER_REGISTRY.find_filter(command) {
                (filter.apply(&raw_output), Some(filter.name.clone()))
            } else {
                (raw_output.clone(), None)
            }
        } else {
            (raw_output.clone(), None)
        };
        
        let raw_tokens = estimate_tokens(&raw_output);
        let filtered_tokens = estimate_tokens(&filtered_output);
        let tokens_saved = raw_tokens.saturating_sub(filtered_tokens);
        
        if self.tee_on_failure && exit_code != 0 {
            self.save_tee(command, &raw_output, exit_code)?;
        }
        
        Ok(ProxyResult {
            output: filtered_output,
            exit_code,
            filter_applied: filter_name,
            raw_tokens,
            filtered_tokens,
            tokens_saved,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Save raw output to tee directory on failure
    fn save_tee(&self, command: &str, output: &str, exit_code: i32) -> Result<()> {
        use std::fs;
        use std::path::PathBuf;
        
        let tee_dir = dirs::data_local_dir()
            .map(|d| d.join("memix").join("tee"))
            .unwrap_or_else(|| PathBuf::from("/tmp/memix/tee"));
        
        fs::create_dir_all(&tee_dir)?;
        
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let slug = sanitize_slug(command);
        let filename = format!("{}_{}_{}.log", timestamp, slug, exit_code);
        
        let path = tee_dir.join(filename);
        fs::write(&path, output)?;
        
        tracing::info!("Saved raw output to {:?}", path);
        
        // Cleanup old files (keep last 20)
        self.cleanup_tee_files(&tee_dir, 20)?;
        
        Ok(())
    }

    /// Remove old tee files, keeping only the most recent N
    fn cleanup_tee_files(&self, dir: &std::path::Path, keep: usize) -> Result<()> {
        use std::fs;
        
        let mut entries: Vec<_> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "log").unwrap_or(false))
            .collect();
        
        if entries.len() <= keep {
            return Ok(());
        }
        
        // Sort by filename (chronological)
        entries.sort_by_key(|e| e.file_name());
        
        let to_remove = entries.len() - keep;
        for entry in entries.into_iter().take(to_remove) {
            let _ = fs::remove_file(entry.path());
        }
        
        Ok(())
    }

    /// Check if a command would be filtered
    pub fn would_filter(&self, command: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }
        TOML_FILTER_REGISTRY.find_filter(command).map(|f| f.name.clone())
    }
}

/// Result of a proxied command execution
#[derive(Debug, Clone)]
pub struct ProxyResult {
    /// Filtered output
    pub output: String,
    /// Exit code
    pub exit_code: i32,
    /// Name of filter applied (if any)
    pub filter_applied: Option<String>,
    /// Estimated tokens in raw output
    pub raw_tokens: usize,
    /// Estimated tokens in filtered output
    pub filtered_tokens: usize,
    /// Tokens saved by filtering
    pub tokens_saved: usize,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
}

impl ProxyResult {
    /// Get the savings percentage
    pub fn savings_percent(&self) -> f64 {
        if self.raw_tokens == 0 {
            return 0.0;
        }
        (self.tokens_saved as f64 / self.raw_tokens as f64) * 100.0
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Estimate token count for a string (rough approximation)
fn estimate_tokens(s: &str) -> usize {
    // Rough estimate: ~4 characters per token for code
    // This is fast and good enough for savings estimation
    s.len() / 4
}

/// Sanitize a command for use in filenames
fn sanitize_slug(s: &str) -> String {
    let sanitized: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    
    // Truncate to 40 chars
    if sanitized.len() > 40 {
        sanitized[..40].to_string()
    } else {
        sanitized
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_slug() {
        assert_eq!(sanitize_slug("git status"), "git_status");
        assert_eq!(sanitize_slug("npm install --save-dev"), "npm_install_--save-dev");
        assert!(sanitize_slug("a".repeat(100)).len() <= 40);
    }

    #[test]
    fn test_estimate_tokens() {
        assert!(estimate_tokens("hello world") > 0);
        assert!(estimate_tokens("fn main() { println!(\"hello\"); }") > 0);
    }

    #[test]
    fn test_proxy_result_savings() {
        let result = ProxyResult {
            output: "filtered".to_string(),
            exit_code: 0,
            filter_applied: Some("test".to_string()),
            raw_tokens: 100,
            filtered_tokens: 20,
            tokens_saved: 80,
            duration_ms: 10,
        };
        
        assert_eq!(result.savings_percent(), 80.0);
    }
}
