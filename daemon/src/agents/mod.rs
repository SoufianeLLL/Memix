use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::observer::differ::SemanticDiff;
use crate::observer::graph::DependencyGraph;
use crate::observer::parser::AstNodeFeature;

pub mod terminal_proxy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentTrigger {
    FileSave,
    FileOpen,
    FunctionSignatureChange,
    Interval { seconds: u64 },
    SessionStart,
    GitCommit,
    Manual,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub trigger: AgentTrigger,
    pub scope: String,
    pub action_description: String,
    pub output_key: String,
    pub cooldown_ms: u64,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNotification {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReport {
    pub agent_name: String,
    pub entry_id: String,
    pub output_key: String,
    pub severity: AgentSeverity,
    pub notifications: Vec<AgentNotification>,
    pub data: Value,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSaveAgentContext {
    pub project_id: String,
    pub file_path: String,
    pub file_content: String,
    pub diff: SemanticDiff,
    pub features: Vec<AstNodeFeature>,
    pub dependency_graph: DependencyGraph,
    pub intent_type: String,
    pub intent_confidence: f32,
    pub breaking_signatures: Vec<(String, String, String)>,
    pub recent_change_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartContext {
    pub project_id: String,
    pub workspace_root: String,
}

pub struct AgentRuntime {
    workspace_root: PathBuf,
    configs: Vec<AgentConfig>,
    last_run: HashMap<String, Instant>,
    recent_reports: VecDeque<AgentReport>,
    source_path: Option<String>,
}

impl AgentRuntime {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let mut runtime = Self {
            workspace_root: workspace_root.into(),
            configs: Vec::new(),
            last_run: HashMap::new(),
            recent_reports: VecDeque::with_capacity(128),
            source_path: None,
        };
        runtime.reload();
        runtime
    }

    pub fn reload(&mut self) {
        let (configs, source_path) = load_agent_configs(&self.workspace_root);
        self.configs = configs;
        self.source_path = source_path;
    }

    pub fn source_path(&self) -> Option<String> {
        self.source_path.clone()
    }

    pub fn configs(&self) -> Vec<AgentConfig> {
        self.configs.clone()
    }

    pub fn recent_reports(&self) -> Vec<AgentReport> {
        self.recent_reports.iter().cloned().collect()
    }

    pub fn process_file_save(&mut self, ctx: &FileSaveAgentContext) -> Vec<AgentReport> {
        let mut reports = Vec::new();
        for config in self.configs.clone() {
            if !matches!(config.trigger, AgentTrigger::FileSave | AgentTrigger::FunctionSignatureChange) {
                continue;
            }
            if matches!(config.trigger, AgentTrigger::FunctionSignatureChange)
                && ctx.breaking_signatures.is_empty()
                && ctx.diff.nodes_modified.is_empty()
            {
                continue;
            }
            if !self.cooldown_elapsed(&config.name, config.cooldown_ms) {
                continue;
            }
            if let Some(report) = execute_file_save_agent(&config, ctx) {
                self.record_report(&config.name, report.clone());
                reports.push(report);
            }
        }
        reports
    }

    pub fn process_session_start(&mut self, ctx: &SessionStartContext) -> Vec<AgentReport> {
        let mut reports = Vec::new();
        for config in self.configs.clone() {
            if !matches!(config.trigger, AgentTrigger::SessionStart) {
                continue;
            }
            if !self.cooldown_elapsed(&config.name, config.cooldown_ms) {
                continue;
            }
            let report = AgentReport {
                agent_name: config.name.clone(),
                entry_id: agent_entry_id(&config.name),
                output_key: config.output_key.clone(),
                severity: AgentSeverity::Info,
                notifications: vec![AgentNotification {
                    title: format!("{} initialized", config.name),
                    message: format!("Session started for {} at {}", ctx.project_id, ctx.workspace_root),
                }],
                data: json!({
                    "project_id": ctx.project_id,
                    "workspace_root": ctx.workspace_root,
                    "scope": config.scope,
                    "action_description": config.action_description,
                }),
                generated_at: Utc::now(),
            };
            self.record_report(&config.name, report.clone());
            reports.push(report);
        }
        reports
    }

    fn cooldown_elapsed(&mut self, agent_name: &str, cooldown_ms: u64) -> bool {
        let now = Instant::now();
        let cooldown = Duration::from_millis(cooldown_ms);
        let allowed = self
            .last_run
            .get(agent_name)
            .map(|last| now.duration_since(*last) >= cooldown)
            .unwrap_or(true);
        if allowed {
            self.last_run.insert(agent_name.to_string(), now);
        }
        allowed
    }

    fn record_report(&mut self, agent_name: &str, report: AgentReport) {
        if self.recent_reports.len() >= 128 {
            self.recent_reports.pop_front();
        }
        self.recent_reports.push_back(report);
        self.last_run.insert(agent_name.to_string(), Instant::now());
    }
}

fn execute_file_save_agent(config: &AgentConfig, ctx: &FileSaveAgentContext) -> Option<AgentReport> {
    match config.name.as_str() {
        "CodeObserver" => Some(build_code_observer_report(config, ctx)),
        "IntentPredictor" => Some(build_intent_predictor_report(config, ctx)),
        "SecurityScanner" => build_security_scanner_report(config, ctx),
        "ComplexityWatcher" => build_complexity_watcher_report(config, ctx),
        "TestSentinel" => build_test_sentinel_report(config, ctx),
        "PatternLearner" => Some(build_pattern_learner_report(config, ctx)),
        "DocumentationTracker" => build_documentation_tracker_report(config, ctx),
        _ => Some(build_generic_report(config, ctx)),
    }
}

fn build_code_observer_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> AgentReport {
    let added = ctx.diff.nodes_added.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    let removed = ctx.diff.nodes_removed.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    let modified = ctx.diff.nodes_modified.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    let severity = if !ctx.breaking_signatures.is_empty() {
        AgentSeverity::Warning
    } else {
        AgentSeverity::Info
    };
    let mut notifications = vec![AgentNotification {
        title: format!("{} observed a code mutation", config.name),
        message: format!(
            "{} added, {} removed, {} modified in {}",
            added.len(),
            removed.len(),
            modified.len(),
            ctx.file_path
        ),
    }];
    if !ctx.breaking_signatures.is_empty() {
        notifications.push(AgentNotification {
            title: "Breaking signatures detected".to_string(),
            message: format!("{} signature changes may ripple to dependents", ctx.breaking_signatures.len()),
        });
    }
    AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity,
        notifications,
        data: json!({
            "file": ctx.file_path,
            "nodes_changed": added.len() + removed.len() + modified.len(),
            "added": added,
            "removed": removed,
            "modified": modified,
            "breaking_signatures": ctx.breaking_signatures,
        }),
        generated_at: Utc::now(),
    }
}

fn build_intent_predictor_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> AgentReport {
    let related_files = collect_related_files(&ctx.dependency_graph, &ctx.file_path, 8);
    AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity: if ctx.intent_confidence >= 0.8 {
            AgentSeverity::Info
        } else {
            AgentSeverity::Warning
        },
        notifications: vec![AgentNotification {
            title: format!("{} inferred {}", config.name, ctx.intent_type),
            message: format!("confidence {:.0}% • {} related files", ctx.intent_confidence * 100.0, related_files.len()),
        }],
        data: json!({
            "file": ctx.file_path,
            "intent_type": ctx.intent_type,
            "confidence": ctx.intent_confidence,
            "related_files": related_files,
        }),
        generated_at: Utc::now(),
    }
}

fn build_security_scanner_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> Option<AgentReport> {
    let findings = security_findings(&ctx.file_content);
    if findings.is_empty() {
        return None;
    }
    Some(AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity: AgentSeverity::Critical,
        notifications: vec![AgentNotification {
            title: format!("{} flagged risky code", config.name),
            message: format!("{} findings in {}", findings.len(), ctx.file_path),
        }],
        data: json!({
            "file": ctx.file_path,
            "findings": findings,
        }),
        generated_at: Utc::now(),
    })
}

