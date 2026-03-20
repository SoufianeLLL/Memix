use crate::observer::differ::SemanticDiff;

#[derive(Debug, Clone, PartialEq)]
pub enum DeveloperIntent {
    Scaffolding,
    Refactoring,
    BugFixing,
    ApiDesign,
    Testing,
    Configuration,
    Exploration,
}

pub struct IntentEngine;

impl DeveloperIntent {
	pub fn as_str(&self) -> &'static str {
		match self {
			DeveloperIntent::Scaffolding => "scaffolding",
			DeveloperIntent::Refactoring => "refactoring",
			DeveloperIntent::BugFixing => "bug_fixing",
			DeveloperIntent::ApiDesign => "api_design",
			DeveloperIntent::Testing => "testing",
			DeveloperIntent::Configuration => "configuration",
			DeveloperIntent::Exploration => "exploration",
		}
	}
}

/// Weighted signal scores for multi-factor intent classification.
/// Each signal contributes a (intent, weight) vote; the highest total wins.
struct IntentSignals {
    votes: Vec<(DeveloperIntent, f32)>,
}

impl IntentSignals {
    fn new() -> Self {
        Self { votes: Vec::with_capacity(8) }
    }

    fn vote(&mut self, intent: DeveloperIntent, weight: f32) {
        self.votes.push((intent, weight));
    }

    fn resolve(self) -> (DeveloperIntent, f32) {
        use std::collections::HashMap;
        let mut totals: HashMap<String, (DeveloperIntent, f32)> = HashMap::new();
        for (intent, weight) in self.votes {
            let key = intent.as_str().to_string();
            totals
                .entry(key)
                .and_modify(|entry| entry.1 += weight)
                .or_insert((intent, weight));
        }
        totals
            .into_values()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((DeveloperIntent::Exploration, 0.25))
    }
}

impl IntentEngine {
    /// Multi-factor intent classification over semantic mutations, file-path signals,
    /// pattern-tag prevalence, and structural edit ratios.
    ///
    /// Returns (intent, confidence) where confidence is calibrated 0.25–0.95.
    pub fn classify_intent(diff: &SemanticDiff) -> DeveloperIntent {
        let (intent, _) = Self::classify_with_confidence(diff);
        intent
    }

    pub fn classify_with_confidence(diff: &SemanticDiff) -> (DeveloperIntent, f32) {
        let added = diff.nodes_added.len();
        let removed = diff.nodes_removed.len();
        let modified = diff.nodes_modified.len();
        let total = added + removed + modified;

        let mut signals = IntentSignals::new();

        // === Signal 1: File path heuristics (0.30 weight) ===
        let path_lower = diff.file.to_lowercase();
        if path_lower.contains("/api/") || path_lower.contains("route.") || path_lower.contains("/routes/")
            || path_lower.contains("handler") || path_lower.contains("endpoint") {
            signals.vote(DeveloperIntent::ApiDesign, 0.30);
        }
        if path_lower.contains("/test") || path_lower.contains(".test.") || path_lower.contains(".spec.")
            || path_lower.contains("_test.") || path_lower.contains("/spec/") {
            signals.vote(DeveloperIntent::Testing, 0.35);
        }
        if path_lower.contains("config") || path_lower.contains(".env")
            || path_lower.contains("settings") || path_lower.contains("toml")
            || path_lower.contains("yaml") || path_lower.contains(".yml") {
            signals.vote(DeveloperIntent::Configuration, 0.30);
        }
        if path_lower.contains("migration") || path_lower.contains("seed") {
            signals.vote(DeveloperIntent::Scaffolding, 0.25);
        }

        // === Signal 2: Structural edit ratios (0.35 weight — heaviest signal) ===
        if total > 0 {
            let add_ratio = added as f32 / total as f32;
            let remove_ratio = removed as f32 / total as f32;
            let mod_ratio = modified as f32 / total as f32;

            // Pure additions with zero removals/modifications = scaffolding
            if added > 0 && removed == 0 && modified == 0 {
                signals.vote(DeveloperIntent::Scaffolding, 0.35);
            }
            // High removes coupled with adds = refactoring (structural reshaping)
            if removed > 0 && modified > 0 && add_ratio <= remove_ratio {
                signals.vote(DeveloperIntent::Refactoring, 0.35);
            }
            // Dominated by modifications = bug fixing (logic patching)
            if mod_ratio > add_ratio && mod_ratio > remove_ratio && modified > 0 {
                signals.vote(DeveloperIntent::BugFixing, 0.35);
            }
            // Large batch adds with some modifications = feature scaffolding
            if add_ratio > 0.6 && added >= 3 {
                signals.vote(DeveloperIntent::Scaffolding, 0.25);
            }
        }

        // === Signal 3: Pattern tag prevalence (0.20 weight) ===
        let all_features = diff.nodes_added.iter()
            .chain(diff.nodes_modified.iter())
            .chain(diff.nodes_removed.iter());

        let mut pattern_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for feature in all_features {
            for tag in &feature.pattern_tags {
                *pattern_counts.entry(tag.as_str()).or_default() += 1;
            }
        }

        if pattern_counts.contains_key("tests") {
            signals.vote(DeveloperIntent::Testing, 0.20);
        }
        if pattern_counts.contains_key("repository") || pattern_counts.contains_key("service") {
            signals.vote(DeveloperIntent::Scaffolding, 0.15);
        }
        if pattern_counts.contains_key("edge-guards") || pattern_counts.contains_key("adapter") {
            signals.vote(DeveloperIntent::ApiDesign, 0.15);
        }

        // === Signal 4: Change volume heuristic (0.15 weight) ===
        // Very small edits (1-2 nodes, all modifications) = bug fixing
        if total <= 2 && modified > 0 && added == 0 && removed == 0 {
            signals.vote(DeveloperIntent::BugFixing, 0.15);
        }
        // Large-scale removals with zero additions = cleanup/refactor
        if removed >= 3 && added == 0 {
            signals.vote(DeveloperIntent::Refactoring, 0.15);
        }

        // Resolve: highest weighted vote wins
        let (intent, raw_confidence) = signals.resolve();

        // If no signal fired at all, fall back to exploration
        if total == 0 {
            return (DeveloperIntent::Exploration, 0.25);
        }

        (intent, raw_confidence.clamp(0.25, 0.95))
    }

