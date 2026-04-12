//! Proactive risk assessment for file modifications.
//!
//! Analyzes files before edits to assess risk based on:
//! - Dependency graph (how many files depend on this)
//! - Prior breakage signals
//! - Code DNA stability/hotness
//! - Git archaeology (recent changes)

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Risk level for a file modification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Low risk - isolated changes, stable code
    Low,
    /// Medium risk - some dependencies, moderately stable
    Medium,
    /// High risk - many dependents, hot code, recent breakage
    High,
    /// Critical risk - core infrastructure, recent failures
    Critical,
}

/// Risk assessment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// File path being assessed
    pub file_path: String,
    /// Overall risk level
    pub risk_level: RiskLevel,
    /// Risk score (0-100)
    pub risk_score: u32,
    /// Number of files that depend on this file
    pub dependent_count: usize,
    /// Whether this file has had recent breakage
    pub has_recent_breakage: bool,
    /// Number of recent changes (last 7 days)
    pub recent_change_count: usize,
    /// Code stability score (0-100, higher = more stable)
    pub stability_score: u32,
    /// Risk factors identified
    pub risk_factors: Vec<RiskFactor>,
    /// Recommendations
    pub recommendations: Vec<String>,
}

/// Individual risk factor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Factor type
    pub factor_type: RiskFactorType,
    /// Description
    pub description: String,
    /// Weight in overall score
    pub weight: u32,
}

/// Types of risk factors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskFactorType {
    /// Many files depend on this
    HighDependencyCount,
    /// File has broken tests recently
    RecentBreakage,
    /// File changes frequently
    HotCode,
    /// File is part of core infrastructure
    CoreInfrastructure,
    /// File has known issues
    KnownIssues,
    /// Low test coverage
    LowTestCoverage,
    /// Complex code
    HighComplexity,
}

/// Proactive risk analyzer
pub struct RiskAnalyzer {
    /// Dependency graph (file -> files that depend on it)
    dependents: HashMap<String, HashSet<String>>,
    /// Files with recent breakage
    breakage_files: HashSet<String>,
    /// Recent change counts
    recent_changes: HashMap<String, usize>,
    /// Known issues by file
    known_issues: HashMap<String, Vec<String>>,
    /// Core infrastructure files
    core_files: HashSet<String>,
}

impl RiskAnalyzer {
    pub fn new() -> Self {
        Self {
            dependents: HashMap::new(),
            breakage_files: HashSet::new(),
            recent_changes: HashMap::new(),
            known_issues: HashMap::new(),
            core_files: HashSet::new(),
        }
    }
    
    /// Register a dependency relationship
    pub fn register_dependency(&mut self, from: String, to: String) {
        self.dependents
            .entry(to)
            .or_insert_with(HashSet::new)
            .insert(from);
    }
    
    /// Mark a file as having recent breakage
    pub fn mark_breakage(&mut self, file: String) {
        self.breakage_files.insert(file);
    }
    
    /// Clear breakage marker
    pub fn clear_breakage(&mut self, file: &str) {
        self.breakage_files.remove(file);
    }
    
    /// Record a recent change
    pub fn record_change(&mut self, file: String) {
        *self.recent_changes.entry(file).or_insert(0) += 1;
    }
    
    /// Add a known issue for a file
    pub fn add_known_issue(&mut self, file: String, issue: String) {
        self.known_issues
            .entry(file)
            .or_insert_with(Vec::new)
            .push(issue);
    }
    
    /// Mark a file as core infrastructure
    pub fn mark_core(&mut self, file: String) {
        self.core_files.insert(file);
    }
    
    /// Assess risk for a file
    pub fn assess(&self, file_path: &str) -> RiskAssessment {
        let mut risk_score = 0u32;
        let mut risk_factors = Vec::new();
        let mut recommendations = Vec::new();
        
        // Factor 1: Dependency count
        let dependent_count = self.dependents
            .get(file_path)
            .map(|d| d.len())
            .unwrap_or(0);
        
        if dependent_count > 20 {
            risk_score += 30;
            risk_factors.push(RiskFactor {
                factor_type: RiskFactorType::HighDependencyCount,
                description: format!("{} files depend on this file", dependent_count),
                weight: 30,
            });
            recommendations.push("Consider running full test suite after changes".to_string());
        } else if dependent_count > 10 {
            risk_score += 15;
            risk_factors.push(RiskFactor {
                factor_type: RiskFactorType::HighDependencyCount,
                description: format!("{} files depend on this file", dependent_count),
                weight: 15,
            });
        }
        
        // Factor 2: Recent breakage
        let has_recent_breakage = self.breakage_files.contains(file_path);
        if has_recent_breakage {
            risk_score += 25;
            risk_factors.push(RiskFactor {
                factor_type: RiskFactorType::RecentBreakage,
                description: "This file has caused recent failures".to_string(),
                weight: 25,
            });
            recommendations.push("Review recent failures before modifying".to_string());
        }
        
        // Factor 3: Hot code (frequent changes)
        let recent_change_count = self.recent_changes.get(file_path).copied().unwrap_or(0);
        if recent_change_count > 10 {
            risk_score += 15;
            risk_factors.push(RiskFactor {
                factor_type: RiskFactorType::HotCode,
                description: format!("{} recent changes", recent_change_count),
                weight: 15,
            });
        }
        
        // Factor 4: Core infrastructure
        if self.core_files.contains(file_path) {
            risk_score += 20;
            risk_factors.push(RiskFactor {
                factor_type: RiskFactorType::CoreInfrastructure,
                description: "Core infrastructure file".to_string(),
                weight: 20,
            });
            recommendations.push("Extra caution required for core files".to_string());
        }
        
        // Factor 5: Known issues
        if let Some(issues) = self.known_issues.get(file_path) {
            if !issues.is_empty() {
                risk_score += 10;
                risk_factors.push(RiskFactor {
                    factor_type: RiskFactorType::KnownIssues,
                    description: format!("{} known issues", issues.len()),
                    weight: 10,
                });
            }
        }
        
        // Calculate stability score (inverse of risk)
        let stability_score = 100u32.saturating_sub(risk_score);
        
        // Determine risk level
        let risk_level = match risk_score {
            0..=20 => RiskLevel::Low,
            21..=40 => RiskLevel::Medium,
            41..=70 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };
        
        // Add general recommendations based on risk level
        match risk_level {
            RiskLevel::High | RiskLevel::Critical => {
                recommendations.push("Consider pair programming for this change".to_string());
                recommendations.push("Create a feature branch for isolation".to_string());
            }
            _ => {}
        }
        
        RiskAssessment {
            file_path: file_path.to_string(),
            risk_level,
            risk_score,
            dependent_count,
            has_recent_breakage,
            recent_change_count,
            stability_score,
            risk_factors,
            recommendations,
        }
    }
    
    /// Get all high-risk files
    pub fn get_high_risk_files(&self) -> Vec<&String> {
        self.dependents.keys()
            .filter(|file| {
                let assessment = self.assess(*file);
                matches!(assessment.risk_level, RiskLevel::High | RiskLevel::Critical)
            })
            .collect()
    }
    
    /// Clear recent change tracking (call periodically)
    pub fn clear_recent_changes(&mut self) {
        self.recent_changes.clear();
    }
    
    /// Clear breakage markers (call after fixes)
    pub fn clear_all_breakages(&mut self) {
        self.breakage_files.clear();
    }
}

impl Default for RiskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