fn build_complexity_watcher_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> Option<AgentReport> {
    let risky = ctx
        .diff
        .nodes_modified
        .iter()
        .chain(ctx.diff.nodes_added.iter())
        .filter(|feature| feature.cyclomatic_complexity > 10)
        .map(|feature| {
            json!({
                "name": feature.name,
                "complexity": feature.cyclomatic_complexity,
                "kind": feature.kind,
            })
        })
        .collect::<Vec<_>>();
    if risky.is_empty() {
        return None;
    }
    Some(AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity: AgentSeverity::Warning,
        notifications: vec![AgentNotification {
            title: format!("{} flagged complex code", config.name),
            message: format!("{} functions above threshold in {}", risky.len(), ctx.file_path),
        }],
        data: json!({
            "file": ctx.file_path,
            "threshold": 10,
            "risky_functions": risky,
        }),
        generated_at: Utc::now(),
    })
}

fn build_test_sentinel_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> Option<AgentReport> {
    if ctx.breaking_signatures.is_empty() && ctx.diff.nodes_modified.is_empty() {
        return None;
    }
    let impacted_tests = ctx
        .dependency_graph
        .edges_in
        .get(&ctx.file_path)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|path| is_test_path(path))
        .collect::<Vec<_>>();
    if impacted_tests.is_empty() {
        return None;
    }
    Some(AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity: AgentSeverity::Warning,
        notifications: vec![AgentNotification {
            title: format!("{} found potentially impacted tests", config.name),
            message: format!("{} test files may require updates", impacted_tests.len()),
        }],
        data: json!({
            "file": ctx.file_path,
            "impacted_tests": impacted_tests,
            "breaking_signatures": ctx.breaking_signatures,
        }),
        generated_at: Utc::now(),
    })
}

