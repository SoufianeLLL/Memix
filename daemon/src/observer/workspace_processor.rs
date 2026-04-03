// Per-workspace observer state and processing logic.
// Each workspace has its own cache, feature snapshots, pending path, and detectors.
// The shared event loop dispatches TaggedEvents to the correct WorkspaceProcessor.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;

use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};
use crate::git::archaeologist::{GitArchaeologist, ProjectGitInsights};
use crate::intelligence::autonomous::AutonomousPairProgrammer;
use crate::intelligence::intent::IntentEngine;
use crate::intelligence::predictor::ContextPredictor;
use crate::observer::call_graph::CallGraph;
use crate::observer::decisions::{DecisionDetector, DecisionSignal};
use crate::observer::differ::AstDiffer;
use crate::observer::dna::{DnaRuleConfig, ProjectCodeDna};
use crate::observer::imports::{extract_imports, signature_head};
use crate::observer::parser::{AstNodeFeature, AstParser};
use crate::observer::skeleton::FileSkeleton;
use crate::recorder::flight::{FlightRecorder, SessionEvent};
use crate::storage::StorageBackend;
use crate::token::engine::TokenEngine;

/// Per-workspace state for the observer pipeline
pub struct WorkspaceProcessor {
    pub project_id: String,
    pub workspace_root: PathBuf,
    pub pending_path: Option<PathBuf>,
    pub pending_ack_path: Option<PathBuf>,
    
    /// AST cache: file path -> (bytes, tree)
    pub cache: HashMap<String, (Vec<u8>, Option<(tree_sitter::Tree, tree_sitter::Language)>)>,
    
    /// Feature snapshots for skeleton building
    pub feature_snapshots: HashMap<String, Vec<AstNodeFeature>>,
    
    /// Recently deleted files for known_issues tracking
    pub recent_deleted_files: VecDeque<String>,
    
    /// Decision detector for auto-decisions
    pub decision_detector: DecisionDetector,
    
    /// Git archaeologist for this workspace (wrapped for Send+Sync)
    pub archaeologist: Arc<tokio::sync::Mutex<Option<GitArchaeologist>>>,
    
    /// Last persist timestamps (throttling)
    pub last_observer_persist: std::time::Instant,
    pub last_fsi_persist: std::time::Instant,
}

impl WorkspaceProcessor {
    pub fn new(project_id: String, workspace_root: PathBuf) -> Self {
        // Find git root for this workspace
        let git_root = {
            let mut dir = workspace_root.clone();
            let mut found: Option<PathBuf> = None;
            loop {
                if dir.join(".git").exists() {
                    found = Some(dir.clone());
                    break;
                }
                if !dir.pop() {
                    break;
                }
            }
            found
        };
        
        let archaeologist = git_root.as_ref().and_then(|root| GitArchaeologist::open(root).ok());
        
        let pending_path = workspace_root
            .join(".memix")
            .join("brain")
            .join("pending.json");
        let pending_path = if pending_path.parent().map(|p| p.exists()).unwrap_or(false) {
            Some(pending_path)
        } else {
            None
        };
        
        let pending_ack_path = workspace_root
            .join(".memix")
            .join("brain")
            .join("pending.ack.json");
        
        Self {
            project_id,
            workspace_root,
            pending_path,
            pending_ack_path: Some(pending_ack_path),
            cache: HashMap::new(),
            feature_snapshots: HashMap::new(),
            recent_deleted_files: VecDeque::with_capacity(32),
            decision_detector: DecisionDetector::new(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rules")
            ),
            archaeologist: Arc::new(tokio::sync::Mutex::new(archaeologist)),
            last_observer_persist: std::time::Instant::now(),
            last_fsi_persist: std::time::Instant::now(),
        }
    }
    
    /// Check if brain is paused for this workspace
    pub fn is_paused(&self, config: &crate::server::DaemonConfig) -> bool {
        config.brain_paused
    }
    
