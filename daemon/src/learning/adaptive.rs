//! Adaptive learning module for filter optimization.
//!
//! Learns from command execution patterns to improve filter configurations.
//! Tracks which filters are most effective and suggests improvements.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Learning sample from a command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSample {
    /// Command that was executed
    pub command: String,
    /// Filter that was applied (if any)
    pub filter_name: Option<String>,
    /// Raw token count
    pub raw_tokens: usize,
    /// Filtered token count
    pub filtered_tokens: usize,
    /// Whether the output was useful (user feedback)
    pub was_useful: Option<bool>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Filter performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterMetrics {
    /// Filter name
    pub name: String,
    /// Total applications
    pub total_applications: usize,
    /// Average savings percentage
    pub avg_savings_pct: f64,
    /// Total tokens saved
    pub total_tokens_saved: usize,
    /// User satisfaction rate (was_useful = true)
    pub satisfaction_rate: f64,
    /// Last used
    pub last_used: DateTime<Utc>,
}

/// Adaptive learning engine
pub struct AdaptiveLearner {
    /// Filter performance history
    filter_history: HashMap<String, Vec<LearningSample>>,
    /// Aggregated metrics
    metrics: HashMap<String, FilterMetrics>,
}

impl AdaptiveLearner {
    pub fn new() -> Self {
        Self {
            filter_history: HashMap::new(),
            metrics: HashMap::new(),
        }
    }
    
    /// Record a learning sample
    pub fn record(&mut self, sample: LearningSample) {
        if let Some(filter_name) = sample.filter_name.clone() {
            let history = self.filter_history
                .entry(filter_name.clone())
                .or_insert_with(Vec::new);
            
            history.push(sample);
            
            // Update metrics
            self.update_metrics(&filter_name);
        }
    }
    
    /// Update metrics for a filter
    fn update_metrics(&mut self, filter_name: &str) {
        let history = match self.filter_history.get(filter_name) {
            Some(h) => h,
            None => return,
        };
        
        if history.is_empty() {
            return;
        }
        
        let total_applications = history.len();
        let total_raw: usize = history.iter().map(|s| s.raw_tokens).sum();
        let total_filtered: usize = history.iter().map(|s| s.filtered_tokens).sum();
        let total_tokens_saved = total_raw.saturating_sub(total_filtered);
        
        let avg_savings_pct = if total_raw > 0 {
            (total_tokens_saved as f64 / total_raw as f64) * 100.0
        } else {
            0.0
        };
        
        let useful_count = history.iter()
            .filter(|s| s.was_useful == Some(true))
            .count();
        let total_feedback = history.iter()
            .filter(|s| s.was_useful.is_some())
            .count();
        
        let satisfaction_rate = if total_feedback > 0 {
            useful_count as f64 / total_feedback as f64
        } else {
            1.0 // Assume good if no feedback
        };
        
        let last_used = history.iter()
            .map(|s| s.timestamp)
            .max()
            .unwrap_or_else(Utc::now);
        
        self.metrics.insert(filter_name.to_string(), FilterMetrics {
            name: filter_name.to_string(),
            total_applications,
            avg_savings_pct,
            total_tokens_saved,
            satisfaction_rate,
            last_used,
        });
    }
    
    /// Get metrics for a filter
    pub fn get_metrics(&self, filter_name: &str) -> Option<&FilterMetrics> {
        self.metrics.get(filter_name)
    }
    
    /// Get all filter metrics
    pub fn get_all_metrics(&self) -> Vec<&FilterMetrics> {
        self.metrics.values().collect()
    }
    
    /// Get top performing filters
    pub fn get_top_filters(&self, limit: usize) -> Vec<&FilterMetrics> {
        let mut all: Vec<_> = self.metrics.values().collect();
        all.sort_by(|a, b| {
            // Sort by total tokens saved, then by satisfaction rate
            b.total_tokens_saved
                .cmp(&a.total_tokens_saved)
                .then_with(|| {
                    b.satisfaction_rate
                        .partial_cmp(&a.satisfaction_rate)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        all.into_iter().take(limit).collect()
    }
    
    /// Get underperforming filters (low satisfaction or low savings)
    pub fn get_underperforming_filters(&self) -> Vec<&FilterMetrics> {
        self.metrics.values()
            .filter(|m| {
                m.satisfaction_rate < 0.7 || m.avg_savings_pct < 20.0
            })
            .collect()
    }
    
    /// Suggest filter improvements
    pub fn suggest_improvements(&self) -> Vec<FilterImprovement> {
        let mut suggestions = Vec::new();
        
        for metrics in self.metrics.values() {
            // Low satisfaction - filter may be too aggressive
            if metrics.satisfaction_rate < 0.7 && metrics.total_applications >= 5 {
                suggestions.push(FilterImprovement {
                    filter_name: metrics.name.clone(),
                    issue: FilterIssue::LowSatisfaction(metrics.satisfaction_rate),
                    suggestion: "Consider reducing filter aggressiveness or adding more context preservation".to_string(),
                    priority: ImprovementPriority::High,
                });
            }
            
            // Low savings - filter may not be effective
            if metrics.avg_savings_pct < 20.0 && metrics.total_applications >= 5 {
                suggestions.push(FilterImprovement {
                    filter_name: metrics.name.clone(),
                    issue: FilterIssue::LowSavings(metrics.avg_savings_pct),
                    suggestion: "Consider adding more strip patterns or increasing max_lines reduction".to_string(),
                    priority: ImprovementPriority::Medium,
                });
            }
            
            // Unused filter
            let days_since_use = (Utc::now() - metrics.last_used).num_days();
            if days_since_use > 30 {
                suggestions.push(FilterImprovement {
                    filter_name: metrics.name.clone(),
                    issue: FilterIssue::Unused(days_since_use),
                    suggestion: "Consider removing this filter or updating its pattern".to_string(),
                    priority: ImprovementPriority::Low,
                });
            }
        }
        
        // Sort by priority
        suggestions.sort_by(|a, b| {
            use ImprovementPriority::*;
            match (a.priority, b.priority) {
                (High, High) => std::cmp::Ordering::Equal,
                (High, _) => std::cmp::Ordering::Less,
                (_, High) => std::cmp::Ordering::Greater,
                (Medium, Medium) => std::cmp::Ordering::Equal,
                (Medium, _) => std::cmp::Ordering::Less,
                (_, Medium) => std::cmp::Ordering::Greater,
                (Low, Low) => std::cmp::Ordering::Equal,
            }
        });
        
        suggestions
    }
}

impl Default for AdaptiveLearner {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter improvement suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterImprovement {
    /// Filter name
    pub filter_name: String,
    /// Issue detected
    pub issue: FilterIssue,
    /// Suggested action
    pub suggestion: String,
    /// Priority level
    pub priority: ImprovementPriority,
}

/// Issue type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterIssue {
    /// Users marked output as not useful
    LowSatisfaction(f64),
    /// Filter not saving enough tokens
    LowSavings(f64),
    /// Filter not used recently
    Unused(i64),
}

/// Improvement priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImprovementPriority {
    High,
    Medium,
    Low,
}
