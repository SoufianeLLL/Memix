pub mod adaptive;

pub use adaptive::{AdaptiveLearner, FilterMetrics, FilterImprovement};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PromptOutcome {
    SuccessFirstTry,
    NeededClarification(u32),
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptContextSection {
    pub section_name: String,
    pub tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRecord {
    pub task_type: String,
    pub model: String,
    pub context_sections: Vec<PromptContextSection>,
    pub total_tokens: usize,
    pub outcome: PromptOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptOptimizationSuggestion {
    pub task_type: String,
    pub always_include: Vec<String>,
    pub consider_excluding: Vec<String>,
    pub recommended_budget: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskModelPerformance {
    pub first_try_rate: f32,
    pub avg_tokens: usize,
    pub runs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPerformanceReport {
    pub model_performance: HashMap<String, HashMap<String, TaskModelPerformance>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeveloperProfile {
    pub universal_patterns: Vec<String>,
    pub preferred_stack: Vec<String>,
    pub code_style: Vec<String>,
}

pub struct PromptOptimizer;

impl PromptOptimizer {
    pub fn to_memory_entry(project_id: &str, record: &PromptRecord) -> MemoryEntry {
        let now = Utc::now();
        MemoryEntry {
            id: format!("prompt_record_{}_{}.json", now.timestamp_millis(), uuid::Uuid::new_v4()),
            project_id: project_id.to_string(),
            kind: MemoryKind::Context,
            content: serde_json::to_string_pretty(record).unwrap_or_else(|_| "{}".to_string()),
            tags: vec!["learning".to_string(), "prompt-record".to_string()],
            source: MemorySource::AgentExtracted,
            superseded_by: None,
            contradicts: vec![],
            parent_id: None,
            caused_by: vec![],
            enables: vec![],
            created_at: now,
            updated_at: now,
            access_count: 0,
            last_accessed_at: None,
        }
    }

    pub fn records_from_entries(entries: &[MemoryEntry]) -> Vec<PromptRecord> {
        entries
            .iter()
            .filter(|entry| entry.tags.iter().any(|tag| tag == "prompt-record"))
            .filter_map(|entry| serde_json::from_str::<PromptRecord>(&entry.content).ok())
            .collect()
    }

    pub fn suggest_context(task_type: &str, records: &[PromptRecord]) -> PromptOptimizationSuggestion {
        let relevant = records
            .iter()
            .filter(|record| record.task_type.eq_ignore_ascii_case(task_type))
            .collect::<Vec<_>>();
        let mut successful_frequency: HashMap<String, usize> = HashMap::new();
        let mut failure_frequency: HashMap<String, usize> = HashMap::new();
        let mut successful_runs = 0usize;
        let mut budgets = Vec::new();

        for record in &relevant {
            match record.outcome {
                PromptOutcome::SuccessFirstTry => {
                    successful_runs += 1;
                    budgets.push(record.total_tokens);
                    for section in &record.context_sections {
                        *successful_frequency.entry(section.section_name.clone()).or_default() += 1;
                    }
                }
                PromptOutcome::NeededClarification(_) | PromptOutcome::Failed => {
                    for section in &record.context_sections {
                        *failure_frequency.entry(section.section_name.clone()).or_default() += 1;
                    }
                }
            }
        }

        let always_include = successful_frequency
            .iter()
            .filter(|(_, count)| **count == successful_runs && successful_runs > 0)
            .map(|(section, _)| section.clone())
            .collect::<Vec<_>>();
        let consider_excluding = failure_frequency
            .iter()
            .filter(|(section, count)| !successful_frequency.contains_key(*section) && **count > 0)
            .map(|(section, _)| section.clone())
            .collect::<Vec<_>>();
        let recommended_budget = if budgets.is_empty() {
            0
        } else {
            budgets.iter().sum::<usize>() / budgets.len()
        };

        PromptOptimizationSuggestion {
            task_type: task_type.to_string(),
            always_include,
            consider_excluding,
            recommended_budget,
        }
    }

    pub fn model_performance(records: &[PromptRecord]) -> ModelPerformanceReport {
        let mut grouped: HashMap<String, HashMap<String, Vec<&PromptRecord>>> = HashMap::new();
        for record in records {
            grouped
                .entry(record.model.clone())
                .or_default()
                .entry(record.task_type.clone())
                .or_default()
                .push(record);
        }
        let mut report = HashMap::new();
        for (model, tasks) in grouped {
            let mut task_report = HashMap::new();
            for (task, items) in tasks {
                let runs = items.len();
                let successes = items
                    .iter()
                    .filter(|record| matches!(record.outcome, PromptOutcome::SuccessFirstTry))
                    .count();
                let avg_tokens = if runs == 0 {
                    0
                } else {
                    items.iter().map(|record| record.total_tokens).sum::<usize>() / runs
                };
                task_report.insert(
                    task,
                    TaskModelPerformance {
                        first_try_rate: if runs == 0 { 0.0 } else { successes as f32 / runs as f32 },
                        avg_tokens,
                        runs,
                    },
                );
            }
            report.insert(model, task_report);
        }
        ModelPerformanceReport {
            model_performance: report,
        }
    }
}

pub struct CrossProjectLearner;

impl CrossProjectLearner {
    pub fn compute_developer_profile(entries_by_project: &HashMap<String, Vec<MemoryEntry>>) -> DeveloperProfile {
        let total_projects = entries_by_project.len().max(1) as f32;
        let mut pattern_frequency: HashMap<String, usize> = HashMap::new();
        let mut tech_frequency: HashMap<String, usize> = HashMap::new();
        let mut code_style_frequency: HashMap<String, usize> = HashMap::new();

        for entries in entries_by_project.values() {
            let mut seen_patterns = HashSet::new();
            let mut seen_tech = HashSet::new();
            let mut seen_style = HashSet::new();
            for entry in entries {
                if entry.id == "patterns.json" {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&entry.content) {
                        collect_strings(value.get("preferences"), &mut seen_patterns);
                        collect_strings(value.get("code_style"), &mut seen_style);
                    }
                }
                if entry.id == "identity.json" {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&entry.content) {
                        collect_strings(value.get("tech_stack"), &mut seen_tech);
                    }
                }
            }
            for item in seen_patterns {
                *pattern_frequency.entry(item).or_default() += 1;
            }
            for item in seen_tech {
                *tech_frequency.entry(item).or_default() += 1;
            }
            for item in seen_style {
                *code_style_frequency.entry(item).or_default() += 1;
            }
        }

        DeveloperProfile {
            universal_patterns: pattern_frequency
                .into_iter()
                .filter(|(_, count)| (*count as f32 / total_projects) >= 0.6)
                .map(|(pattern, count)| format!("{} (used in {}/{} projects)", pattern, count, total_projects as usize))
                .collect(),
            preferred_stack: tech_frequency
                .into_iter()
                .filter(|(_, count)| (*count as f32 / total_projects) >= 0.5)
                .map(|(tech, _)| tech)
                .collect(),
            code_style: code_style_frequency
                .into_iter()
                .filter(|(_, count)| (*count as f32 / total_projects) >= 0.5)
                .map(|(style, _)| style)
                .collect(),
        }
    }
}

fn collect_strings(value: Option<&serde_json::Value>, out: &mut HashSet<String>) {
    let Some(serde_json::Value::Array(items)) = value else {
        return;
    };
    for item in items {
        if let Some(string) = item.as_str() {
            out.insert(string.to_string());
        }
    }
}