    /// Process a file deletion event
    pub async fn process_deletion(
        &mut self,
        path: &std::path::Path,
        storage: &Arc<dyn StorageBackend + Send + Sync>,
        autonomous: &Arc<tokio::sync::Mutex<AutonomousPairProgrammer>>,
        recorder: &Arc<FlightRecorder>,
        call_graph: &Arc<tokio::sync::Mutex<CallGraph>>,
    ) {
        if path.exists() {
            return;
        }
        
        let key = path.to_string_lossy().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        if !AstParser::is_supported(ext) && !self.cache.contains_key(&key) && !self.feature_snapshots.contains_key(&key) {
            return;
        }
        
        self.cache.remove(&key);
        self.feature_snapshots.remove(&key);
        
        {
            let mut autonomous = autonomous.lock().await;
            autonomous.dependency_graph.remove_file(&key);
        }
        
        while self.recent_deleted_files.len() >= 32 {
            self.recent_deleted_files.pop_front();
        }
        self.recent_deleted_files.push_back(key.clone());
        
        recorder.record_event(SessionEvent::AstMutation { file: key.clone(), nodes_changed: 0 });
        
        // Skeleton cleanup
        call_graph.lock().await.remove_file(&key);
        let fsi_id = crate::observer::skeleton::file_skeleton_id(&key);
        let _ = storage.delete_skeleton_entry(&self.project_id, &fsi_id).await;
        
        let fusi_prefix = format!("fusi::{}", crate::observer::skeleton::normalize_path(&key));
        if let Ok(entries) = storage.get_skeleton_entries(&self.project_id).await {
            for entry in entries {
                if entry.id.starts_with(&fusi_prefix) {
                    let _ = storage.delete_skeleton_entry(&self.project_id, &entry.id).await;
                }
            }
        }
        
        tracing::debug!("Observer removed file from live graph: {}", key);
    }
    
