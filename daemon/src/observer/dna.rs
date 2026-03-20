use config::{Config as ConfigLoader, File as ConfigFile, FileFormat};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::observer::graph::DependencyGraph;
use crate::observer::parser::AstNodeFeature;

/// A compact per-file fingerprint used to build the project-level DNA summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeDna {
	pub file_path: String,
	pub language: String,
	pub cyclomatic_complexity: u32,
	pub primary_exports: Vec<String>,
	pub semantic_dependencies: Vec<String>,
	pub architectural_categorization: String,
	pub pattern_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnaRuleConfig {
	#[serde(default)]
	pub file_category_rules: Vec<DnaCategoryRule>,
	#[serde(default)]
	pub pattern_rules: Vec<DnaPatternRule>,
	#[serde(default)]
	pub architecture_rules: Vec<DnaArchitectureRule>,
	#[serde(default)]
	pub error_handling_rules: Vec<DnaErrorHandlingRule>,
	#[serde(default)]
	pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnaCategoryRule {
	pub id: String,
	pub classify_as: String,
	#[serde(default)]
	pub languages: Vec<String>,
	#[serde(default)]
	pub path_patterns: Vec<String>,
	#[serde(default)]
	pub requires_any_patterns: Vec<String>,
	#[serde(default)]
	pub requires_all_patterns: Vec<String>,
	#[serde(default)]
	pub entity_kinds: Vec<String>,
	#[serde(default)]
	pub require_exported: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnaCountRequirement {
	pub value: String,
	pub min_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnaArchitectureRule {
	pub id: String,
	pub architecture: String,
	#[serde(default)]
	pub required_categories: Vec<DnaCountRequirement>,
	#[serde(default)]
	pub required_patterns: Vec<DnaCountRequirement>,
	#[serde(default)]
	pub min_dependency_depth: Option<usize>,
	#[serde(default)]
	pub require_dense_dependency_graph: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnaErrorHandlingRule {
	pub id: String,
	pub label: String,
	#[serde(default)]
	pub languages: Vec<String>,
	#[serde(default)]
	pub body_patterns: Vec<String>,
	#[serde(default)]
	pub min_matches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnaPatternRule {
	pub id: String,
	pub tag: String,
	#[serde(default)]
	pub languages: Vec<String>,
	#[serde(default)]
	pub path_patterns: Vec<String>,
	#[serde(default)]
	pub name_patterns: Vec<String>,
	#[serde(default)]
	pub entity_kinds: Vec<String>,
	#[serde(default)]
	pub require_exported: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OTelAttribute {
	pub key: String,
	pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OTelEvent {
	pub name: String,
	pub attributes: Vec<OTelAttribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObserverDnaOtelExport {
	pub schema_url: String,
	pub resource_attributes: Vec<OTelAttribute>,
	pub events: Vec<OTelEvent>,
}

/// Project-level fingerprint that can be injected as a compact architectural briefing.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectCodeDna {
	pub indexed_files: usize,
	pub functions_indexed: usize,
	pub architecture: String,
	pub complexity_score: f32,
	pub dominant_patterns: Vec<String>,
	pub hot_zones: Vec<String>,
	pub stable_zones: Vec<String>,
	pub dependency_depth: usize,
	pub circular_risks: Vec<String>,
	pub type_coverage: f32,
	pub error_handling: String,
	pub test_coverage_estimate: f32,
	pub active_development_areas: Vec<String>,
	pub stale_areas: Vec<String>,
	pub explainability_summary: String,
	pub language_breakdown: HashMap<String, usize>,
	pub rules_source: Option<String>,
	pub applied_rule_ids: Vec<String>,
}

impl DnaRuleConfig {
	pub fn built_in() -> Self {
		Self {
			file_category_rules: vec![
				DnaCategoryRule {
					id: "builtin-api-surface".to_string(),
					classify_as: "api-surface".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(api)(/|$)".to_string(), r"route\.(ts|tsx|js|jsx|rs|py|go|java)$".to_string()],
					requires_any_patterns: vec![],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-api-surface-pattern".to_string(),
					classify_as: "api-surface".to_string(),
					languages: vec![],
					path_patterns: vec![],
					requires_any_patterns: vec!["api-route".to_string()],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-server-actions".to_string(),
					classify_as: "server-actions".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(actions)(/|$)".to_string()],
					requires_any_patterns: vec![],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-server-actions-pattern".to_string(),
					classify_as: "server-actions".to_string(),
					languages: vec![],
					path_patterns: vec![],
					requires_any_patterns: vec!["server-actions".to_string()],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-ui-components".to_string(),
					classify_as: "ui-components".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(components)(/|$)".to_string()],
					requires_any_patterns: vec![],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-ui-components-pattern".to_string(),
					classify_as: "ui-components".to_string(),
					languages: vec![],
					path_patterns: vec![],
					requires_any_patterns: vec!["component-driven".to_string(), "react-hook".to_string()],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-domain-services".to_string(),
					classify_as: "domain-services".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(services|repositories|adapters|facades)(/|$)".to_string()],
					requires_any_patterns: vec![],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-domain-services-pattern".to_string(),
					classify_as: "domain-services".to_string(),
					languages: vec![],
					path_patterns: vec![],
					requires_any_patterns: vec!["repository".to_string(), "service".to_string(), "adapter".to_string(), "facade".to_string()],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-tests".to_string(),
					classify_as: "tests".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(tests?|spec)(/|$)".to_string(), r"\.(test|spec)\.[^.]+$".to_string()],
					requires_any_patterns: vec![],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-tests-pattern".to_string(),
					classify_as: "tests".to_string(),
					languages: vec![],
					path_patterns: vec![],
					requires_any_patterns: vec!["tests".to_string()],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaCategoryRule {
					id: "builtin-shared-library".to_string(),
					classify_as: "shared-library".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(lib|shared)(/|$)".to_string()],
					requires_any_patterns: vec![],
					requires_all_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
			],
			pattern_rules: vec![
				DnaPatternRule {
					id: "builtin-api-route-pattern".to_string(),
					tag: "api-route".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(api)(/|$)".to_string(), r"route\.(ts|tsx|js|jsx|rs|py|go|java)$".to_string()],
					name_patterns: vec![r"^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)$".to_string()],
					entity_kinds: vec!["function".to_string()],
					require_exported: Some(true),
				},
				DnaPatternRule {
					id: "builtin-repository-pattern".to_string(),
					tag: "repository".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(repository|repositories)(/|\.|$)".to_string()],
					name_patterns: vec![r"(?i)repository".to_string()],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaPatternRule {
					id: "builtin-service-pattern".to_string(),
					tag: "service".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(service|services)(/|\.|$)".to_string()],
					name_patterns: vec![r"(?i)service".to_string()],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaPatternRule {
					id: "builtin-adapter-pattern".to_string(),
					tag: "adapter".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(adapter|adapters)(/|\.|$)".to_string()],
					name_patterns: vec![r"(?i)adapter".to_string()],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaPatternRule {
					id: "builtin-facade-pattern".to_string(),
					tag: "facade".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(facade|facades)(/|\.|$)".to_string()],
					name_patterns: vec![r"(?i)facade".to_string()],
					entity_kinds: vec![],
					require_exported: None,
				},
				DnaPatternRule {
					id: "builtin-edge-guards-pattern".to_string(),
					tag: "edge-guards".to_string(),
					languages: vec![],
					path_patterns: vec![r"(^|/)(middleware|webhook|webhooks)(/|\.|$)".to_string()],
					name_patterns: vec![],
					entity_kinds: vec![],
					require_exported: None,
				},
			],
			architecture_rules: vec![
				DnaArchitectureRule {
					id: "builtin-server-first-architecture".to_string(),
					architecture: "server-first/modular-monolith".to_string(),
					required_categories: vec![
						DnaCountRequirement { value: "api-surface".to_string(), min_count: 1 },
						DnaCountRequirement { value: "server-actions".to_string(), min_count: 1 },
						DnaCountRequirement { value: "ui-components".to_string(), min_count: 1 },
						DnaCountRequirement { value: "domain-services".to_string(), min_count: 1 },
					],
					required_patterns: vec![],
					min_dependency_depth: None,
					require_dense_dependency_graph: None,
				},
				DnaArchitectureRule {
					id: "builtin-service-heavy-architecture".to_string(),
					architecture: "service-heavy/modular-monolith".to_string(),
					required_categories: vec![
						DnaCountRequirement { value: "api-surface".to_string(), min_count: 1 },
						DnaCountRequirement { value: "domain-services".to_string(), min_count: 1 },
					],
					required_patterns: vec![],
					min_dependency_depth: Some(2),
					require_dense_dependency_graph: Some(true),
				},
				DnaArchitectureRule {
					id: "builtin-feature-sliced-architecture".to_string(),
					architecture: "full-stack/feature-sliced".to_string(),
					required_categories: vec![
						DnaCountRequirement { value: "server-actions".to_string(), min_count: 1 },
						DnaCountRequirement { value: "ui-components".to_string(), min_count: 1 },
					],
					required_patterns: vec![],
					min_dependency_depth: None,
					require_dense_dependency_graph: None,
				},
				DnaArchitectureRule {
					id: "builtin-component-layered-architecture".to_string(),
					architecture: "component-driven/application-layered".to_string(),
					required_categories: vec![DnaCountRequirement { value: "ui-components".to_string(), min_count: 1 }],
					required_patterns: vec![],
					min_dependency_depth: None,
					require_dense_dependency_graph: Some(false),
				},
			],
			error_handling_rules: vec![
				DnaErrorHandlingRule {
					id: "builtin-result-pattern".to_string(),
					label: "result-pattern".to_string(),
					languages: vec![],
					body_patterns: vec![r"Result<".to_string(), r"anyhow::Result".to_string(), r"\bOk\(".to_string(), r"\bErr\(".to_string()],
					min_matches: 1,
				},
				DnaErrorHandlingRule {
					id: "builtin-try-catch-pattern".to_string(),
					label: "try-catch".to_string(),
					languages: vec![],
					body_patterns: vec![r"\btry\b".to_string(), r"\.catch\(".to_string(), r"catch\s*\(".to_string()],
					min_matches: 1,
				},
				DnaErrorHandlingRule {
					id: "builtin-exception-driven-pattern".to_string(),
					label: "exception-driven".to_string(),
					languages: vec![],
					body_patterns: vec![r"\bthrow\b".to_string(), r"panic!".to_string(), r"\braise\b".to_string()],
					min_matches: 1,
				},
			],
			source_path: Some("built-in defaults".to_string()),
		}
	}

	pub fn resolve_for_workspace(workspace_root: &Path) -> Self {
		let built_in = Self::built_in();
		let loaded = Self::load_for_workspace(workspace_root);
		if loaded.source_path.is_some() {
			let mut resolved = loaded;
			resolved.file_category_rules.extend(built_in.file_category_rules);
			resolved.pattern_rules.extend(built_in.pattern_rules);
			resolved.architecture_rules.extend(built_in.architecture_rules);
			resolved.error_handling_rules.extend(built_in.error_handling_rules);
			return resolved;
		}
		built_in
	}

	pub fn load_for_workspace(workspace_root: &Path) -> Self {
		let mut candidates: Vec<PathBuf> = vec![
			workspace_root.join("dna_rules.toml"),
			workspace_root.join(".memix").join("dna_rules.toml"),
		];
		if let Some(home) = dirs::home_dir() {
			candidates.push(home.join(".memix").join("dna_rules.toml"));
		}

		for path in candidates {
			if !path.exists() {
				continue;
			}
			let path_string = path.to_string_lossy().to_string();
			let builder = ConfigLoader::builder().add_source(ConfigFile::new(&path_string, FileFormat::Toml));
			if let Ok(mut rules) = builder.build().and_then(|cfg| cfg.try_deserialize::<DnaRuleConfig>()) {
				rules.source_path = Some(path_string);
				return rules;
			}
		}
		Self::default()
	}
}

impl CodeDna {
	pub fn generate_from_ast(
		file_path: &str,
		functions: &[AstNodeFeature],
		semantic_dependencies: Vec<String>,
		rules: &DnaRuleConfig,
		applied_rule_ids: &mut HashSet<String>,
	) -> Self {
		let mut exports = functions
			.iter()
			.filter(|feature| feature.is_exported)
			.map(|feature| feature.name.clone())
			.collect::<Vec<_>>();
		if exports.is_empty() {
			exports = functions.iter().take(5).map(|feature| feature.name.clone()).collect();
		}

		let complexity = functions
			.iter()
			.map(|feature| feature.cyclomatic_complexity)
			.sum::<u32>();

		let mut pattern_signals = aggregate_pattern_signals(file_path, functions, rules, applied_rule_ids);
		let architectural_categorization = categorize_file(
			file_path,
			functions,
			&pattern_signals,
			rules,
			applied_rule_ids,
		);
		pattern_signals.sort();
		pattern_signals.dedup();

		Self {
			file_path: file_path.to_string(),
			language: functions
				.first()
				.map(|feature| feature.language.clone())
				.unwrap_or_else(|| "unknown".to_string()),
			cyclomatic_complexity: complexity,
			primary_exports: exports,
			semantic_dependencies,
			architectural_categorization,
			pattern_signals,
		}
	}
}

impl ProjectCodeDna {
	pub fn summarize(
		file_features: &HashMap<String, Vec<AstNodeFeature>>,
		dependency_graph: &DependencyGraph,
		recent_change_files: &[String],
		rules: &DnaRuleConfig,
	) -> Self {
		if file_features.is_empty() {
			return Self::default();
		}

		let mut applied_rule_ids = HashSet::new();
		let mut per_file = Vec::new();
		for (file_path, features) in file_features {
			let dependencies = dependency_graph
				.edges_out
				.get(file_path)
				.map(|deps| deps.iter().cloned().collect::<Vec<_>>())
				.unwrap_or_default();
			per_file.push(CodeDna::generate_from_ast(
				file_path,
				features,
				dependencies,
				rules,
				&mut applied_rule_ids,
			));
		}

		let indexed_files = per_file.len();
		let functions_indexed = file_features.values().map(|features| features.len()).sum::<usize>();
		let total_complexity = per_file
			.iter()
			.map(|dna| dna.cyclomatic_complexity as usize)
			.sum::<usize>();
		let avg_complexity = if functions_indexed > 0 {
			total_complexity as f32 / functions_indexed as f32
		} else {
			0.0
		};
		let complexity_score = (avg_complexity / 12.0).clamp(0.0, 1.0);

		let typed_files = file_features
			.keys()
			.filter(|path| is_typed_path(path))
			.count();
		let type_coverage = if indexed_files > 0 {
			typed_files as f32 / indexed_files as f32
		} else {
			0.0
		};

		let dependency_depth = max_dependency_depth(&dependency_graph.edges_out);
		let error_handling = detect_error_handling(file_features, rules, &mut applied_rule_ids);
		let dominant_patterns = detect_patterns(&per_file, &error_handling);
		let architecture = detect_architecture(
			&per_file,
			dependency_graph,
			dependency_depth,
			rules,
			&mut applied_rule_ids,
		);
		let circular_risks = detect_circular_risks(&dependency_graph.edges_out);
		let hot_zones = rank_hot_zones(&per_file, dependency_graph, recent_change_files, 5);
		let stable_zones = rank_stable_zones(&per_file, dependency_graph, recent_change_files, 5);
		let active_development_areas = rank_active_areas(recent_change_files, 4);
		let stale_areas = stable_zones
			.iter()
			.map(|file| format!("{} (no recent semantic changes)", file))
			.collect::<Vec<_>>();
		let test_coverage_estimate = estimate_test_coverage(&per_file);
		let language_breakdown = summarize_languages(&per_file);
		let mut applied_rule_ids = applied_rule_ids.into_iter().collect::<Vec<_>>();
		applied_rule_ids.sort();
		let explainability_summary = build_explainability_summary(
			&architecture,
			&hot_zones,
			&circular_risks,
			&dominant_patterns,
			dependency_depth,
		);

		Self {
			indexed_files,
			functions_indexed,
			architecture,
			complexity_score,
			dominant_patterns,
			hot_zones,
			stable_zones,
			dependency_depth,
			circular_risks,
			type_coverage,
			error_handling,
			test_coverage_estimate,
			active_development_areas,
			stale_areas,
			explainability_summary,
			language_breakdown,
			rules_source: rules.source_path.clone(),
			applied_rule_ids,
		}
	}

	pub fn to_otel_export(&self) -> ObserverDnaOtelExport {
		ObserverDnaOtelExport {
			schema_url: "https://opentelemetry.io/schemas/1.26.0".to_string(),
			resource_attributes: vec![
				otel_attr("memix.dna.indexed_files", self.indexed_files.to_string()),
				otel_attr("memix.dna.functions_indexed", self.functions_indexed.to_string()),
				otel_attr("memix.dna.architecture", self.architecture.clone()),
				otel_attr("memix.dna.complexity_score", format!("{:.4}", self.complexity_score)),
				otel_attr("memix.dna.type_coverage", format!("{:.4}", self.type_coverage)),
				otel_attr("memix.dna.error_handling", self.error_handling.clone()),
				otel_attr(
					"memix.dna.test_coverage_estimate",
					format!("{:.4}", self.test_coverage_estimate),
				),
				otel_attr("memix.dna.dependency_depth", self.dependency_depth.to_string()),
				otel_attr("memix.dna.patterns", self.dominant_patterns.join(",")),
				otel_attr("memix.dna.summary", self.explainability_summary.clone()),
			],
			events: vec![
				OTelEvent {
					name: "memix.dna.hot_zones".to_string(),
					attributes: self
						.hot_zones
						.iter()
						.enumerate()
						.map(|(index, file)| otel_attr(format!("zone.{}", index + 1), file.clone()))
						.collect(),
				},
				OTelEvent {
					name: "memix.dna.circular_risks".to_string(),
					attributes: self
						.circular_risks
						.iter()
						.enumerate()
						.map(|(index, risk)| otel_attr(format!("risk.{}", index + 1), risk.clone()))
						.collect(),
				},
			],
		}
	}
}

fn normalize_path(path: &str) -> String {
	path.replace('\\', "/").to_lowercase()
}

fn is_typed_path(path: &str) -> bool {
	let p = normalize_path(path);
	p.ends_with(".ts")
		|| p.ends_with(".tsx")
		|| p.ends_with(".rs")
		|| p.ends_with(".go")
		|| p.ends_with(".java")
		|| p.ends_with(".cpp")
		|| p.ends_with(".cc")
		|| p.ends_with(".cxx")
		|| p.ends_with(".hpp")
}

fn aggregate_pattern_signals(
	file_path: &str,
	functions: &[AstNodeFeature],
	rules: &DnaRuleConfig,
	applied_rule_ids: &mut HashSet<String>,
) -> Vec<String> {
	let mut patterns = functions
		.iter()
		.flat_map(|feature| feature.pattern_tags.clone())
		.collect::<HashSet<_>>();

	for rule in &rules.pattern_rules {
		if pattern_rule_matches(rule, file_path, functions) {
			patterns.insert(rule.tag.clone());
			applied_rule_ids.insert(rule.id.clone());
		}
	}

	let mut out = patterns.into_iter().collect::<Vec<_>>();
	out.sort();
	out
}

fn categorize_file(
	file_path: &str,
	functions: &[AstNodeFeature],
	pattern_signals: &[String],
	rules: &DnaRuleConfig,
	applied_rule_ids: &mut HashSet<String>,
) -> String {
	for rule in &rules.file_category_rules {
		if category_rule_matches(rule, file_path, functions, pattern_signals) {
			applied_rule_ids.insert(rule.id.clone());
			return rule.classify_as.clone();
		}
	}

	if is_test_file(file_path, functions) {
		"tests".to_string()
	} else if regex_match(r"(^|/)(lib|shared)(/|$)", &normalize_path(file_path)) {
		"shared-library".to_string()
	} else {
		"application-logic".to_string()
	}
}

fn detect_architecture(
	per_file: &[CodeDna],
	dependency_graph: &DependencyGraph,
	dependency_depth: usize,
	rules: &DnaRuleConfig,
	applied_rule_ids: &mut HashSet<String>,
) -> String {
	let category_counts = count_labeled_values(
		per_file
			.iter()
			.map(|dna| dna.architectural_categorization.as_str()),
	);
	let pattern_counts = count_labeled_values(
		per_file
			.iter()
			.flat_map(|dna| dna.pattern_signals.iter().map(|signal| signal.as_str())),
	);
	let dense = dependency_graph.edges_in.values().any(|deps| deps.len() >= 5);

	for rule in &rules.architecture_rules {
		if architecture_rule_matches(rule, &category_counts, &pattern_counts, dependency_depth, dense) {
			applied_rule_ids.insert(rule.id.clone());
			return rule.architecture.clone();
		}
	}

	"modular-monolith".to_string()
}

fn detect_patterns(per_file: &[CodeDna], error_handling: &str) -> Vec<String> {
	let mut patterns = per_file
		.iter()
		.flat_map(|dna| dna.pattern_signals.clone())
		.collect::<HashSet<_>>();
	patterns.insert(error_handling.to_string());
	let mut out = patterns.into_iter().collect::<Vec<_>>();
	out.sort();
	out
}

fn detect_error_handling(
	file_features: &HashMap<String, Vec<AstNodeFeature>>,
	rules: &DnaRuleConfig,
	applied_rule_ids: &mut HashSet<String>,
) -> String {
	let mut best_rule: Option<(&DnaErrorHandlingRule, usize)> = None;

	for rule in &rules.error_handling_rules {
		let matches = count_error_rule_matches(rule, file_features);
		if matches >= rule.min_matches.max(1)
			&& best_rule
				.as_ref()
				.map(|(_, best_matches)| matches > *best_matches)
				.unwrap_or(true)
		{
			best_rule = Some((rule, matches));
		}
	}

	if let Some((rule, _)) = best_rule {
		applied_rule_ids.insert(rule.id.clone());
		return rule.label.clone();
	}

	"mixed".to_string()
}

fn estimate_test_coverage(per_file: &[CodeDna]) -> f32 {
	if per_file.is_empty() {
		return 0.0;
	}

	let test_files = per_file
		.iter()
		.filter(|dna| dna.architectural_categorization == "tests")
		.count();

	(test_files as f32 / per_file.len() as f32).clamp(0.0, 1.0)
}

fn rank_hot_zones(
	per_file: &[CodeDna],
	dependency_graph: &DependencyGraph,
	recent_change_files: &[String],
	limit: usize,
) -> Vec<String> {
	let mut change_counts: HashMap<String, usize> = HashMap::new();
	for file in recent_change_files {
		*change_counts.entry(file.clone()).or_default() += 1;
	}

	let mut ranked = per_file
		.iter()
		.map(|dna| {
			let score = change_counts.get(&dna.file_path).copied().unwrap_or(0) * 10
				+ dependency_graph.edges_in.get(&dna.file_path).map(|deps| deps.len()).unwrap_or(0)
				+ dependency_graph.edges_out.get(&dna.file_path).map(|deps| deps.len()).unwrap_or(0)
				+ dna.cyclomatic_complexity.min(25) as usize;
			(dna.file_path.clone(), score)
		})
		.collect::<Vec<_>>();

	ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
	ranked
		.into_iter()
		.filter(|(_, score)| *score > 0)
		.take(limit)
		.map(|(file, _)| file)
		.collect()
}

fn rank_stable_zones(
	per_file: &[CodeDna],
	dependency_graph: &DependencyGraph,
	recent_change_files: &[String],
	limit: usize,
) -> Vec<String> {
	let recent = recent_change_files.iter().cloned().collect::<HashSet<_>>();
	let mut ranked = per_file
		.iter()
		.filter(|dna| !recent.contains(&dna.file_path))
		.map(|dna| {
			let fan_in = dependency_graph
				.edges_in
				.get(&dna.file_path)
				.map(|deps| deps.len())
				.unwrap_or(0);
			let fan_out = dependency_graph
				.edges_out
				.get(&dna.file_path)
				.map(|deps| deps.len())
				.unwrap_or(0);
			(dna.file_path.clone(), fan_in * 10 + fan_out + dna.primary_exports.len())
		})
		.collect::<Vec<_>>();

	ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
	ranked.into_iter().take(limit).map(|(file, _)| file).collect()
}

fn rank_active_areas(recent_change_files: &[String], limit: usize) -> Vec<String> {
	let mut counts: HashMap<String, usize> = HashMap::new();
	for file in recent_change_files {
		let normalized = normalize_path(file);
		let segments = normalized
			.split('/')
			.filter(|segment| !segment.is_empty())
			.collect::<Vec<_>>();
		let area = if segments.len() >= 2 {
			format!("{}/{}", segments[segments.len() - 2], segments[segments.len() - 1])
		} else {
			normalized
		};
		*counts.entry(area).or_default() += 1;
	}

	let mut ranked = counts.into_iter().collect::<Vec<_>>();
	ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
	ranked.into_iter().take(limit).map(|(area, _)| area).collect()
}

fn detect_circular_risks(edges_out: &HashMap<String, HashSet<String>>) -> Vec<String> {
	let mut cycles = Vec::new();
	for (source, targets) in edges_out {
		for target in targets {
			if edges_out
				.get(target)
				.map(|reverse| reverse.contains(source))
				.unwrap_or(false)
			{
				let repr = if source < target {
					format!("{} ↔ {}", source, target)
				} else {
					format!("{} ↔ {}", target, source)
				};
				if !cycles.contains(&repr) {
					cycles.push(repr);
				}
			}
		}
	}
	cycles.sort();
	cycles.truncate(5);
	cycles
}

fn max_dependency_depth(edges_out: &HashMap<String, HashSet<String>>) -> usize {
	fn dfs(
		node: &str,
		edges_out: &HashMap<String, HashSet<String>>,
		memo: &mut HashMap<String, usize>,
		visiting: &mut HashSet<String>,
	) -> usize {
		if let Some(depth) = memo.get(node) {
			return *depth;
		}
		if !visiting.insert(node.to_string()) {
			return 0;
		}

		let depth = 1 + edges_out
			.get(node)
			.into_iter()
			.flat_map(|children| children.iter())
			.map(|child| dfs(child, edges_out, memo, visiting))
			.max()
			.unwrap_or(0);

		visiting.remove(node);
		memo.insert(node.to_string(), depth);
		depth
	}

	let mut memo = HashMap::new();
	let mut visiting = HashSet::new();
	edges_out
		.keys()
		.map(|node| dfs(node, edges_out, &mut memo, &mut visiting))
		.max()
		.unwrap_or(0)
		.saturating_sub(1)
}

fn summarize_languages(per_file: &[CodeDna]) -> HashMap<String, usize> {
	let mut breakdown = HashMap::new();
	for dna in per_file {
		*breakdown.entry(dna.language.clone()).or_insert(0) += 1;
	}
	breakdown
}

fn count_labeled_values<'a>(values: impl Iterator<Item = &'a str>) -> HashMap<String, usize> {
	let mut counts = HashMap::new();
	for value in values {
		*counts.entry(value.to_string()).or_insert(0) += 1;
	}
	counts
}

fn architecture_rule_matches(
	rule: &DnaArchitectureRule,
	category_counts: &HashMap<String, usize>,
	pattern_counts: &HashMap<String, usize>,
	dependency_depth: usize,
	dense_dependency_graph: bool,
) -> bool {
	rule.required_categories.iter().all(|requirement| {
		category_counts
			.get(&requirement.value)
			.copied()
			.unwrap_or(0)
			>= requirement.min_count.max(1)
	})
		&& rule.required_patterns.iter().all(|requirement| {
			pattern_counts
				.get(&requirement.value)
				.copied()
				.unwrap_or(0)
				>= requirement.min_count.max(1)
		})
		&& rule
			.min_dependency_depth
			.map(|min_depth| dependency_depth >= min_depth)
			.unwrap_or(true)
		&& rule
			.require_dense_dependency_graph
			.map(|required| dense_dependency_graph == required)
			.unwrap_or(true)
}

fn count_error_rule_matches(
	rule: &DnaErrorHandlingRule,
	file_features: &HashMap<String, Vec<AstNodeFeature>>,
) -> usize {
	let mut matches = 0usize;
	for features in file_features.values() {
		for feature in features {
			if !rule.languages.is_empty()
				&& !rule
					.languages
					.iter()
					.any(|language| feature.language.eq_ignore_ascii_case(language))
			{
				continue;
			}
			for pattern in &rule.body_patterns {
				matches += regex_find_count(pattern, &feature.body);
			}
		}
	}
	matches
}

fn build_explainability_summary(
	architecture: &str,
	hot_zones: &[String],
	circular_risks: &[String],
	dominant_patterns: &[String],
	dependency_depth: usize,
) -> String {
	let pattern_summary = dominant_patterns
		.iter()
		.take(3)
		.cloned()
		.collect::<Vec<_>>()
		.join(", ");
	format!(
		"This project uses a {} architecture with {} hot zones, {} circular dependencies, dependency depth {}, and dominant patterns: {}.",
		architecture,
		hot_zones.len(),
		circular_risks.len(),
		dependency_depth,
		if pattern_summary.is_empty() {
			"none detected".to_string()
		} else {
			pattern_summary
		}
	)
}

fn is_test_file(file_path: &str, functions: &[AstNodeFeature]) -> bool {
	let p = normalize_path(file_path);
	p.contains("/tests/")
		|| p.contains(".test.")
		|| p.contains(".spec.")
		|| functions.iter().any(|feature| {
			feature.pattern_tags.iter().any(|tag| tag == "tests") || feature.name.starts_with("test_")
		})
}

fn category_rule_matches(
	rule: &DnaCategoryRule,
	file_path: &str,
	functions: &[AstNodeFeature],
	pattern_signals: &[String],
) -> bool {
	rule_matches_common(
		&rule.languages,
		&rule.path_patterns,
		&[],
		&rule.entity_kinds,
		rule.require_exported,
		file_path,
		functions,
	) && matches_any_required_patterns(pattern_signals, &rule.requires_any_patterns)
		&& matches_all_required_patterns(pattern_signals, &rule.requires_all_patterns)
}

fn pattern_rule_matches(rule: &DnaPatternRule, file_path: &str, functions: &[AstNodeFeature]) -> bool {
	rule_matches_common(
		&rule.languages,
		&rule.path_patterns,
		&rule.name_patterns,
		&rule.entity_kinds,
		rule.require_exported,
		file_path,
		functions,
	)
}

fn rule_matches_common(
	languages: &[String],
	path_patterns: &[String],
	name_patterns: &[String],
	entity_kinds: &[String],
	require_exported: Option<bool>,
	file_path: &str,
	functions: &[AstNodeFeature],
) -> bool {
	let normalized = normalize_path(file_path);
	let language_matches = languages.is_empty()
		|| functions.iter().any(|feature| {
			languages
				.iter()
				.any(|lang| feature.language.eq_ignore_ascii_case(lang))
		});
	let path_matches = path_patterns.is_empty()
		|| path_patterns.iter().any(|pattern| regex_match(pattern, &normalized));
	let name_matches = name_patterns.is_empty()
		|| functions.iter().any(|feature| {
			name_patterns
				.iter()
				.any(|pattern| regex_match(pattern, &feature.name))
		});
	let kind_matches = entity_kinds.is_empty()
		|| functions.iter().any(|feature| {
			entity_kinds
				.iter()
				.any(|kind| feature.kind.eq_ignore_ascii_case(kind))
		});
	let export_matches = require_exported
		.map(|required| functions.iter().any(|feature| feature.is_exported == required))
		.unwrap_or(true);
	language_matches && path_matches && name_matches && kind_matches && export_matches
}

fn matches_any_required_patterns(pattern_signals: &[String], required: &[String]) -> bool {
	required.is_empty()
		|| pattern_signals
			.iter()
			.any(|pattern| required.iter().any(|required_pattern| pattern == required_pattern))
}

fn matches_all_required_patterns(pattern_signals: &[String], required: &[String]) -> bool {
	required
		.iter()
		.all(|required_pattern| pattern_signals.iter().any(|pattern| pattern == required_pattern))
}

fn regex_match(pattern: &str, value: &str) -> bool {
	Regex::new(pattern)
		.map(|regex| regex.is_match(value))
		.unwrap_or_else(|_| value.contains(pattern))
}

fn regex_find_count(pattern: &str, value: &str) -> usize {
	Regex::new(pattern)
		.map(|regex| regex.find_iter(value).count())
		.unwrap_or_else(|_| value.matches(pattern).count())
}

fn otel_attr(key: impl Into<String>, value: impl Into<String>) -> OTelAttribute {
	OTelAttribute {
		key: key.into(),
		value: value.into(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn summarizes_project_code_dna_with_expected_signals() {
		let mut file_features = HashMap::new();
		file_features.insert(
			"src/app/api/route.ts".to_string(),
			vec![AstNodeFeature {
				name: "GET".to_string(),
				kind: "function".to_string(),
				body: "export async function GET() { if (true) { return ok; } }".to_string(),
				start_byte: 0,
				end_byte: 10,
				language: "typescript".to_string(),
				cyclomatic_complexity: 2,
				pattern_tags: vec!["api-route".to_string()],
				is_exported: true,
				calls: vec![],
				line_count: None,
			}],
		);
		file_features.insert(
			"src/components/Button.tsx".to_string(),
			vec![AstNodeFeature {
				name: "Button".to_string(),
				kind: "function".to_string(),
				body: "function Button() { return value && other; }".to_string(),
				start_byte: 0,
				end_byte: 10,
				language: "tsx".to_string(),
				cyclomatic_complexity: 2,
				pattern_tags: vec!["component-driven".to_string()],
				is_exported: true,
				calls: vec![],
				line_count: None,
			}],
		);
		file_features.insert(
			"src/lib/repository.rs".to_string(),
			vec![AstNodeFeature {
				name: "save".to_string(),
				kind: "function".to_string(),
				body: "fn save() -> Result<(), anyhow::Error> { Ok(()) }".to_string(),
				start_byte: 0,
				end_byte: 10,
				language: "rust".to_string(),
				cyclomatic_complexity: 1,
				pattern_tags: vec!["repository".to_string()],
				is_exported: true,
				calls: vec![],
				line_count: None,
			}],
		);

		let mut dependency_graph = DependencyGraph::new();
		dependency_graph.add_dependency("src/app/api/route.ts", "src/lib/repository.rs");
		dependency_graph.add_dependency("src/components/Button.tsx", "src/lib/repository.rs");

		let summary = ProjectCodeDna::summarize(
			&file_features,
			&dependency_graph,
			&[
				"src/app/api/route.ts".to_string(),
				"src/app/api/route.ts".to_string(),
			],
			&DnaRuleConfig::default(),
		);

		assert_eq!(summary.indexed_files, 3);
		assert_eq!(summary.functions_indexed, 3);
		assert!(summary.architecture.contains("monolith") || summary.architecture.contains("server-first"));
		assert!(summary.complexity_score > 0.0);
		assert_eq!(summary.error_handling, "result-pattern");
		assert!(summary.dominant_patterns.iter().any(|pattern| pattern == "repository"));
		assert!(summary.hot_zones.iter().any(|file| file == "src/app/api/route.ts"));
		assert!(summary.type_coverage > 0.9);
		assert!(!summary.explainability_summary.is_empty());
		assert!(summary.language_breakdown.contains_key("rust"));
	}

	#[test]
	fn emits_otel_export() {
		let dna = ProjectCodeDna {
			indexed_files: 2,
			functions_indexed: 4,
			architecture: "server-first/modular-monolith".to_string(),
			complexity_score: 0.42,
			dominant_patterns: vec!["repository".to_string(), "component-driven".to_string()],
			hot_zones: vec!["src/app/api/route.ts".to_string()],
			stable_zones: vec![],
			dependency_depth: 3,
			circular_risks: vec!["a ↔ b".to_string()],
			type_coverage: 1.0,
			error_handling: "result-pattern".to_string(),
			test_coverage_estimate: 0.5,
			active_development_areas: vec![],
			stale_areas: vec![],
			explainability_summary: "summary".to_string(),
			language_breakdown: HashMap::new(),
			rules_source: None,
			applied_rule_ids: vec![],
		};

		let export = dna.to_otel_export();
		assert!(export.schema_url.contains("opentelemetry"));
		assert!(export.resource_attributes.iter().any(|attr| attr.key == "memix.dna.architecture"));
		assert!(export.events.iter().any(|event| event.name == "memix.dna.hot_zones"));
	}
}