fn build_pattern_learner_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> AgentReport {
    let mut patterns = HashSet::new();
    for feature in ctx.diff.nodes_modified.iter().chain(ctx.diff.nodes_added.iter()) {
        for pattern in &feature.pattern_tags {
            patterns.insert(pattern.clone());
        }
    }
    let mut patterns = patterns.into_iter().collect::<Vec<_>>();
    patterns.sort();
    AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity: AgentSeverity::Info,
        notifications: vec![AgentNotification {
            title: format!("{} observed recurring patterns", config.name),
            message: if patterns.is_empty() {
                "No explicit pattern shift detected".to_string()
            } else {
                format!("Observed patterns: {}", patterns.join(", "))
            },
        }],
        data: json!({
            "file": ctx.file_path,
            "observed_patterns": patterns,
            "recent_change_files": ctx.recent_change_files,
        }),
        generated_at: Utc::now(),
    }
}

fn build_documentation_tracker_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> Option<AgentReport> {
    if ctx.breaking_signatures.is_empty() {
        return None;
    }
    Some(AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity: AgentSeverity::Warning,
        notifications: vec![AgentNotification {
            title: format!("{} suspects stale docs", config.name),
            message: format!("Signature changes in {} may require README/file map updates", ctx.file_path),
        }],
        data: json!({
            "file": ctx.file_path,
            "stale_doc_risk": true,
            "signature_changes": ctx.breaking_signatures,
        }),
        generated_at: Utc::now(),
    })
}

fn build_generic_report(config: &AgentConfig, ctx: &FileSaveAgentContext) -> AgentReport {
    AgentReport {
        agent_name: config.name.clone(),
        entry_id: agent_entry_id(&config.name),
        output_key: config.output_key.clone(),
        severity: AgentSeverity::Info,
        notifications: vec![AgentNotification {
            title: format!("{} executed", config.name),
            message: format!("Observed file save in {}", ctx.file_path),
        }],
        data: json!({
            "file": ctx.file_path,
            "scope": config.scope,
            "action_description": config.action_description,
        }),
        generated_at: Utc::now(),
    }
}

fn collect_related_files(graph: &DependencyGraph, active_file: &str, limit: usize) -> Vec<String> {
    let mut files = Vec::new();
    if let Some(outgoing) = graph.edges_out.get(active_file) {
        files.extend(outgoing.iter().cloned());
    }
    if let Some(incoming) = graph.edges_in.get(active_file) {
        files.extend(incoming.iter().cloned());
    }
    files.sort();
    files.dedup();
    files.truncate(limit);
    files
}

/// A single security rule loaded from `memix-security.toml`.
#[derive(Debug, Clone, Deserialize)]
struct SecurityRule {
    id: String,
    pattern: String,
    message: String,
    #[serde(default = "default_severity")]
    severity: String,
}