    /// Process a file modification event
    #[allow(clippy::too_many_arguments)]
    pub async fn process_modification(
        &mut self,
        path: &std::path::Path,
        parser: &mut AstParser,
        storage: &Arc<dyn StorageBackend + Send + Sync>,
        autonomous: &Arc<tokio::sync::Mutex<AutonomousPairProgrammer>>,
        recorder: &Arc<FlightRecorder>,
        predictor: &Arc<ContextPredictor>,
        call_graph: &Arc<tokio::sync::Mutex<CallGraph>>,
        agent_runtime: &Arc<tokio::sync::Mutex<crate::agents::AgentRuntime>>,
        code_dna: &Arc<tokio::sync::Mutex<ProjectCodeDna>>,
        git_insights: &Arc<tokio::sync::Mutex<ProjectGitInsights>>,
        config: &crate::server::DaemonConfig,
    ) {
        if !path.is_file() {
            return;
        }
        
        let key = path.to_string_lossy().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        // Check for package.json changes
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name == "package.json" {
            self.process_package_json(&key, storage).await;
        }
        
        if !AstParser::is_supported(ext) {
            return;
        }
        
        let new_bytes = match tokio::fs::read(path).await {
            Ok(b) => b,
            Err(_) => return,
        };
        
        let new_tree = match parser.parse_file(path) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("AST parse failed for {:?}: {}", path, e);
                return;
            }
        };
        let Some(new_tree) = new_tree else { return; };
        
        let (old_bytes, old_tree) = self.cache
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (Vec::new(), None));
        
        let diff = AstDiffer::compute_diff(
            &key,
            parser,
            old_tree.as_ref(),
            &new_tree,
            &old_bytes,
            &new_bytes,
            ext,
        );
        
        let new_features = parser.extract_features(&new_tree.0, new_tree.1.clone(), &new_bytes, ext);
        let new_features_for_agents = new_features.clone();
        
        // Detect breaking signatures
        let breaking_signatures = self.detect_breaking_signatures(&old_tree, &old_bytes, &new_features, parser, ext);
        
        self.cache.insert(key.clone(), (new_bytes.clone(), Some(new_tree)));
        self.feature_snapshots.insert(key.clone(), new_features.clone());
        
        let nodes_changed = diff.nodes_added.len() + diff.nodes_removed.len() + diff.nodes_modified.len();
        recorder.record_event(SessionEvent::AstMutation { file: key.clone(), nodes_changed });
        predictor.record_activity(&key, nodes_changed).await;
        
        let intent = IntentEngine::classify_intent(&diff);
        let intent_confidence = IntentEngine::confidence(&diff);
        recorder.record_event(SessionEvent::IntentDetected {
            intent_type: intent.as_str().to_string(),
        });
        
        // Generate breaking signature warnings
        self.emit_breaking_signature_warnings(&key, &breaking_signatures, &old_bytes, storage).await;
        
        let source_code = String::from_utf8_lossy(&new_bytes).to_string();
        
        // OXC analysis
        let oxc_analysis = self.run_oxc_analysis(&key, &source_code, ext);
        let imports = self.extract_imports_with_oxc(&key, &source_code, ext, &oxc_analysis, storage).await;
        
        // Skeleton persistence (throttled)
        if self.last_fsi_persist.elapsed() >= std::time::Duration::from_secs(Self::fsi_debounce_secs()) {
            self.last_fsi_persist = std::time::Instant::now();
            
            if let Some(features) = self.feature_snapshots.get(&key) {
                self.persist_skeleton(&key, features, &new_bytes, storage, autonomous, call_graph, &oxc_analysis).await;
            }
        }
        
        // Update session_state.json
        self.update_session_state(&key, storage).await;
        
        // Update dependency graph and run agents
        let (intent_entry_json, related_files, graph_snapshot) = {
            let mut a = autonomous.lock().await;
            let local_imports: Vec<String> = imports.into_iter()
                .filter(|imp| imp.contains('/') || imp.contains('\\'))
                .collect();
            a.update_dependency_graph(&key, &local_imports);
            
            let related_files = self.get_related_files(&a, &key);
            
            // Preload context
            let rationale = vec![
                format!("intent={} confidence={:.2}", intent.as_str(), intent_confidence),
                format!("related_files={}", related_files.len()),
                format!("nodes_changed={}", nodes_changed),
            ];
            
            let token_weight = TokenEngine::count_tokens(&format!(
                "{}\n{}\n{}",
                key,
                related_files.join("\n"),
                rationale.join("\n")
            )).unwrap_or(0);
            
            predictor.preload_context(
                &key,
                vec![
                    "observerGraph".to_string(),
                    "observerChanges".to_string(),
                    "fileMap".to_string(),
                    "knownIssues".to_string(),
                ],
                related_files.clone(),
                token_weight,
                intent.as_str().to_string(),
                intent_confidence,
                rationale.clone(),
            ).await;
            
            let intent_entry_json = predictor.get_cached_context(&key).await
                .and_then(|snapshot| serde_json::to_string_pretty(&snapshot).ok());
            
            a.record_change(key.clone(), diff.clone());
            
            (intent_entry_json, related_files, a.dependency_graph.clone())
        };
        
        // Run file-save agents
        let recent_change_files = {
            let a = autonomous.lock().await;
            a.change_history.iter().rev().take(20).map(|c| c.file.clone()).collect::<Vec<_>>()
        };
        
        let reports = {
            let mut runtime = agent_runtime.lock().await;
            runtime.process_file_save(&crate::agents::FileSaveAgentContext {
                project_id: self.project_id.clone(),
                file_path: key.clone(),
                file_content: String::from_utf8_lossy(&new_bytes).to_string(),
                diff: diff.clone(),
                features: new_features_for_agents,
                dependency_graph: graph_snapshot.clone(),
                intent_type: intent.as_str().to_string(),
                intent_confidence,
                breaking_signatures: breaking_signatures.clone(),
                recent_change_files,
            })
        };
        
        // Persist agent reports
        for report in reports {
            let kind = if report.severity >= crate::agents::AgentSeverity::Warning {
                MemoryKind::Warning
            } else {
                MemoryKind::Context
            };
            let entry = MemoryEntry {
                id: report.entry_id.clone(),
                project_id: self.project_id.clone(),
                kind,
                content: serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
                tags: vec!["agent".to_string(), report.agent_name.to_lowercase(), intent.as_str().to_string()],
                source: MemorySource::AgentExtracted,
                superseded_by: None,
                contradicts: vec![],
                parent_id: None,
                caused_by: vec![],
                enables: related_files.clone(),
                created_at: report.generated_at,
                updated_at: report.generated_at,
                access_count: 0,
                last_accessed_at: None,
            };
            let _ = storage.upsert_entry(&self.project_id, entry).await;
        }
        
        // Persist observer snapshots (throttled)
        if self.last_observer_persist.elapsed() >= std::time::Duration::from_secs(2) {
            self.last_observer_persist = std::time::Instant::now();
            self.persist_observer_snapshots(
                storage, autonomous, code_dna, git_insights, agent_runtime, &intent_entry_json
            ).await;
        }
    }
    
    fn detect_breaking_signatures(
        &self,
        old_tree: &Option<(tree_sitter::Tree, tree_sitter::Language)>,
        old_bytes: &[u8],
        new_features: &[AstNodeFeature],
        parser: &AstParser,
        ext: &str,
    ) -> Vec<(String, String, String)> {
        let mut breaking = Vec::new();
        
        if let Some((old_tree_ref, old_lang)) = old_tree {
            let old_features = parser.extract_features(old_tree_ref, old_lang.clone(), old_bytes, ext);
            let old_map: HashMap<String, AstNodeFeature> = old_features
                .into_iter()
                .map(|f| (f.name.clone(), f))
                .collect();
            
            for nf in new_features {
                if let Some(of) = old_map.get(&nf.name) {
                    let old_sig = signature_head(&of.body);
                    let new_sig = signature_head(&nf.body);
                    if !old_sig.is_empty() && !new_sig.is_empty() && old_sig != new_sig {
                        breaking.push((nf.name.clone(), old_sig, new_sig));
                    }
                }
            }
        }
        
        breaking
    }
    
    async fn emit_breaking_signature_warnings(
        &self,
        key: &str,
        breaking_signatures: &[(String, String, String)],
        old_bytes: &[u8],
        storage: &Arc<dyn StorageBackend + Send + Sync>,
    ) {
        let genuinely_breaking: Vec<_> = breaking_signatures.iter()
            .filter(|(_, old_sig, new_sig)| {
                let normalize = |s: &str| -> String {
                    s.trim()
                        .trim_start_matches("pub(crate)")
                        .trim_start_matches("pub")
                        .trim_start_matches("async")
                        .trim_start_matches("impl")
                        .trim()
                        .to_string()
                };
                normalize(old_sig) != normalize(new_sig)
            })
            .collect();
        
        if !old_bytes.is_empty() && !genuinely_breaking.is_empty() {
            let now = Utc::now();
            let details = breaking_signatures.iter()
                .map(|(name, old_sig, new_sig)| format!("- {}: '{}' -> '{}'", name, old_sig, new_sig))
                .collect::<Vec<_>>()
                .join("\n");
            
            let entry = MemoryEntry {
                id: format!("warning_signature_{}_{}.json", now.timestamp_millis(), uuid::Uuid::new_v4()),
                project_id: self.project_id.clone(),
                kind: MemoryKind::Warning,
                content: format!("Potential breaking signature change detected in {}:\n{}", key, details),
                tags: vec!["warning".to_string(), "semantic-diff".to_string(), "signature-change".to_string()],
                source: MemorySource::FileWatcher,
                superseded_by: None,
                contradicts: vec![],
                parent_id: None,
                caused_by: vec![],
                enables: vec![],
                created_at: now,
                updated_at: now,
                access_count: 0,
                last_accessed_at: None,
            };
            let _ = storage.upsert_entry(&self.project_id, entry).await;
        }
    }
    
    fn run_oxc_analysis(&self, key: &str, source_code: &str, ext: &str) -> Option<crate::observer::oxc_semantic::OxcAnalysis> {
        let is_oxc_supported = crate::observer::oxc_semantic::is_oxc_supported(ext);
        if is_oxc_supported && std::env::var("MEMIX_OXC_ENABLED").unwrap_or_else(|_| "true".to_string()) == "true" {
            crate::observer::oxc_semantic::analyze_file(
                std::path::Path::new(key),
                source_code,
                Some(&self.workspace_root),
            )
        } else {
            None
        }
    }
    
    #[allow(clippy::too_many_arguments)]
    async fn extract_imports_with_oxc(
        &self,
        key: &str,
        source_code: &str,
        ext: &str,
        oxc_analysis: &Option<crate::observer::oxc_semantic::OxcAnalysis>,
        storage: &Arc<dyn StorageBackend + Send + Sync>,
    ) -> Vec<String> {
        if let Some(ref analysis) = oxc_analysis {
            let unresolved_relative: Vec<String> = analysis.resolved_imports.iter()
                .filter(|i| i.resolved_path.is_none() && (i.specifier.starts_with('.') || i.specifier.starts_with('/')))
                .map(|i| i.specifier.clone())
                .collect();
            
            if !unresolved_relative.is_empty() {
                let now = Utc::now();
                let entry = MemoryEntry {
                    id: format!("warning_dead_import_{}_{}.json", now.timestamp_millis(), uuid::Uuid::new_v4()),
                    project_id: self.project_id.clone(),
                    kind: MemoryKind::Warning,
                    content: format!(
                        "Unresolved import(s) detected in {}:\n{}",
                        key,
                        unresolved_relative.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n")
                    ),
                    tags: vec!["warning".to_string(), "oxc".to_string(), "dead-import".to_string()],
                    source: MemorySource::FileWatcher,
                    superseded_by: None,
                    contradicts: vec![],
                    parent_id: None,
                    caused_by: vec![],
                    enables: vec![],
                    created_at: now,
                    updated_at: now,
                    access_count: 0,
                    last_accessed_at: None,
                };
                let _ = storage.upsert_entry(&self.project_id, entry).await;
            }
            
            analysis.resolved_imports.iter()
                .map(|i| i.resolved_path.clone().unwrap_or_else(|| i.specifier.clone()))
                .collect()
        } else {
            extract_imports(ext, source_code)
        }
    }
    
    #[allow(clippy::too_many_arguments)]
    async fn persist_skeleton(
        &self,
        key: &str,
        features: &[AstNodeFeature],
        new_bytes: &[u8],
        storage: &Arc<dyn StorageBackend + Send + Sync>,
        autonomous: &Arc<tokio::sync::Mutex<AutonomousPairProgrammer>>,
        call_graph: &Arc<tokio::sync::Mutex<CallGraph>>,
        oxc_analysis: &Option<crate::observer::oxc_semantic::OxcAnalysis>,
    ) {
        let call_symbols: Vec<(String, Vec<crate::observer::call_graph::ResolvedEdge>)> = if let Some(ref analysis) = oxc_analysis {
            let mut oxc_calls: HashMap<String, Vec<crate::observer::call_graph::ResolvedEdge>> = HashMap::new();
            for call in &analysis.calls {
                oxc_calls.entry(call.caller_fn.clone()).or_default().push(crate::observer::call_graph::ResolvedEdge {
                    callee_file: call.callee_file.clone().unwrap_or_default(),
                    callee_symbol: call.callee_symbol.clone().unwrap_or_else(|| call.callee_expr.clone()),
                    callee_line: call.callee_line.unwrap_or(call.line),
                    is_method: call.is_method,
                });
            }
            oxc_calls.into_iter().collect()
        } else {
            features.iter()
                .filter(|f| matches!(f.kind.as_str(), "function" | "method" | "constructor"))
                .map(|f| (f.name.clone(), f.calls.iter().map(|s| crate::observer::call_graph::ResolvedEdge::new_unresolved(s)).collect()))
                .collect()
        };
        
        call_graph.lock().await.update_file(key, call_symbols);
        
        let dep_graph_snapshot = {
            let a = autonomous.lock().await;
            a.dependency_graph.clone()
        };
        
        let skeleton = FileSkeleton::build(
            key,
            features,
            &dep_graph_snapshot,
            &String::from_utf8_lossy(new_bytes),
        );
        
        let fsi_entry = skeleton.to_memory_entry(&self.project_id);
        if let Err(e) = storage.upsert_skeleton_entry(&self.project_id, fsi_entry).await {
            tracing::warn!("Skeleton: FSI upsert failed for {}: {}", key, e);
        }
        
        // FuSI for hot files
        let is_hot = self.is_hot_file(key, &dep_graph_snapshot);
        if is_hot {
            let call_graph_snapshot = call_graph.lock().await;
            let symbol_entries = skeleton.to_symbol_entries(&self.project_id, &call_graph_snapshot);
            for entry in symbol_entries {
                if let Err(e) = storage.upsert_skeleton_entry(&self.project_id, entry).await {
                    tracing::warn!("Skeleton: FuSI upsert failed: {}", e);
                }
            }
        }
        
        tracing::debug!("Skeleton: FSI persisted for {} (hot={})", key, is_hot);
    }
    
    async fn update_session_state(&self, key: &str, storage: &Arc<dyn StorageBackend + Send + Sync>) {
        if let Ok(existing) = storage.get_entry(&self.project_id, "session_state.json").await {
            let mut state: serde_json::Value = if !existing.content.is_empty() {
                serde_json::from_str(&existing.content).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({
                    "session_number": 1,
                    "current_task": "Development in progress",
                    "progress": [],
                    "modified_files": [],
                    "last_updated": chrono::Utc::now().to_rfc3339()
                })
            };
            
            if let Some(files) = state.get_mut("modified_files") {
                if let Some(arr) = files.as_array_mut() {
                    let key_val = serde_json::json!(key);
                    if !arr.contains(&key_val) {
                        arr.push(key_val);
                    }
                }
            }
            state["last_updated"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            
            let entry = MemoryEntry {
                id: "sessionState".to_string(),
                project_id: self.project_id.clone(),
                kind: MemoryKind::Context,
                content: serde_json::to_string(&state).unwrap_or_default(),
                tags: vec!["session".to_string()],
                source: MemorySource::AgentExtracted,
                superseded_by: None,
                contradicts: vec![],
                parent_id: None,
                caused_by: vec![],
                enables: vec![],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                access_count: 0,
                last_accessed_at: None,
            };
            let _ = storage.upsert_entry(&self.project_id, entry).await;
        }
    }
    
    fn get_related_files(
        &self,
        autonomous: &AutonomousPairProgrammer,
        key: &str,
    ) -> Vec<String> {
        let mut files = Vec::new();
        if let Some(deps) = autonomous.dependency_graph.edges_out.get(key) {
            files.extend(deps.iter().cloned());
        }
        if let Some(deps) = autonomous.dependency_graph.edges_in.get(key) {
            files.extend(deps.iter().cloned());
        }
        files.sort();
        files.dedup();
        files.truncate(8);
        files
    }
    
    #[allow(clippy::too_many_arguments)]
    async fn persist_observer_snapshots(
        &mut self,
        storage: &Arc<dyn StorageBackend + Send + Sync>,
        autonomous: &Arc<tokio::sync::Mutex<AutonomousPairProgrammer>>,
        code_dna: &Arc<tokio::sync::Mutex<ProjectCodeDna>>,
        git_insights: &Arc<tokio::sync::Mutex<ProjectGitInsights>>,
        agent_runtime: &Arc<tokio::sync::Mutex<crate::agents::AgentRuntime>>,
        intent_entry_json: &Option<String>,
    ) {
        let recent_reports = {
            let runtime = agent_runtime.lock().await;
            runtime.recent_reports()
        };
        
        let (graph_json, changes_json, dna_json, dna_snapshot, git_json, git_snapshot, file_map_json, known_issues_json) = {
            let a = autonomous.lock().await;
            let graph_json = serde_json::to_string_pretty(&a.dependency_graph).unwrap_or_else(|_| "{}".to_string());
            
            let changes: Vec<_> = a.change_history.iter().rev().take(25).map(|c| c.diff.clone()).collect();
            let changes_json = serde_json::to_string_pretty(&changes).unwrap_or_else(|_| "[]".to_string());
            
            let recent_change_files = a.change_history.iter().rev().take(50).map(|c| c.file.clone()).collect::<Vec<_>>();
            
            let dna_rules = DnaRuleConfig::resolve_for_workspace(&self.workspace_root);
            
            let tracked_git_files = recent_change_files.iter().cloned()
                .chain(self.feature_snapshots.keys().take(12).cloned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            
            // Lock archaeologist to get git insights
            let snapshot = {
                let arch_lock = self.archaeologist.lock().await;
                if let Some(arch) = arch_lock.as_ref() {
                    arch.project_insights(&tracked_git_files, 75).ok()
                } else {
                    None
                }
            };
            let git_json = snapshot.as_ref().and_then(|s| serde_json::to_string_pretty(s).ok());
            let (git_json, git_snapshot) = (git_json, snapshot);
            
            let snapshot = ProjectCodeDna::summarize(
                &self.feature_snapshots,
                &a.dependency_graph,
                &recent_change_files,
                &dna_rules,
            );
            let dna_json = serde_json::to_string_pretty(&snapshot).ok();
            let (dna_json, dna_snapshot) = (dna_json, Some(snapshot));
            
            let file_map_json = serde_json::to_string_pretty(&Self::build_file_map_snapshot(&self.feature_snapshots, &a.dependency_graph))
                .unwrap_or_else(|_| "{}".to_string());
            
            let known_issues_json = serde_json::to_string_pretty(&Self::build_known_issues_snapshot(&recent_reports, &self.recent_deleted_files))
                .unwrap_or_else(|_| "[]".to_string());
            
            (Some(graph_json), Some(changes_json), dna_json, dna_snapshot, git_json, git_snapshot, Some(file_map_json), Some(known_issues_json))
        };
        
        // Update shared state
        if let Some(dna_snapshot) = dna_snapshot {
            *code_dna.lock().await = dna_snapshot;
        }
        if let Some(git_snapshot) = git_snapshot {
            *git_insights.lock().await = git_snapshot;
        }
        
        // Persist to storage
        if let (Some(graph_json), Some(changes_json), Some(dna_json), Some(file_map_json), Some(known_issues_json)) = 
            (graph_json, changes_json, dna_json, file_map_json, known_issues_json) 
        {
            let entries = vec![
                ("observerGraph", graph_json, vec!["observer".to_string(), "graph".to_string()]),
                ("observerChanges", changes_json, vec!["observer".to_string(), "changes".to_string()]),
                ("observerDna", dna_json, vec!["observer".to_string(), "dna".to_string(), "architecture".to_string()]),
                ("fileMap", file_map_json, vec!["observer".to_string(), "fileMap".to_string(), "generated".to_string()]),
                ("knownIssues", known_issues_json, vec!["observer".to_string(), "knownIssues".to_string(), "generated".to_string()]),
            ];
            
            for (id, content, tags) in entries {
                let entry = MemoryEntry {
                    id: id.to_string(),
                    project_id: self.project_id.clone(),
                    kind: MemoryKind::Context,
                    content,
                    tags,
                    source: MemorySource::FileWatcher,
                    superseded_by: None,
                    contradicts: vec![],
                    parent_id: None,
                    caused_by: vec![],
                    enables: vec![],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    access_count: 0,
                    last_accessed_at: None,
                };
                let _ = storage.upsert_entry(&self.project_id, entry).await;
            }
            
            // Intent entry
            if let Some(intent_json) = intent_entry_json {
                let entry = MemoryEntry {
                    id: "observerIntent".to_string(),
                    project_id: self.project_id.clone(),
                    kind: MemoryKind::Context,
                    content: intent_json.clone(),
                    tags: vec!["observer".to_string(), "intent".to_string(), "predictive".to_string()],
                    source: MemorySource::FileWatcher,
                    superseded_by: None,
                    contradicts: vec![],
                    parent_id: None,
                    caused_by: vec![],
                    enables: vec![],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    access_count: 0,
                    last_accessed_at: None,
                };
                let _ = storage.upsert_entry(&self.project_id, entry).await;
            }
            
            // Git entry
            if let Some(git_json) = git_json {
                let entry = MemoryEntry {
                    id: "observerGit".to_string(),
                    project_id: self.project_id.clone(),
                    kind: MemoryKind::Context,
                    content: git_json,
                    tags: vec!["observer".to_string(), "git".to_string(), "archaeology".to_string()],
                    source: MemorySource::GitArchaeology,
                    superseded_by: None,
                    contradicts: vec![],
                    parent_id: None,
                    caused_by: vec![],
                    enables: vec![],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    access_count: 0,
                    last_accessed_at: None,
                };
                let _ = storage.upsert_entry(&self.project_id, entry).await;
            }
        }
    }
    
    async fn process_package_json(&mut self, key: &str, storage: &Arc<dyn StorageBackend + Send + Sync>) {
        let new_deps = crate::observer::decisions::detect_new_dependencies(&self.workspace_root, self.decision_detector.get_previous_deps());
        
        for (name, version) in new_deps {
            let signal = DecisionSignal::DependencyAdded {
                name: name.clone(),
                version: version.clone(),
                file: key.to_string(),
            };
            
            let decisions = self.decision_detector.process_signal(signal).await;
            
            for decision in decisions {
                let existing = storage.get_entry(&self.project_id, "decisions.json").await;
                let mut arr: Vec<serde_json::Value> = match &existing {
                    Ok(e) if !e.content.is_empty() => serde_json::from_str(&e.content).unwrap_or_default(),
                    _ => vec![],
                };
                
                arr.push(serde_json::json!({
                    "id": decision.id,
                    "title": decision.title,
                    "rationale": decision.rationale,
                    "alternatives": decision.alternatives,
                    "evidence": decision.evidence,
                    "confidence": decision.confidence,
                    "rule_id": decision.rule_id,
                    "triggered_by": decision.triggered_by,
                    "tags": decision.tags,
                    "created_at": decision.created_at,
                }));
                
                let entry = MemoryEntry {
                    id: "decisions".to_string(),
                    project_id: self.project_id.clone(),
                    kind: MemoryKind::Decision,
                    content: serde_json::to_string(&arr).unwrap_or_default(),
                    tags: vec!["decisions".to_string()],
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
                };
                
                match storage.upsert_entry(&self.project_id, entry).await {
                    Ok(_) => {
                        tracing::info!("Auto-decision recorded: {}", decision.title);
                        self.decision_detector.mark_recorded(decision.id.clone()).await;
                    },
                    Err(e) => tracing::warn!("Failed to save auto-decision: {}", e),
                }
            }
        }
    }
    
    fn is_hot_file(&self, file_path: &str, graph: &crate::observer::graph::DependencyGraph) -> bool {
        if self.feature_snapshots.contains_key(file_path) {
            if self.feature_snapshots.len() <= Self::max_hot_files() {
                return true;
            }
        }
        if let Some(deps) = graph.edges_in.get(file_path) {
            if deps.len() >= 3 {
                return true;
            }
        }
        false
    }
    
    fn max_hot_files() -> usize {
        std::env::var("MEMIX_MAX_HOT_FILES").ok().and_then(|s| s.parse().ok()).unwrap_or(30)
    }
    
    fn fsi_debounce_secs() -> u64 {
        std::env::var("MEMIX_FSI_DEBOUNCE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
    }
    
    fn build_file_map_snapshot(
        feature_snapshots: &HashMap<String, Vec<AstNodeFeature>>,
        graph: &crate::observer::graph::DependencyGraph,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        
        for (file, features) in feature_snapshots.iter().take(50) {
            let deps_out = graph.edges_out.get(file).map(|d| d.iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
            let deps_in = graph.edges_in.get(file).map(|d| d.iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
            
            map.insert(file.clone(), serde_json::json!({
                "symbols": features.iter().map(|f| &f.name).take(20).collect::<Vec<_>>(),
                "imports_count": deps_out.len(),
                "imported_by_count": deps_in.len(),
            }));
        }
        
        serde_json::Value::Object(map)
    }
    
    fn build_known_issues_snapshot(
        reports: &[crate::agents::AgentReport],
        recent_deleted_files: &VecDeque<String>,
    ) -> serde_json::Value {
        let mut issues = Vec::new();
        
        for file in recent_deleted_files.iter().rev().take(10) {
            issues.push(serde_json::json!({
                "issue": format!("Recently deleted file observed: {}", file),
                "file": file,
                "severity": "warning",
                "source": "observer",
                "recommendations": vec!["Verify this deletion was intentional and update dependent files if needed."],
            }));
        }
        
        for report in reports.iter().rev().take(64) {
            if report.severity >= crate::agents::AgentSeverity::Warning {
                let file = report.data.get("file").and_then(|v| v.as_str()).map(|s| s.to_string());
                let severity = match report.severity {
                    crate::agents::AgentSeverity::Critical => "critical",
                    crate::agents::AgentSeverity::Warning => "warning",
                    crate::agents::AgentSeverity::Info => "info",
                };
                
                issues.push(serde_json::json!({
                    "issue": format!("{} reported a warning", report.agent_name),
                    "file": file,
                    "severity": severity,
                    "source": report.agent_name,
                    "recommendations": report.notifications.iter().map(|n| n.message.clone()).collect::<Vec<_>>(),
                }));
            }
        }
        
        issues.truncate(40);
        serde_json::Value::Array(issues)
    }
}
