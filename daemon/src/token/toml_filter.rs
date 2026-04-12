//! TOML-based declarative filter system for terminal output optimization.
//!
//! Provides an 8-stage pipeline that can be configured via TOML files:
//!   1. strip_ansi           - remove ANSI escape codes
//!   2. replace              - regex substitutions, line-by-line
//!   3. match_output         - short-circuit: if blob matches, return message
//!   4. strip/keep_lines     - filter lines by regex
//!   5. truncate_lines_at    - truncate each line to N chars
//!   6. head/tail_lines      - keep first/last N lines
//!   7. max_lines            - absolute line cap
//!   8. on_empty             - message if result is empty
//!

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};
use serde::Deserialize;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Deserialization types (TOML schema)
// ---------------------------------------------------------------------------

/// A match-output rule: if pattern matches, short-circuit and return message.
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct MatchOutputRule {
    pub pattern: String,
    pub message: String,
    #[serde(default)]
    pub unless: Option<String>,
}

/// A regex substitution applied line-by-line.
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ReplaceRule {
    pub pattern: String,
    pub replacement: String,
}

/// Inline test definition
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct TomlFilterTest {
    pub name: String,
    pub input: String,
    pub expected: String,
}

#[derive(Deserialize)]
struct TomlFilterFile {
    schema_version: u32,
    #[serde(default)]
    filters: BTreeMap<String, TomlFilterDef>,
    #[serde(default)]
    tests: BTreeMap<String, Vec<TomlFilterTest>>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct TomlFilterDef {
    pub description: Option<String>,
    pub match_command: String,
    #[serde(default)]
    pub strip_ansi: bool,
    #[serde(default)]
    pub replace: Vec<ReplaceRule>,
    #[serde(default)]
    pub match_output: Vec<MatchOutputRule>,
    #[serde(default)]
    pub strip_lines_matching: Vec<String>,
    #[serde(default)]
    pub keep_lines_matching: Vec<String>,
    pub truncate_lines_at: Option<usize>,
    pub head_lines: Option<usize>,
    pub tail_lines: Option<usize>,
    pub max_lines: Option<usize>,
    pub on_empty: Option<String>,
}

// ---------------------------------------------------------------------------
// Compiled types (post-validation, ready to use)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CompiledMatchOutputRule {
    pattern: Regex,
    message: String,
    unless: Option<Regex>,
}

#[derive(Debug)]
struct CompiledReplaceRule {
    pattern: Regex,
    replacement: String,
}

#[derive(Debug)]
enum LineFilter {
    None,
    Strip(RegexSet),
    Keep(RegexSet),
}

/// A filter that has been parsed and compiled - all regexes are ready.
#[derive(Debug)]
pub struct CompiledFilter {
    pub name: String,
    pub description: Option<String>,
    match_regex: Regex,
    strip_ansi: bool,
    replace: Vec<CompiledReplaceRule>,
    match_output: Vec<CompiledMatchOutputRule>,
    line_filter: LineFilter,
    truncate_lines_at: Option<usize>,
    head_lines: Option<usize>,
    tail_lines: Option<usize>,
    pub max_lines: Option<usize>,
    on_empty: Option<String>,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Global filter registry loaded at startup
pub static TOML_FILTER_REGISTRY: Lazy<TomlFilterRegistry> = Lazy::new(|| {
    TomlFilterRegistry::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load TOML filters: {}", e);
        TomlFilterRegistry::empty()
    })
});

pub struct TomlFilterRegistry {
    pub filters: Vec<CompiledFilter>,
}

impl TomlFilterRegistry {
    pub fn empty() -> Self {
        Self { filters: Vec::new() }
    }

    /// Load registry from built-in TOML content
    pub fn load() -> Result<Self> {
        let mut filters = Vec::new();
        
        // Load built-in filters
        let builtin = include_str!("filters/builtin.toml");
        filters.extend(Self::parse_and_compile(builtin, "builtin")?);
        
        // TODO: Load from ~/.config/memix/filters.toml and .memix/filters.toml
        
        tracing::info!("Loaded {} TOML filters", filters.len());
        Ok(Self { filters })
    }

    fn parse_and_compile(content: &str, source: &str) -> Result<Vec<CompiledFilter>> {
        let file: TomlFilterFile = toml::from_str(content)
            .with_context(|| format!("Failed to parse TOML from {}", source))?;
        
        let mut compiled = Vec::new();
        for (name, def) in file.filters {
            match Self::compile_filter(&name, def) {
                Ok(f) => compiled.push(f),
                Err(e) => tracing::warn!("Failed to compile filter '{}': {}", name, e),
            }
        }
        Ok(compiled)
    }