fn default_severity() -> String {
    "warning".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct SecurityConfig {
    #[serde(default)]
    rules: Vec<SecurityRule>,
}

/// Load security rules from `memix-security.toml`.
/// Search order: workspace root → workspace .memix/ → ~/.memix/ → built-in defaults.
fn load_security_rules(workspace_root: Option<&Path>) -> Vec<SecurityRule> {
    let candidates: Vec<PathBuf> = [
        workspace_root.map(|r| r.join("memix-security.toml")),
        workspace_root.map(|r| r.join(".memix").join("memix-security.toml")),
        dirs::home_dir().map(|h| h.join(".memix").join("memix-security.toml")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in &candidates {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(config) = toml::from_str::<SecurityConfig>(&content) {
                if !config.rules.is_empty() {
                    tracing::info!("Loaded {} security rules from {}", config.rules.len(), path.display());
                    return config.rules;
                }
            }
        }
    }

    // Built-in defaults when no config file is found
    vec![
        SecurityRule {
            id: "hardcoded-secret".to_string(),
            pattern: r#"(?i)(api[_-]?key|secret|token|password)\s*[:=]\s*['"][A-Za-z0-9_\-]{8,}['"]"#.to_string(),
            message: "Potential hardcoded credential".to_string(),
            severity: "critical".to_string(),
        },
        SecurityRule {
            id: "unsafe-eval".to_string(),
            pattern: r"\beval\s*\(|\bexec\s*\(".to_string(),
            message: "Dynamic code execution detected".to_string(),
            severity: "critical".to_string(),
        },
        SecurityRule {
            id: "sql-concatenation".to_string(),
            pattern: r#"(?i)(select|insert|update|delete).*(\\+|format!\(|f\")"#.to_string(),
            message: "Possible SQL string concatenation".to_string(),
            severity: "warning".to_string(),
        },
        SecurityRule {
            id: "env-exposure".to_string(),
            pattern: r"process\.env\.[A-Z0-9_]+|std::env::var\(".to_string(),
            message: "Environment variable access should be reviewed for exposure".to_string(),
            severity: "info".to_string(),
        },
    ]
}

fn security_findings(content: &str) -> Vec<Value> {
    security_findings_with_workspace(content, None)
}

fn security_findings_with_workspace(content: &str, workspace_root: Option<&Path>) -> Vec<Value> {
    let rules = load_security_rules(workspace_root);
    let mut findings = Vec::new();
    for rule in &rules {
        if let Ok(regex) = Regex::new(&rule.pattern) {
            for matched in regex.find_iter(content) {
                findings.push(json!({
                    "id": rule.id,
                    "message": rule.message,
                    "severity": rule.severity,
                    "snippet": content[matched.start()..matched.end()].to_string(),
                }));
            }
        }
    }
    findings
}

fn is_test_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    normalized.contains("/test")
        || normalized.contains("/tests")
        || normalized.contains("/spec")
        || normalized.contains(".test.")
        || normalized.contains(".spec.")
}

fn agent_entry_id(agent_name: &str) -> String {
    let slug = agent_name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '_' })
        .collect::<String>();
    format!("agent_{}.json", slug)
}

fn load_agent_configs(workspace_root: &Path) -> (Vec<AgentConfig>, Option<String>) {
    let candidates = [workspace_root.join("AGENTS.md"), workspace_root.join("A*G*E*N*T*S.m*d")];
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&candidate) {
            let source = Some(candidate.to_string_lossy().to_string());
            let configs = parse_agents_markdown(&content, source.clone());
            return (configs, source);
        }
    }
    (Vec::new(), None)
}

pub fn parse_agents_markdown(content: &str, source_path: Option<String>) -> Vec<AgentConfig> {
    let mut sections = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        if let Some(name) = line.trim().strip_prefix("## Agent:") {
            if let Some(name) = current_name.take() {
                if let Some(config) = parse_agent_section(&name, &current_lines, source_path.clone()) {
                    sections.push(config);
                }
            }
            current_name = Some(name.trim().to_string());
            current_lines.clear();
            continue;
        }
        if current_name.is_some() {
            current_lines.push(line.to_string());
        }
    }

    if let Some(name) = current_name {
        if let Some(config) = parse_agent_section(&name, &current_lines, source_path) {
            sections.push(config);
        }
    }

    sections
}

