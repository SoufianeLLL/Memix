//! Pattern detection for command discovery.
//!
//! Analyzes command execution history to detect patterns and identify
//! opportunities for new filter rules.

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::tracker::{CommandTracker, CommandStats};

/// Detected command pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    /// Base command
    pub command: String,
    /// Detected argument pattern (regex)
    pub arg_pattern: String,
    /// Frequency of this pattern
    pub frequency: usize,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Average output size (tokens)
    pub avg_output_size: usize,
    /// Whether this pattern is already filtered
    pub is_filtered: bool,
    /// Suggested filter type
    pub suggested_filter: Option<String>,
}

/// Pattern detector configuration
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// Minimum frequency to consider a pattern
    pub min_frequency: usize,
    /// Minimum confidence threshold
    pub min_confidence: f64,
    /// Maximum patterns to detect
    pub max_patterns: usize,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            min_frequency: 3,
            min_confidence: 0.6,
            max_patterns: 50,
        }
    }
}

/// Pattern detector
pub struct PatternDetector {
    config: DetectorConfig,
    /// Known command patterns (from TOML filters)
    known_patterns: Vec<String>,
    /// Regex for extracting flags
    flag_regex: Regex,
    /// Regex for extracting file paths
    path_regex: Regex,
}

impl PatternDetector {
    pub fn new(config: DetectorConfig) -> Self {
        Self {
            config,
            known_patterns: Vec::new(),
            flag_regex: Regex::new(r"--?[a-zA-Z][a-zA-Z0-9-]*").unwrap(),
            path_regex: Regex::new(r"[^\s]+\.[a-zA-Z]{1,10}").unwrap(),
        }
    }
    
    /// Detect patterns from command tracker
    pub fn detect(&self, tracker: &CommandTracker) -> Result<Vec<DetectedPattern>> {
        let mut patterns = Vec::new();
        
        // Get all command frequencies
        let frequencies = tracker.get_all_frequencies();
        
        for (command, &freq) in frequencies {
            if freq < self.config.min_frequency {
                continue;
            }
            
            // Check if already filtered
            let is_filtered = crate::token::TOML_FILTER_REGISTRY
                .find_filter(&command)
                .is_some();
            
            // Get argument patterns
            if let Some(args) = tracker.get_arg_patterns(&command) {
                let detected = self.detect_arg_patterns(&command, args, freq, is_filtered);
                patterns.extend(detected);
            }
            
            // If not filtered and has high frequency, suggest as new pattern
            if !is_filtered && freq >= self.config.min_frequency {
                patterns.push(DetectedPattern {
                    command: command.clone(),
                    arg_pattern: ".*".to_string(),
                    frequency: freq,
                    confidence: self.calculate_confidence(freq, 0),
                    avg_output_size: 100, // Default estimate
                    is_filtered: false,
                    suggested_filter: Some("minimal".to_string()),
                });
            }
        }
        
        // Sort by confidence and frequency
        patterns.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.frequency.cmp(&a.frequency))
        });
        
        // Limit to max patterns
        patterns.truncate(self.config.max_patterns);
        
        Ok(patterns)
    }
    
    /// Detect argument patterns for a command
    fn detect_arg_patterns(
        &self,
        command: &str,
        args: &HashMap<String, usize>,
        total_freq: usize,
        is_filtered: bool,
    ) -> Vec<DetectedPattern> {
        let mut patterns = Vec::new();
        
        // Group flags by frequency
        let mut flags: Vec<(&String, &usize)> = args.iter()
            .filter(|(arg, _)| arg.starts_with('-'))
            .collect();
        
        flags.sort_by_key(|(_, freq)| std::cmp::Reverse(*freq));
        
        // Detect common flag combinations
        if flags.len() >= 2 {
            let common_flags: Vec<String> = flags.iter()
                .take(3)
                .map(|(arg, _)| (*arg).clone())
                .collect();
            
            let pattern = format!("^{}\\s+({})", command, common_flags.join("|"));
            
            let flag_freq: usize = flags.iter().take(3).map(|(_, f)| **f).sum();
            
            patterns.push(DetectedPattern {
                command: command.to_string(),
                arg_pattern: pattern,
                frequency: flag_freq,
                confidence: self.calculate_confidence(flag_freq, total_freq),
                avg_output_size: 100,
                is_filtered,
                suggested_filter: if !is_filtered { Some("minimal".to_string()) } else { None },
            });
        }
        
        // Detect file patterns
        let file_args: Vec<(&String, &usize)> = args.iter()
            .filter(|(arg, _)| self.path_regex.is_match(arg))
            .collect();
        
        if !file_args.is_empty() {
            let file_freq: usize = file_args.iter().map(|(_, f)| **f).sum();
            
            patterns.push(DetectedPattern {
                command: command.to_string(),
                arg_pattern: format!(r"^{}\s+.*\.\w+$", command),
                frequency: file_freq,
                confidence: self.calculate_confidence(file_freq, total_freq),
                avg_output_size: 100,
                is_filtered,
                suggested_filter: if !is_filtered { Some("minimal".to_string()) } else { None },
            });
        }
        
        patterns
    }
    
    /// Calculate confidence score
    fn calculate_confidence(&self, pattern_freq: usize, total_freq: usize) -> f64 {
        if total_freq == 0 {
            return 0.0;
        }
        
        let frequency_score = (pattern_freq as f64 / total_freq as f64).min(1.0);
        
        // Higher frequency = higher confidence
        let volume_score = (pattern_freq as f64 / 100.0).min(1.0);
        
        // Weighted average
        (frequency_score * 0.6 + volume_score * 0.4).min(1.0)
    }
    
    /// Update known patterns from existing filters
    pub fn update_known_patterns(&mut self, patterns: Vec<String>) {
        self.known_patterns = patterns;
    }
    
    /// Check if a command matches a known pattern
    pub fn is_known(&self, command: &str) -> bool {
        self.known_patterns.iter().any(|p| command.starts_with(p))
    }
}

impl Default for PatternDetector {
    fn default() -> Self {
        Self::new(DetectorConfig::default())
    }
}
