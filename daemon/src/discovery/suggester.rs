//! Filter suggestion engine.
//!
//! Proposes new TOML filter rules based on detected command patterns.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::detector::DetectedPattern;
use super::tracker::CommandStats;

/// Suggested filter rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedFilter {
    /// Filter name (usually command name)
    pub name: String,
    /// TOML filter definition
    pub toml: String,
    /// Confidence score
    pub confidence: f64,
    /// Reason for suggestion
    pub reason: String,
    /// Estimated token savings
    pub estimated_savings_pct: f64,
}

/// Filter suggester
pub struct FilterSuggester {
    /// Minimum confidence to suggest
    min_confidence: f64,
    /// Minimum frequency to suggest
    min_frequency: usize,
}

impl FilterSuggester {
    pub fn new() -> Self {
        Self {
            min_confidence: 0.5,
            min_frequency: 5,
        }
    }
    
    /// Generate filter suggestions from detected patterns
    pub fn suggest(&self, patterns: &[DetectedPattern]) -> Vec<SuggestedFilter> {
        patterns
            .iter()
            .filter(|p| !p.is_filtered)
            .filter(|p| p.confidence >= self.min_confidence)
            .filter(|p| p.frequency >= self.min_frequency)
            .map(|p| self.create_suggestion(p))
            .collect()
    }
    
    /// Create a filter suggestion from a detected pattern
    fn create_suggestion(&self, pattern: &DetectedPattern) -> SuggestedFilter {
        let name = pattern.command.replace(|c: char| !c.is_alphanumeric(), "-");
        
        let estimated_savings = self.estimate_savings(pattern);
        
        let toml = self.generate_toml(&name, pattern, estimated_savings);
        
        let reason = format!(
            "Detected {} executions with {:.0}% confidence",
            pattern.frequency,
            pattern.confidence * 100.0
        );
        
        SuggestedFilter {
            name,
            toml,
            confidence: pattern.confidence,
            reason,
            estimated_savings_pct: estimated_savings,
        }
    }
    
    /// Estimate token savings for a pattern
    fn estimate_savings(&self, pattern: &DetectedPattern) -> f64 {
        // Base savings by command type
        match pattern.command.as_str() {
            "git" => 70.0,
            "cargo" | "rustc" => 75.0,
            "npm" | "yarn" | "pnpm" => 60.0,
            "pip" | "poetry" | "uv" => 55.0,
            "docker" | "kubectl" => 65.0,
            "make" | "cmake" | "gradle" => 50.0,
            "pytest" | "jest" | "vitest" => 80.0,
            "ls" | "find" | "tree" => 60.0,
            "grep" | "rg" | "fd" => 55.0,
            _ => 40.0,
        }
    }
    
    /// Generate TOML filter definition
    fn generate_toml(&self, name: &str, pattern: &DetectedPattern, savings: f64) -> String {
        let description = format!(
            "Auto-generated filter for {} - {:.0}% estimated savings",
            pattern.command,
            savings
        );
        
        // Determine filter strategy based on command type
        let (strip_lines, max_lines, on_empty) = self.get_filter_strategy(&pattern.command);
        
        let mut toml = format!(
            r#"[filters.{}]
description = "{}"
match_command = "^{}\\b"
strip_ansi = true
"#,
            name,
            description,
            pattern.command
        );
        
        if !strip_lines.is_empty() {
            toml.push_str("strip_lines_matching = [\n");
            for line in strip_lines {
                toml.push_str(&format!("  \"{}\",\n", line));
            }
            toml.push_str("]\n");
        }
        
        if let Some(max) = max_lines {
            toml.push_str(&format!("max_lines = {}\n", max));
        }
        
        if let Some(msg) = on_empty {
            toml.push_str(&format!("on_empty = \"{}\"\n", msg));
        }
        
        toml
    }
    
    /// Get filter strategy for a command type
    fn get_filter_strategy(&self, command: &str) -> (Vec<&'static str>, Option<usize>, Option<&'static str>) {
        match command {
            "git" => (
                vec!["^On branch", "^Your branch is", "^\\s*$"],
                Some(30),
                Some("git: clean"),
            ),
            "cargo" => (
                vec!["^   Compiling", "^    Finished", "^\\s*$"],
                Some(50),
                Some("cargo: ok"),
            ),
            "npm" | "yarn" | "pnpm" => (
                vec!["^npm WARN", "^\\s*$", "up to date"],
                Some(20),
                Some("npm: ok"),
            ),
            "docker" => (
                vec!["^CONTAINER ID", "^\\s*$"],
                Some(20),
                None,
            ),
            "pytest" | "jest" | "vitest" => (
                vec!["^=+$", "^-+$", "^\\s*$", "^PASSED"],
                Some(40),
                Some("tests: passed"),
            ),
            "make" | "cmake" => (
                vec!["^\\s*$"],
                Some(50),
                None,
            ),
            _ => (
                vec!["^\\s*$"],
                Some(50),
                None,
            ),
        }
    }
    
    /// Generate a summary report of suggestions
    pub fn generate_report(&self, suggestions: &[SuggestedFilter]) -> String {
        let mut report = String::new();
        
        report.push_str("# Filter Suggestions Report\n\n");
        report.push_str(&format!("Total suggestions: {}\n\n", suggestions.len()));
        
        for suggestion in suggestions {
            report.push_str(&format!("## {} (confidence: {:.0}%)\n\n", 
                suggestion.name, 
                suggestion.confidence * 100.0
            ));
            report.push_str(&format!("**Reason:** {}\n\n", suggestion.reason));
            report.push_str(&format!("**Estimated savings:** {:.0}%\n\n", suggestion.estimated_savings_pct));
            report.push_str("```toml\n");
            report.push_str(&suggestion.toml);
            report.push_str("```\n\n");
        }
        
        report
    }
}

impl Default for FilterSuggester {
    fn default() -> Self {
        Self::new()
    }
}