fn parse_agent_section(name: &str, lines: &[String], source_path: Option<String>) -> Option<AgentConfig> {
    let yaml_lines = extract_yaml_block(lines)?;
    let fields = parse_simple_yaml_map(&yaml_lines);
    Some(AgentConfig {
        name: name.to_string(),
        trigger: parse_trigger(fields.get("trigger").map(String::as_str).unwrap_or("manual")),
        scope: fields.get("scope").cloned().unwrap_or_else(|| "workspace".to_string()),
        action_description: fields.get("action").cloned().unwrap_or_default(),
        output_key: fields
            .get("output")
            .cloned()
            .unwrap_or_else(|| format!("brain:{{project}}:{}", name.to_lowercase())),
        cooldown_ms: parse_cooldown_ms(fields.get("cooldown").map(String::as_str).unwrap_or("0ms")),
        source_path,
    })
}

fn extract_yaml_block(lines: &[String]) -> Option<Vec<String>> {
    let mut in_yaml = false;
    let mut yaml_lines = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "```yaml" {
            in_yaml = true;
            continue;
        }
        if in_yaml && trimmed == "```" {
            break;
        }
        if in_yaml {
            yaml_lines.push(line.to_string());
        }
    }
    if yaml_lines.is_empty() {
        None
    } else {
        Some(yaml_lines)
    }
}

fn parse_simple_yaml_map(lines: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].trim_end();
        if line.trim().is_empty() {
            index += 1;
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            index += 1;
            continue;
        };
        let key = raw_key.trim().to_string();
        let value = raw_value.trim();
        if value == "|" {
            index += 1;
            let mut block = Vec::new();
            while index < lines.len() {
                let candidate = &lines[index];
                if candidate.starts_with(' ') || candidate.starts_with('\t') {
                    block.push(candidate.trim().to_string());
                    index += 1;
                    continue;
                }
                break;
            }
            out.insert(key, block.join("\n"));
            continue;
        }
        out.insert(key, value.to_string());
        index += 1;
    }
    out
}

fn parse_trigger(value: &str) -> AgentTrigger {
    let normalized = value.trim().to_lowercase();
    if normalized.contains("file_save") {
        AgentTrigger::FileSave
    } else if normalized.contains("file_open") {
        AgentTrigger::FileOpen
    } else if normalized.contains("function_signature_change") {
        AgentTrigger::FunctionSignatureChange
    } else if normalized.contains("session_start") {
        AgentTrigger::SessionStart
    } else if normalized.contains("git_commit") {
        AgentTrigger::GitCommit
    } else if normalized.contains("manual") {
        AgentTrigger::Manual
    } else if normalized.starts_with("every ") {
        AgentTrigger::Interval {
            seconds: parse_interval_seconds(&normalized).unwrap_or(60),
        }
    } else {
        AgentTrigger::Unknown(value.to_string())
    }
}

fn parse_interval_seconds(value: &str) -> Option<u64> {
    let regex = Regex::new(r"every\s+(\d+)\s+(second|seconds|minute|minutes|hour|hours)").ok()?;
    let captures = regex.captures(value)?;
    let count = captures.get(1)?.as_str().parse::<u64>().ok()?;
    let unit = captures.get(2)?.as_str();
    Some(match unit {
        "second" | "seconds" => count,
        "minute" | "minutes" => count.saturating_mul(60),
        "hour" | "hours" => count.saturating_mul(3600),
        _ => count,
    })
}

fn parse_cooldown_ms(value: &str) -> u64 {
    let normalized = value.trim().to_lowercase();
    if let Ok(regex) = Regex::new(r"^(\d+)(ms|s|m|h)$") {
        if let Some(captures) = regex.captures(&normalized) {
            let amount = captures
                .get(1)
                .and_then(|m| m.as_str().parse::<u64>().ok())
                .unwrap_or(0);
            let unit = captures.get(2).map(|m| m.as_str()).unwrap_or("ms");
            return match unit {
                "ms" => amount,
                "s" => amount.saturating_mul(1000),
                "m" => amount.saturating_mul(60_000),
                "h" => amount.saturating_mul(3_600_000),
                _ => amount,
            };
        }
    }
    parse_interval_seconds(&normalized)
        .map(|seconds| seconds.saturating_mul(1000))
        .unwrap_or(0)
}