    /// Calibrated confidence score for the current classification.
    pub fn confidence(diff: &SemanticDiff) -> f32 {
        let (_, confidence) = Self::classify_with_confidence(diff);
        confidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer::parser::AstNodeFeature;

    fn make_diff(file: &str, added: usize, removed: usize, modified: usize) -> SemanticDiff {
        let make_features = |count: usize| -> Vec<AstNodeFeature> {
            (0..count).map(|i| AstNodeFeature {
                name: format!("fn_{}", i),
                kind: "function".to_string(),
                body: String::new(),
                start_byte: i * 100,
                end_byte: (i + 1) * 100,
                language: "rust".to_string(),
                cyclomatic_complexity: 1,
                pattern_tags: vec![],
                is_exported: false,
                calls: vec![],
                line_count: None,
            }).collect()
        };
        SemanticDiff {
            file: file.to_string(),
            nodes_added: make_features(added),
            nodes_removed: make_features(removed),
            nodes_modified: make_features(modified),
        }
    }

    #[test]
    fn pure_additions_are_scaffolding() {
        let diff = make_diff("src/models/user.rs", 5, 0, 0);
        let (intent, _) = IntentEngine::classify_with_confidence(&diff);
        assert_eq!(intent, DeveloperIntent::Scaffolding);
    }

    #[test]
    fn api_route_file_is_api_design() {
        let diff = make_diff("src/api/users/route.ts", 2, 0, 1);
        let (intent, _) = IntentEngine::classify_with_confidence(&diff);
        assert_eq!(intent, DeveloperIntent::ApiDesign);
    }

    #[test]
    fn test_file_is_testing() {
        let diff = make_diff("src/__tests__/auth.test.ts", 3, 0, 1);
        let (intent, _) = IntentEngine::classify_with_confidence(&diff);
        assert_eq!(intent, DeveloperIntent::Testing);
    }

    #[test]
    fn small_modifications_are_bug_fixing() {
        let diff = make_diff("src/utils/format.ts", 0, 0, 2);
        let (intent, _) = IntentEngine::classify_with_confidence(&diff);
        assert_eq!(intent, DeveloperIntent::BugFixing);
    }

    #[test]
    fn removes_with_modifications_are_refactoring() {
        let diff = make_diff("src/core/engine.rs", 1, 4, 2);
        let (intent, _) = IntentEngine::classify_with_confidence(&diff);
        assert_eq!(intent, DeveloperIntent::Refactoring);
    }

    #[test]
    fn empty_diff_is_exploration() {
        let diff = make_diff("src/main.rs", 0, 0, 0);
        let (intent, confidence) = IntentEngine::classify_with_confidence(&diff);
        assert_eq!(intent, DeveloperIntent::Exploration);
        assert!((confidence - 0.25).abs() < 0.01);
    }
}
