use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use chrono::{DateTime, Utc};
use crate::observer::graph::DependencyGraph;
use crate::observer::differ::SemanticDiff;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub file: String,
    pub change_type: ChangeType,
    pub severity: ImpactSeverity,
    pub impacted_files: Vec<ImpactedFile>,
    pub recommendations: Vec<String>,
    pub risk_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    FunctionAdded,
    FunctionRemoved,
    FunctionModified,
    TypeChanged,
    ImportAdded,
    ImportRemoved,
    ExportChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImpactSeverity {
    Critical,
    High,
    Medium,
    Low,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactedFile {
    pub path: String,
    pub line: Option<u32>,
    pub reason: String,
    pub urgency: ImpactSeverity,
}

pub struct AutonomousPairProgrammer {
    pub dependency_graph: DependencyGraph,
    pub change_history: VecDeque<CodeChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChange {
    pub file: String,
    pub timestamp: DateTime<Utc>,
    pub diff: SemanticDiff,
}

impl AutonomousPairProgrammer {
    pub fn new() -> Self {
        Self {
            dependency_graph: DependencyGraph::new(),
            change_history: VecDeque::with_capacity(100),
        }
    }

    pub fn update_dependency_graph(&mut self, file: &str, imports: &[String]) {
        self.dependency_graph.set_dependencies(file, imports);
    }

    pub fn analyze_impact(&self, file: &str, diff: &SemanticDiff) -> ImpactAnalysis {
        let mut impacted_files = Vec::new();
        let mut recommendations = Vec::new();
        let mut risk_score: f32 = 0.0;
        let mut seen_impacted_files = HashSet::new();

        // Get files that depend on this file
        let dependents = self.dependency_graph.edges_in
            .get(file)
            .cloned()
            .unwrap_or_default();

        if dependents.is_empty() && diff.nodes_added.is_empty() && diff.nodes_removed.is_empty() {
            return ImpactAnalysis {
                file: file.to_string(),
                change_type: ChangeType::FunctionModified,
                severity: ImpactSeverity::None,
                impacted_files: vec![],
                recommendations: vec!["No impact detected - this file is not imported by others".to_string()],
                risk_score: 0.0,
            };
        }

        // Analyze function removals - highest risk
        for removed in &diff.nodes_removed {
            risk_score += 0.5;
            for dependent in &dependents {
                if seen_impacted_files.insert(dependent.clone()) {
                    impacted_files.push(ImpactedFile {
                        path: dependent.clone(),
                        line: None,
                        reason: format!("Function '{}' was removed - may cause compile errors", removed.name),
                        urgency: ImpactSeverity::Critical,
                    });
                }
            }
            recommendations.push(format!(
                "⚠️ CRITICAL: Function '{}' removed. {} dependent files will break.",
                removed.name,
                dependents.len()
            ));
        }

        // Analyze function modifications
        for modified in &diff.nodes_modified {
            risk_score += 0.3;
            for dependent in &dependents {
                if seen_impacted_files.insert(dependent.clone()) {
                    impacted_files.push(ImpactedFile {
                        path: dependent.clone(),
                        line: None,
                        reason: format!("Function '{}' signature changed - call sites may be incompatible", modified.name),
                        urgency: ImpactSeverity::High,
                    });
                }
            }
            recommendations.push(format!(
                "🔴 HIGH RISK: Function '{}' modified. {} files may need updates.",
                modified.name,
                dependents.len()
            ));
        }

        // Analyze new imports
        if !diff.nodes_added.is_empty() {
            risk_score += 0.1;
            recommendations.push(format!(
                "✅ Added {} new functions - review for potential use",
                diff.nodes_added.len()
            ));
        }

        let severity = match risk_score {
            s if s >= 1.0 => ImpactSeverity::Critical,
            s if s >= 0.7 => ImpactSeverity::High,
            s if s >= 0.4 => ImpactSeverity::Medium,
            s if s > 0.0 => ImpactSeverity::Low,
            _ => ImpactSeverity::None,
        };

        ImpactAnalysis {
            file: file.to_string(),
            change_type: if !diff.nodes_removed.is_empty() {
                ChangeType::FunctionRemoved
            } else if !diff.nodes_added.is_empty() {
                ChangeType::FunctionAdded
            } else {
                ChangeType::FunctionModified
            },
            severity,
            impacted_files,
            recommendations,
            risk_score: risk_score.min(1.0),
        }
    }

    pub fn record_change(&mut self, file: String, diff: SemanticDiff) {
        self.change_history.push_back(CodeChange {
            file,
            timestamp: Utc::now(),
            diff,
        });

        // Keep only last 100 changes — O(1) eviction
        while self.change_history.len() > 100 {
            self.change_history.pop_front();
        }
    }

    pub fn detect_conflicts(&self) -> Vec<ConflictReport> {
        let mut conflicts = Vec::new();
        
        // Simple conflict detection: if a function was modified in multiple places
        let mut function_changes: HashMap<String, Vec<&CodeChange>> = HashMap::new();
        
        for change in &self.change_history {
            for modified in &change.diff.nodes_modified {
                function_changes
                    .entry(modified.name.clone())
                    .or_default()
                    .push(&change);
            }
        }

        for (func_name, changes) in function_changes {
            if changes.len() > 1 {
                let files: Vec<String> = changes.iter().map(|c| c.file.clone()).collect();
                let unique_files: HashSet<&String> = files.iter().collect();
                if unique_files.len() == 1 {
                    continue; // Same file
                }
                conflicts.push(ConflictReport {
                    conflict_type: "simultaneous_modification".to_string(),
                    description: format!(
                        "Function '{}' modified in {} files: {}",
                        func_name,
                        files.len(),
                        files.join(", ")
                    ),
                    files,
                    severity: "high".to_string(),
                });
            }
        }

        conflicts
    }

    pub fn predict_questions(&self, file: &str) -> Vec<PredictedQuestion> {
        let mut questions = Vec::new();
        
        // Get dependents
        let dependents = self.dependency_graph.edges_in
            .get(file)
            .cloned()
            .unwrap_or_default();

        if !dependents.is_empty() {
            questions.push(PredictedQuestion {
                question: format!(
                    "You're about to modify '{}'. It is used by {} other files. Want me to show impact analysis?",
                    file,
                    dependents.len()
                ),
                context: "dependency_warning".to_string(),
                priority: if dependents.len() > 5 { "high" } else { "medium" }.to_string(),
            });
        }

        // Check recent changes for context
        let recent_changes = self.change_history.iter()
            .filter(|c| c.file == file)
            .take(3)
            .collect::<Vec<_>>();

        if !recent_changes.is_empty() {
            questions.push(PredictedQuestion {
                question: "This file was recently modified. Would you like context on recent changes?".to_string(),
                context: "recent_changes".to_string(),
                priority: "low".to_string(),
            });
        }

        questions
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictReport {
    pub conflict_type: String,
    pub description: String,
    pub files: Vec<String>,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedQuestion {
    pub question: String,
    pub context: String,
    pub priority: String,
}

impl Default for AutonomousPairProgrammer {
    fn default() -> Self {
        Self::new()
    }
}
