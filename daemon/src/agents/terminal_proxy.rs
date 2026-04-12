use std::process::Command;
use crate::token::filters::filter::{FilterLevel, Language, get_filter, smart_truncate};

pub struct TerminalProxy;

impl TerminalProxy {
    /// Intercepts a command execution, applying RTK-style compression heuristics.
    /// If the command fails, it falls back to raw output so nothing is completely lost.
    pub fn execute(cmd_str: &str, lang: Language) -> Result<String, std::io::Error> {
        let is_known = Self::is_known_proxy_target(cmd_str);
        
        let output = if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", cmd_str]).output()?
        } else {
            Command::new("sh").args(["-c", cmd_str]).output()?
        };
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        
        if is_known && output.status.success() {
            // Apply aggressive filtering for our target token optimization
            let filter_strategy = get_filter(FilterLevel::Aggressive);
            let compressed = filter_strategy.filter(&stdout, &lang);
            
            // Truncate to a practical limit for LLM context
            let final_output = smart_truncate(&compressed, 400, &lang);
            Ok(final_output)
        } else {
            // raw fallback or error propagation (with stderr)
            let mut result = stdout;
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if !stderr.is_empty() {
                result.push_str("\n[STDERR]\n");
                result.push_str(&stderr);
            }
            Ok(result)
        }
    }

    fn is_known_proxy_target(cmd: &str) -> bool {
        // Fast checks to see if this is something RTK optimizes well
        let c = cmd.to_lowercase();
        c.starts_with("cargo test") || 
        c.starts_with("git log") || 
        c.starts_with("npm list") ||
        c.starts_with("pytest") ||
        c.starts_with("cat ") ||
        c.starts_with("read ")
    }
}