    fn compile_filter(name: &str, def: TomlFilterDef) -> Result<CompiledFilter> {
        let match_regex = Regex::new(&def.match_command)
            .with_context(|| format!("Invalid match_command regex for {}", name))?;
        
        // Compile replace rules
        let replace = def.replace.into_iter().map(|r| {
            Ok(CompiledReplaceRule {
                pattern: Regex::new(&r.pattern)
                    .with_context(|| format!("Invalid replace pattern"))?,
                replacement: r.replacement,
            })
        }).collect::<Result<Vec<_>>>()?;
        
        // Compile match_output rules
        let match_output = def.match_output.into_iter().map(|r| {
            Ok(CompiledMatchOutputRule {
                pattern: Regex::new(&r.pattern)
                    .with_context(|| format!("Invalid match_output pattern"))?,
                message: r.message,
                unless: r.unless.map(|u| Regex::new(&u))
                    .transpose()?,
            })
        }).collect::<Result<Vec<_>>>()?;
        
        // Compile line filters
        let line_filter = if !def.strip_lines_matching.is_empty() {
            let patterns: Vec<&str> = def.strip_lines_matching.iter().map(|s| s.as_str()).collect();
            LineFilter::Strip(RegexSet::new(&patterns)?)
        } else if !def.keep_lines_matching.is_empty() {
            let patterns: Vec<&str> = def.keep_lines_matching.iter().map(|s| s.as_str()).collect();
            LineFilter::Keep(RegexSet::new(&patterns)?)
        } else {
            LineFilter::None
        };
        
        Ok(CompiledFilter {
            name: name.to_string(),
            description: def.description,
            match_regex,
            strip_ansi: def.strip_ansi,
            replace,
            match_output,
            line_filter,
            truncate_lines_at: def.truncate_lines_at,
            head_lines: def.head_lines,
            tail_lines: def.tail_lines,
            max_lines: def.max_lines,
            on_empty: def.on_empty,
        })
    }

    /// Find a filter that matches the given command
    pub fn find_filter(&self, command: &str) -> Option<&CompiledFilter> {
        self.filters.iter().find(|f| f.match_regex.is_match(command))
    }
}

// ---------------------------------------------------------------------------
// Filter Application
// ---------------------------------------------------------------------------

impl CompiledFilter {
    /// Apply the 8-stage filter pipeline to command output
    pub fn apply(&self, output: &str) -> String {
        let mut result = output.to_string();
        
        // Stage 1: Strip ANSI escape codes
        if self.strip_ansi {
            result = strip_ansi_codes(&result);
        }
        
        // Stage 2: Apply regex replacements (line by line)
        for rule in &self.replace {
            result = result.lines()
                .map(|line| rule.pattern.replace(line, &rule.replacement).to_string())
                .collect::<Vec<_>>()
                .join("\n");
        }
        
        // Stage 3: Match output short-circuit
        for rule in &self.match_output {
            if rule.pattern.is_match(&result) {
                // Check unless condition
                if let Some(unless) = &rule.unless {
                    if unless.is_match(&result) {
                        continue;
                    }
                }
                return rule.message.clone();
            }
        }
        
        // Stage 4: Strip/keep lines
        result = match &self.line_filter {
            LineFilter::None => result,
            LineFilter::Strip(set) => {
                result.lines()
                    .filter(|line| !set.is_match(line))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            LineFilter::Keep(set) => {
                result.lines()
                    .filter(|line| set.is_match(line))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };
        
        // Stage 5: Truncate lines
        if let Some(max_len) = self.truncate_lines_at {
            result = result.lines()
                .map(|line| {
                    if line.len() > max_len {
                        format!("{}...", &line[..max_len.saturating_sub(3)])
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        
        // Stage 6: Head/tail lines
        let lines: Vec<&str> = result.lines().collect();
        result = if let Some(head) = self.head_lines {
            lines.into_iter().take(head).collect::<Vec<_>>().join("\n")
        } else if let Some(tail) = self.tail_lines {
            lines.into_iter().rev().take(tail).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
        } else {
            lines.join("\n")
        };
        
        // Stage 7: Max lines
        if let Some(max) = self.max_lines {
            result = result.lines().take(max).collect::<Vec<_>>().join("\n");
        }
        
        // Stage 8: On empty
        if result.trim().is_empty() {
            if let Some(msg) = &self.on_empty {
                return msg.clone();
            }
        }
        
        result
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip ANSI escape codes from a string
fn strip_ansi_codes(s: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;
    
    static ANSI_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][AB012]").unwrap()
    });
    ANSI_REGEX.replace_all(s, "").to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[32mHello\x1b[0m World";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_filter_application() {
        let filter = CompiledFilter {
            name: "test".to_string(),
            description: None,
            match_regex: Regex::new("^test").unwrap(),
            strip_ansi: true,
            replace: vec![],
            match_output: vec![],
            line_filter: LineFilter::Strip(RegexSet::new(&["^\\s*$"]).unwrap()),
            truncate_lines_at: None,
            head_lines: Some(3),
            tail_lines: None,
            max_lines: None,
            on_empty: Some("ok".to_string()),
        };
        
        let input = "line1\n\nline2\n\nline3\nline4";
        let result = filter.apply(input);
        assert_eq!(result, "line1\nline2\nline3");
    }
}
