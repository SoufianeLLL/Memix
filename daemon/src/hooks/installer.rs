//! Hook installer for AI agents.
//!
//! Installs command interception hooks for Claude Code, Cursor, Windsurf, etc.
//! Each agent has its own hook format and installation location.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Hook installer for AI agents
pub struct HookInstaller {
    /// Whether hooks are enabled
    enabled: bool,
}

impl Default for HookInstaller {
    fn default() -> Self {
        Self::new()
    }
}

impl HookInstaller {
    pub fn new() -> Self {
        Self { enabled: true }
    }
    
    /// Install hooks for all supported agents
    pub fn install_all(&self) -> Result<Vec<String>> {
        let mut installed = Vec::new();
        
        if let Ok(_) = self.install_claude_code() {
            installed.push("claude-code".to_string());
        }
        
        if let Ok(_) = self.install_cursor() {
            installed.push("cursor".to_string());
        }
        
        if let Ok(_) = self.install_windsurf() {
            installed.push("windsurf".to_string());
        }
        
        Ok(installed)
    }
    
    /// Install hook for Claude Code
    pub fn install_claude_code(&self) -> Result<()> {
        let settings_path = self.get_claude_settings_path()?;
        
        // Create settings directory if needed
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Read existing settings or create new
        let mut settings: serde_json::Value = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
            serde_json::from_str(&content)?
        } else {
            serde_json::json!({})
        };
        
        // Add hook configuration
        let hook_path = self.get_hook_script_path("claude");
        
        if let Some(obj) = settings.as_object_mut() {
            obj.entry("hooks".to_string())
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .map(|h| {
                    h.insert("PreToolUse".to_string(), serde_json::json!([{
                        "matcher": "Bash",
                        "hooks": [{
                            "type": "command",
                            "command": hook_path.to_string_lossy().to_string()
                        }]
                    }]));
                });
        }
        
        // Write settings
        let content = serde_json::to_string_pretty(&settings)?;
        std::fs::write(&settings_path, content)?;
        
        tracing::info!("Installed Claude Code hook at {:?}", settings_path);
        Ok(())
    }
    
    /// Install hook for Cursor
    pub fn install_cursor(&self) -> Result<()> {
        // Cursor uses .cursorrules file for prompt-level guidance
        let rules_path = Path::new(".cursorrules");
        
        let rules_content = r#"# Memix Token Optimization

When executing shell commands, Memix will automatically optimize output to save tokens.

## How it works
- Commands like `git status`, `cargo test`, `npm install` are filtered
- Output is reduced by 60-90% while keeping essential information
- No workflow changes needed - optimization is transparent

## Supported commands
- Git: status, log, diff, branch, tag
- Package managers: npm, yarn, pnpm, cargo, pip
- Build tools: make, cmake, gradle
- Docker: ps, images, logs
- Testing: pytest, jest, cargo test

## To disable
Remove this file or set `MEMIX_HOOKS_DISABLED=1` in your environment.
"#;
        
        std::fs::write(rules_path, rules_content)?;
        
        tracing::info!("Installed Cursor rules at .cursorrules");
        Ok(())
    }
    
    /// Install hook for Windsurf
    pub fn install_windsurf(&self) -> Result<()> {
        // Windsurf uses .windsurfrules file
        let rules_path = Path::new(".windsurfrules");
        
        let rules_content = r#"# Memix Token Optimization

When executing shell commands, Memix will automatically optimize output to save tokens.

## How it works
- Commands like `git status`, `cargo test`, `npm install` are filtered
- Output is reduced by 60-90% while keeping essential information
- No workflow changes needed - optimization is transparent

## Supported commands
- Git: status, log, diff, branch, tag
- Package managers: npm, yarn, pnpm, cargo, pip
- Build tools: make, cmake, gradle
- Docker: ps, images, logs
- Testing: pytest, jest, cargo test

## To disable
Remove this file or set `MEMIX_HOOKS_DISABLED=1` in your environment.
"#;
        
        std::fs::write(rules_path, rules_content)?;
        
        tracing::info!("Installed Windsurf rules at .windsurfrules");
        Ok(())
    }
    
    /// Uninstall all hooks
    pub fn uninstall_all(&self) -> Result<Vec<String>> {
        let mut uninstalled = Vec::new();
        
        // Remove Claude Code hook
        let settings_path = self.get_claude_settings_path()?;
        if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
            if let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = settings.as_object_mut() {
                    obj.remove("hooks");
                }
                std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
                uninstalled.push("claude-code".to_string());
            }
        }
        
        // Remove rules files
        for rules_file in [".cursorrules", ".windsurfrules"] {
            let path = Path::new(rules_file);
            if path.exists() {
                std::fs::remove_file(path)?;
                uninstalled.push(rules_file.trim_start_matches('.').to_string());
            }
        }
        
        Ok(uninstalled)
    }
    
    /// Check if hooks are installed
    pub fn status(&self) -> Vec<HookStatus> {
        let mut status = Vec::new();
        
        // Check Claude Code
        let claude_path = self.get_claude_settings_path().unwrap_or_default();
        status.push(HookStatus {
            agent: "claude-code".to_string(),
            installed: claude_path.exists(),
            path: claude_path.to_string_lossy().to_string(),
        });
        
        // Check Cursor
        status.push(HookStatus {
            agent: "cursor".to_string(),
            installed: Path::new(".cursorrules").exists(),
            path: ".cursorrules".to_string(),
        });
        
        // Check Windsurf
        status.push(HookStatus {
            agent: "windsurf".to_string(),
            installed: Path::new(".windsurfrules").exists(),
            path: ".windsurfrules".to_string(),
        });
        
        status
    }
    
    fn get_claude_settings_path(&self) -> Result<PathBuf> {
        let home = dirs::home_dir().context("Cannot find home directory")?;
        
        // Claude Code settings location varies by platform
        #[cfg(target_os = "macos")]
        let path = home.join(".claude/settings.json");
        
        #[cfg(target_os = "linux")]
        let path = home.join(".claude/settings.json");
        
        #[cfg(target_os = "windows")]
        let path = dirs::config_dir()
            .unwrap_or_else(|| home.join(".config"))
            .join("claude/settings.json");
        
        Ok(path)
    }
    
    fn get_hook_script_path(&self, agent: &str) -> PathBuf {
        // Hooks are stored in the daemon's data directory
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("memix")
            .join("hooks");
        
        data_dir.join(format!("{}-hook.sh", agent))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookStatus {
    pub agent: String,
    pub installed: bool,
    pub path: String,
}

use serde::{Deserialize, Serialize};
