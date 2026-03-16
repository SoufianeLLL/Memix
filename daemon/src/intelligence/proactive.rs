use serde::{Deserialize, Serialize};

use crate::brain::schema::{MemoryEntry, MemoryKind};
use crate::git::archaeologist::ProjectGitInsights;
use crate::observer::dna::ProjectCodeDna;
use crate::observer::graph::DependencyGraph;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskWarning {
    pub file: String,
    pub risk_score: f32,
    pub dependents: usize,
    pub past_breaks: Vec<String>,
    pub known_issues: Vec<String>,
    pub touches_stable_area: bool,
    pub recommendation: String,
}

pub struct ProactiveWarner;

impl ProactiveWarner {
    pub fn assess_risk(
        file: &str,
        project_entries: &[MemoryEntry],
        dependency_graph: &DependencyGraph,
        code_dna: &ProjectCodeDna,
        git_insights: &ProjectGitInsights,
    ) -> Option<RiskWarning> {
        let dependents = dependency_graph
            .edges_in
            .get(file)
            .map(|deps| deps.len())
            .unwrap_or(0);
        let normalized_file = file.to_lowercase();
        let mut known_issues = Vec::new();
        let mut past_breaks = Vec::new();

        for entry in project_entries {
            let content_lower = entry.content.to_lowercase();
            if !content_lower.contains(&normalized_file) {
                continue;
            }
            if matches!(entry.kind, MemoryKind::Warning) {
                if entry.tags.iter().any(|tag| tag.contains("signature-change") || tag.contains("test-risk")) {
                    past_breaks.push(entry.content.clone());
                }
                if entry.tags.iter().any(|tag| tag.contains("warning") || tag.contains("issue")) {
                    known_issues.push(entry.content.clone());
                }
            }
            if matches!(entry.kind, MemoryKind::Context | MemoryKind::Fact) && entry.tags.iter().any(|tag| tag.contains("issue")) {
                known_issues.push(entry.content.clone());
            }
        }

        let touches_stable_area = code_dna.stable_zones.iter().any(|zone| zone == file)
            || git_insights.stable_files.iter().any(|stable| stable.file_path == file);
        let hot_signal = code_dna.hot_zones.iter().any(|zone| zone == file)
            || git_insights.hot_files.iter().any(|hot| hot.file_path == file);

        let mut risk_score = 0.0f32;
        risk_score += (dependents.min(25) as f32 / 25.0) * 0.35;
        risk_score += (past_breaks.len().min(5) as f32 / 5.0) * 0.30;
        risk_score += (known_issues.len().min(5) as f32 / 5.0) * 0.20;
        if touches_stable_area {
            risk_score += 0.10;
        }
        if hot_signal {
            risk_score += 0.10;
        }
        risk_score = risk_score.min(1.0);

        if risk_score < 0.35 {
            return None;
        }

        Some(RiskWarning {
            file: file.to_string(),
            risk_score,
            dependents,
            past_breaks: truncate_items(past_breaks),
            known_issues: truncate_items(known_issues),
            touches_stable_area,
            recommendation: if risk_score >= 0.75 {
                "Create checkpoint before modifying".to_string()
            } else if risk_score >= 0.55 {
                "Review dependents and known issues before editing".to_string()
            } else {
                "Proceed with caution".to_string()
            },
        })
    }
}

fn truncate_items(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items.dedup();
    items.truncate(5);
    items
}
