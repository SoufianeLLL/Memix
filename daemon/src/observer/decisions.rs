// Intelligent Decision Detection Engine
// Automatically observes and records architectural decisions from code signals
// using configurable rules, AST pattern matching, and embedding similarity.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use regex::Regex;
use tokio::sync::RwLock;

use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};

// ═══════════════════════════════════════════════════════════════════════════════
// EMBEDDING-BASED PATTERN DETECTION
// ═══════════════════════════════════════════════════════════════════════════════

/// Pattern reference stored in embedding space for similarity matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternReference {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub example_files: Vec<String>,
    /// Tags to apply when this pattern is detected
    pub decision_tags: Vec<String>,
    /// Decision title template
    pub decision_title: String,
}

/// Result of embedding-based pattern detection
#[derive(Debug, Clone)]
pub struct EmbeddingPatternMatch {
    pub pattern: PatternReference,
    pub similarity: f32,
    pub file: String,
}

/// Detect patterns by computing embedding similarity against known pattern references.
/// This enables detection of patterns not covered by explicit rules.
pub async fn detect_pattern_by_embedding(
    embedding: &[f32],
    file_path: &str,
    pattern_embeddings: &HashMap<String, (Vec<f32>, PatternReference)>,
    threshold: f32,
) -> Option<EmbeddingPatternMatch> {
    if embedding.is_empty() || pattern_embeddings.is_empty() {
        return None;
    }

    // Normalize query embedding
    let query_norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if query_norm < f32::EPSILON {
        return None;
    }
    let query_normalized: Vec<f32> = embedding.iter().map(|x| x / query_norm).collect();

    // Find best matching pattern
    let mut best_match: Option<(f32, &PatternReference)> = None;

    for (_, (pattern_vec, pattern)) in pattern_embeddings {
        let vec_norm: f32 = pattern_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if vec_norm < f32::EPSILON {
            continue;
        }

        let similarity: f32 = query_normalized
            .iter()
            .zip(pattern_vec.iter())
            .map(|(q, v)| q * (v / vec_norm))
            .sum();

        if similarity >= threshold {
            match &best_match {
                None => best_match = Some((similarity, pattern)),
                Some((best_sim, _)) if similarity > *best_sim => {
                    best_match = Some((similarity, pattern));
                }
                _ => {}
            }
        }
    }

    best_match.map(|(similarity, pattern)| EmbeddingPatternMatch {
        pattern: pattern.clone(),
        similarity,
        file: file_path.to_string(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// RULE DEFINITIONS - Loaded from TOML files
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionRule {
    pub id: String,
    pub name: String,
    pub trigger: TriggerKind,
    pub condition: Condition,
    pub template: DecisionTemplate,
    pub confidence: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    FileSave,
    DependencyAdded,
    DirectoryCreated,
    ConfigChanged,
    ImportResolved,
    FileMoved,
    EndpointCreated,
    GitCommit,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Condition {
    /// Regex pattern for file path
    pub file_pattern: Option<String>,
    /// Regex pattern for directory path
    pub path_pattern: Option<String>,
    /// Required language (typescript, rust, etc.)
    pub language: Option<String>,
    /// Tree-sitter AST pattern to match
    pub ast_pattern: Option<String>,
    /// Regex for dependency name
    pub dependency_pattern: Option<String>,
    /// Config file name regex
    pub file: Option<String>,
    /// Config key path
    pub key: Option<String>,
    /// Config value
    pub value: Option<String>,
    /// Minimum confidence for AST pattern match
    pub min_confidence: Option<f32>,
    /// AST diff type (symbol_renamed, etc.)
    pub ast_diff: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionTemplate {
    pub title: String,
    pub rationale: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub alternatives: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// SIGNALS - Events that can trigger decision detection
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum DecisionSignal {
    DependencyAdded {
        name: String,
        version: String,
        file: String,
    },
    DirectoryCreated {
        path: String,
    },
    FileSaved {
        path: String,
        language: String,
        content: String,
        features: Vec<AstFeature>,
    },
    FileMoved {
        old_path: String,
        new_path: String,
    },
    ConfigChanged {
        file: String,
        key: String,
        value: String,
    },
    EndpointCreated {
        method: String,
        path: String,
        file: String,
    },
    PatternDetected {
        pattern_id: String,
        pattern_name: String,
        confidence: f32,
        file: String,
        evidence: Vec<String>,
    },
    GitCommit {
        message: String,
        files_changed: Vec<String>,
    },
    SymbolRenamed {
        old_name: String,
        new_name: String,
        file: String,
    },
}

#[derive(Debug, Clone)]
pub struct AstFeature {
    pub name: String,
    pub kind: String,
    pub is_exported: bool,
    pub pattern_tags: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// DETECTED DECISION - Output of the detection process
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedDecision {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub rule_id: String,
    pub triggered_by: String,
    pub created_at: DateTime<Utc>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// RULE LOADER - Loads and validates rules from TOML files
// ═══════════════════════════════════════════════════════════════════════════════

pub struct RuleLoader {
    rules_dir: std::path::PathBuf,
}

impl RuleLoader {
    pub fn new(rules_dir: std::path::PathBuf) -> Self {
        Self { rules_dir }
    }

    pub fn load_rules(&self) -> Vec<DecisionRule> {
        let mut rules = Vec::new();
        
        // Load decisions.toml
        let decisions_path = self.rules_dir.join("decisions.toml");
        if decisions_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&decisions_path) {
                match toml::from_str::<DecisionRulesFile>(&content) {
                    Ok(file) => rules.extend(file.rule),
                    Err(e) => {
                        tracing::error!("Failed to parse decisions.toml: {}", e);
                    }
                }
            }
        }

        // Validate and log
        tracing::info!("Loaded {} decision rules from {}", rules.len(), self.rules_dir.display());
        for rule in &rules {
            tracing::debug!("  - {} (trigger: {:?}, confidence: {:.2})", rule.id, rule.trigger, rule.confidence);
        }

        rules
    }
}

#[derive(Debug, Deserialize)]
struct DecisionRulesFile {
    rule: Vec<DecisionRule>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// DECISION DETECTOR - Main detection engine
// ═══════════════════════════════════════════════════════════════════════════════

pub struct DecisionDetector {
    rules: Vec<DecisionRule>,
    rule_loader: RuleLoader,
    /// Cache of already-recorded decisions to avoid duplicates
    recorded: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    /// Previous dependency state for change detection
    previous_deps: HashMap<String, String>,
    /// Pattern embeddings for similarity-based detection
    pattern_embeddings: HashMap<String, (Vec<f32>, PatternReference)>,
    /// Similarity threshold for embedding-based pattern detection
    embedding_threshold: f32,
}

impl DecisionDetector {
    pub fn new(rules_dir: std::path::PathBuf) -> Self {
        let rule_loader = RuleLoader::new(rules_dir);
        let rules = rule_loader.load_rules();
        
        Self {
            rules,
            rule_loader,
            recorded: Arc::new(RwLock::new(HashMap::new())),
            previous_deps: HashMap::new(),
            pattern_embeddings: HashMap::new(),
            embedding_threshold: 0.92, // High threshold for pattern detection
        }
    }

    /// Add a pattern reference with its embedding for similarity detection
    pub fn add_pattern_embedding(&mut self, pattern: PatternReference, embedding: Vec<f32>) {
        self.pattern_embeddings.insert(pattern.id.clone(), (embedding, pattern));
    }

    /// Set the embedding similarity threshold
    pub fn set_embedding_threshold(&mut self, threshold: f32) {
        self.embedding_threshold = threshold;
    }

    /// Reload rules from disk (hot-reload support)
    pub fn reload_rules(&mut self) {
        self.rules = self.rule_loader.load_rules();
        tracing::info!("Decision rules reloaded: {} rules active", self.rules.len());
    }

    /// Process a signal and return any decisions that should be recorded
    pub async fn process_signal(&mut self, signal: DecisionSignal) -> Vec<DetectedDecision> {
        let mut decisions = Vec::new();
        let trigger = signal.trigger_kind();

        // Find matching rules
        for rule in &self.rules {
            if rule.trigger != trigger {
                continue;
            }

            if let Some(decision) = self.try_match_rule(rule, &signal).await {
                // Check for duplicates
                if !self.is_duplicate(&decision.id).await {
                    decisions.push(decision);
                }
            }
        }

        // Update previous state for dependency tracking
        if let DecisionSignal::DependencyAdded { name, version, .. } = &signal {
            self.previous_deps.insert(name.clone(), version.clone());
        }

        decisions
    }

    /// Try to match a rule against a signal
    async fn try_match_rule(&self, rule: &DecisionRule, signal: &DecisionSignal) -> Option<DetectedDecision> {
        let matches = match signal {
            DecisionSignal::DependencyAdded { name, version, file } => {
                self.match_dependency_rule(rule, name, version, file)
            }
            DecisionSignal::DirectoryCreated { path } => {
                self.match_path_rule(rule, path)
            }
            DecisionSignal::FileSaved { path, language, content, features } => {
                self.match_file_rule(rule, path, language, content, features)
            }
            DecisionSignal::FileMoved { old_path, new_path } => {
                self.match_move_rule(rule, old_path, new_path)
            }
            DecisionSignal::ConfigChanged { file, key, value } => {
                self.match_config_rule(rule, file, key, value)
            }
            DecisionSignal::EndpointCreated { method, path, file } => {
                self.match_endpoint_rule(rule, method, path, file)
            }
            DecisionSignal::PatternDetected { pattern_id, pattern_name, confidence, file, evidence } => {
                self.match_pattern_rule(rule, pattern_id, pattern_name, *confidence, file, evidence)
            }
            DecisionSignal::GitCommit { message, files_changed } => {
                self.match_commit_rule(rule, message, files_changed)
            }
            DecisionSignal::SymbolRenamed { old_name, new_name, file } => {
                self.match_rename_rule(rule, old_name, new_name, file)
            }
        };

        if matches {
            Some(self.build_decision(rule, signal))
        } else {
            None
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // RULE MATCHING METHODS
    // ─────────────────────────────────────────────────────────────────────────

    fn match_dependency_rule(&self, rule: &DecisionRule, name: &str, _version: &str, _file: &str) -> bool {
        if let Some(ref pattern) = rule.condition.dependency_pattern {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(name) {
                    return false;
                }
            }
        }
        true
    }

    fn match_path_rule(&self, rule: &DecisionRule, path: &str) -> bool {
        if let Some(ref pattern) = rule.condition.path_pattern {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(path) {
                    return false;
                }
            }
        }
        true
    }

    fn match_file_rule(&self, rule: &DecisionRule, path: &str, language: &str, _content: &str, features: &[AstFeature]) -> bool {
        // Check file pattern
        if let Some(ref pattern) = rule.condition.file_pattern {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(path) {
                    return false;
                }
            }
        }

        // Check path pattern
        if let Some(ref pattern) = rule.condition.path_pattern {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(path) {
                    return false;
                }
            }
        }

        // Check language
        if let Some(ref required_lang) = rule.condition.language {
            if required_lang.to_lowercase() != language.to_lowercase() {
                return false;
            }
        }

        // Check AST pattern tags
        if let Some(ref ast_pattern) = rule.condition.ast_pattern {
            let has_pattern = features.iter().any(|f| {
                f.pattern_tags.iter().any(|t| t.to_lowercase().contains(&ast_pattern.to_lowercase()))
            });
            if !has_pattern {
                return false;
            }
        }

        true
    }

    fn match_move_rule(&self, rule: &DecisionRule, _old_path: &str, new_path: &str) -> bool {
        if let Some(ref pattern) = rule.condition.path_pattern {
            if let Ok(re) = Regex::new(pattern) {
                if !re.is_match(new_path) {
                    return false;
                }
            }
        }
        true
    }

    fn match_config_rule(&self, rule: &DecisionRule, file: &str, key: &str, value: &str) -> bool {
        if let Some(ref file_pattern) = rule.condition.file {
            if let Ok(re) = Regex::new(file_pattern) {
                if !re.is_match(file) {
                    return false;
                }
            }
        }
        if let Some(ref required_key) = rule.condition.key {
            if required_key != key {
                return false;
            }
        }
        if let Some(ref required_value) = rule.condition.value {
            if required_value != value {
                return false;
            }
        }
        true
    }

    fn match_endpoint_rule(&self, _rule: &DecisionRule, _method: &str, _path: &str, _file: &str) -> bool {
        true
    }

    fn match_pattern_rule(&self, rule: &DecisionRule, pattern_id: &str, _pattern_name: &str, confidence: f32, _file: &str, _evidence: &[String]) -> bool {
        if let Some(ref ast_pattern) = rule.condition.ast_pattern {
            if ast_pattern.to_lowercase() != pattern_id.to_lowercase() {
                return false;
            }
        }
        if let Some(min_conf) = rule.condition.min_confidence {
            if confidence < min_conf {
                return false;
            }
        }
        true
    }

    fn match_commit_rule(&self, _rule: &DecisionRule, message: &str, _files_changed: &[String]) -> bool {
        let decision_keywords = ["introduce", "use", "adopt", "switch to", "migrate to", "refactor"];
        decision_keywords.iter().any(|k| message.to_lowercase().contains(k))
    }

    fn match_rename_rule(&self, rule: &DecisionRule, _old_name: &str, _new_name: &str, _file: &str) -> bool {
        rule.condition.ast_diff.as_deref() == Some("symbol_renamed")
    }

    // ─────────────────────────────────────────────────────────────────────────
    // DECISION BUILDING
    // ─────────────────────────────────────────────────────────────────────────

    fn build_decision(&self, rule: &DecisionRule, signal: &DecisionSignal) -> DetectedDecision {
        let now = Utc::now();
        let placeholders = self.extract_placeholders(signal);
        
        let title = self.fill_template(&rule.template.title, &placeholders);
        let rationale = self.fill_template(&rule.template.rationale, &placeholders);

        let id = format!("decision_{}_{}", rule.id, now.timestamp_millis());

        DetectedDecision {
            id,
            title,
            rationale,
            alternatives: rule.template.alternatives.clone(),
            tags: rule.template.tags.clone(),
            confidence: rule.confidence,
            evidence: self.extract_evidence(signal),
            rule_id: rule.id.clone(),
            triggered_by: format!("{:?}", signal.trigger_kind()),
            created_at: now,
        }
    }

    fn extract_placeholders(&self, signal: &DecisionSignal) -> HashMap<String, String> {
        let mut map = HashMap::new();
        
        match signal {
            DecisionSignal::DependencyAdded { name, version, file } => {
                map.insert("name".to_string(), name.clone());
                map.insert("version".to_string(), version.clone());
                map.insert("file".to_string(), file.clone());
            }
            DecisionSignal::DirectoryCreated { path } => {
                map.insert("path".to_string(), path.clone());
            }
            DecisionSignal::FileSaved { path, language, .. } => {
                map.insert("path".to_string(), path.clone());
                map.insert("language".to_string(), language.clone());
                map.insert("file".to_string(), path.clone());
                map.insert("filename".to_string(), Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
            }
            DecisionSignal::FileMoved { old_path, new_path } => {
                map.insert("old_path".to_string(), old_path.clone());
                map.insert("new_path".to_string(), new_path.clone());
            }
            DecisionSignal::ConfigChanged { file, key, value } => {
                map.insert("file".to_string(), file.clone());
                map.insert("key".to_string(), key.clone());
                map.insert("value".to_string(), value.clone());
            }
            DecisionSignal::EndpointCreated { method, path, file } => {
                map.insert("method".to_string(), method.clone());
                map.insert("path".to_string(), path.clone());
                map.insert("file".to_string(), file.clone());
            }
            DecisionSignal::PatternDetected { pattern_id, pattern_name, confidence, file, .. } => {
                map.insert("pattern_id".to_string(), pattern_id.clone());
                map.insert("pattern_name".to_string(), pattern_name.clone());
                map.insert("confidence".to_string(), format!("{:.0}%", confidence * 100.0));
                map.insert("file".to_string(), file.clone());
                map.insert("name".to_string(), pattern_name.clone());
            }
            DecisionSignal::GitCommit { message, files_changed } => {
                map.insert("message".to_string(), message.clone());
                map.insert("files_count".to_string(), files_changed.len().to_string());
            }
            DecisionSignal::SymbolRenamed { old_name, new_name, file } => {
                map.insert("old_name".to_string(), old_name.clone());
                map.insert("new_name".to_string(), new_name.clone());
                map.insert("file".to_string(), file.clone());
            }
        }
        
        map
    }

    fn fill_template(&self, template: &str, placeholders: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in placeholders {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }

    fn extract_evidence(&self, signal: &DecisionSignal) -> Vec<String> {
        match signal {
            DecisionSignal::DependencyAdded { name, version, file } => {
                vec![format!("{}: {}@{}", file, name, version)]
            }
            DecisionSignal::DirectoryCreated { path } => {
                vec![format!("Directory created: {}", path)]
            }
            DecisionSignal::FileSaved { path, .. } => {
                vec![format!("File saved: {}", path)]
            }
            DecisionSignal::FileMoved { old_path, new_path } => {
                vec![format!("{} -> {}", old_path, new_path)]
            }
            DecisionSignal::ConfigChanged { file, key, value } => {
                vec![format!("{}: {} = {}", file, key, value)]
            }
            DecisionSignal::EndpointCreated { method, path, file } => {
                vec![format!("{}: {} {}", file, method, path)]
            }
            DecisionSignal::PatternDetected { evidence, .. } => {
                evidence.clone()
            }
            DecisionSignal::GitCommit { message, files_changed } => {
                let mut ev = vec![format!("Commit: {}", message)];
                ev.extend(files_changed.iter().map(|f| format!("Changed: {}", f)));
                ev
            }
            DecisionSignal::SymbolRenamed { old_name, new_name, file } => {
                vec![format!("{}: {} -> {}", file, old_name, new_name)]
            }
        }
    }

    async fn is_duplicate(&self, id: &str) -> bool {
        let recorded = self.recorded.read().await;
        recorded.contains_key(id)
    }

    /// Convert a detected decision to a MemoryEntry for storage
    pub fn to_memory_entry(decision: &DetectedDecision, project_id: &str) -> MemoryEntry {
        let content = serde_json::json!({
            "title": decision.title,
            "rationale": decision.rationale,
            "alternatives": decision.alternatives,
            "evidence": decision.evidence,
            "confidence": decision.confidence,
            "rule_id": decision.rule_id,
            "triggered_by": decision.triggered_by,
        }).to_string();

        MemoryEntry {
            id: decision.id.clone(),
            project_id: project_id.to_string(),
            kind: MemoryKind::Decision,
            content,
            tags: decision.tags.clone(),
            source: MemorySource::AgentExtracted,
            superseded_by: None,
            contradicts: vec![],
            parent_id: None,
            caused_by: vec![],
            enables: vec![],
            created_at: decision.created_at,
            updated_at: decision.created_at,
            access_count: 0,
            last_accessed_at: None,
        }
    }

    /// Mark a decision as recorded
    pub async fn mark_recorded(&self, id: String) {
        let mut recorded = self.recorded.write().await;
        recorded.insert(id, Utc::now());
    }

    /// Get previous dependencies for change detection
    pub fn get_previous_deps(&self) -> &HashMap<String, String> {
        &self.previous_deps
    }

    /// Detect patterns using embedding similarity
    pub async fn detect_from_embedding(
        &self,
        embedding: &[f32],
        file_path: &str,
    ) -> Option<DetectedDecision> {
        let match_result = detect_pattern_by_embedding(
            embedding,
            file_path,
            &self.pattern_embeddings,
            self.embedding_threshold,
        ).await?;

        let now = Utc::now();
        let pattern = &match_result.pattern;

        Some(DetectedDecision {
            id: format!("decision_embedding_{}_{}", pattern.id, now.timestamp_millis()),
            title: pattern.decision_title.replace("{file}", file_path),
            rationale: format!(
                "Detected {} pattern via embedding similarity ({:.0}%). {}",
                pattern.name,
                match_result.similarity * 100.0,
                pattern.description
            ),
            alternatives: vec![],
            tags: pattern.decision_tags.clone(),
            confidence: match_result.similarity,
            evidence: vec![
                format!("File: {}", file_path),
                format!("Pattern: {} (similarity: {:.2})", pattern.name, match_result.similarity),
            ],
            rule_id: format!("embedding_{}", pattern.id),
            triggered_by: "EmbeddingSimilarity".to_string(),
            created_at: now,
        })
    }

    /// Adjust rule confidence based on accumulated feedback
    /// This implements the self-improving confidence mechanism
    pub fn adjust_rule_from_feedback(&mut self, rule_id: &str, feedback: &str) {
        // Find the rule by ID
        let rule = self.rules.iter_mut().find(|r| r.id == rule_id);
        
        if let Some(rule) = rule {
            let adjustment = match feedback {
                "useful" => 0.05,      // Positive feedback: increase confidence
                "dismissed" => -0.02,  // Dismissed: slight decrease
                "incorrect" => -0.10, // Incorrect: significant decrease
                _ => 0.0,
            };

            let old_confidence = rule.confidence;
            rule.confidence = (rule.confidence + adjustment).clamp(0.0, 1.0);

            if old_confidence != rule.confidence {
                tracing::info!(
                    "Rule '{}' confidence adjusted: {:.2} -> {:.2} (feedback: {})",
                    rule_id, old_confidence, rule.confidence, feedback
                );
            }
        }
    }

    /// Get statistics about rule performance based on feedback
    pub fn get_rule_stats(&self) -> HashMap<String, RuleStats> {
        // This would be populated from stored feedback in a real implementation
        // For now, return current confidence levels
        self.rules
            .iter()
            .map(|r| {
                (
                    r.id.clone(),
                    RuleStats {
                        confidence: r.confidence,
                        trigger_count: 0, // Would be tracked in production
                        useful_count: 0,
                        dismissed_count: 0,
                        incorrect_count: 0,
                    },
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleStats {
    pub confidence: f32,
    pub trigger_count: u64,
    pub useful_count: u64,
    pub dismissed_count: u64,
    pub incorrect_count: u64,
}

impl DecisionSignal {
    pub fn trigger_kind(&self) -> TriggerKind {
        match self {
            DecisionSignal::DependencyAdded { .. } => TriggerKind::DependencyAdded,
            DecisionSignal::DirectoryCreated { .. } => TriggerKind::DirectoryCreated,
            DecisionSignal::FileSaved { .. } => TriggerKind::FileSave,
            DecisionSignal::FileMoved { .. } => TriggerKind::FileMoved,
            DecisionSignal::ConfigChanged { .. } => TriggerKind::ConfigChanged,
            DecisionSignal::EndpointCreated { .. } => TriggerKind::EndpointCreated,
            DecisionSignal::PatternDetected { .. } => TriggerKind::FileSave,
            DecisionSignal::GitCommit { .. } => TriggerKind::GitCommit,
            DecisionSignal::SymbolRenamed { .. } => TriggerKind::FileSave,
        }
    }
}

/// Read dependencies from package.json and return new ones compared to previous scan
pub fn detect_new_dependencies(
    root: &Path,
    previous_deps: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let pkg_path = root.join("package.json");
    let content = match std::fs::read_to_string(&pkg_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let mut new_deps = Vec::new();

    for section in ["dependencies", "devDependencies"] {
        if let Some(deps) = pkg.get(section).and_then(|v| v.as_object()) {
            for (name, value) in deps {
                let version = value.as_str().unwrap_or("*").to_string();
                let is_new = !previous_deps.contains_key(name);
                let is_changed = previous_deps.get(name).map(|v| v != &version).unwrap_or(false);

                if is_new || is_changed {
                    new_deps.push((name.clone(), version));
                }
            }
        }
    }

    new_deps
}

/// Extract commit message decisions (keywords like "use X", "introduce Y")
pub fn extract_commit_decisions(message: &str) -> Option<(String, String)> {
    let patterns = [
        (r"use\s+(\w+)\s+(?:for|as|in)\s+(.+)", "use"),
        (r"introduce\s+(\w+)\s+(?:for|as|in)\s+(.+)", "introduce"),
        (r"adopt\s+(\w+)\s+(?:for|as|in)\s+(.+)", "adopt"),
        (r"switch\s+to\s+(\w+)\s+(?:for|as|in)\s*(.*)", "switch"),
        (r"migrate\s+(?:to\s+)?(\w+)\s*(.*)", "migrate"),
    ];

    for (pattern, _kind) in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(message) {
                let tool = caps.get(1).map(|m| m.as_str().to_string())?;
                let purpose = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                return Some((tool, purpose));
            }
        }
    }

    None
}

impl Default for DecisionDetector {
    fn default() -> Self {
        Self::new(std::path::PathBuf::from("rules"))
    }
}
