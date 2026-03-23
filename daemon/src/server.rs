use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use crate::agents::AgentRuntime;
use crate::brain::validator::BrainValidator;
use crate::brain::hierarchy::{BrainHierarchy, HierarchyLayer};
use crate::brain::manager::BrainManager;
use crate::brain::schema::MemoryEntry;
use crate::context::{CompileRequest, ContextCompiler};
use crate::learning::{CrossProjectLearner, PromptOptimizer, PromptRecord};
use crate::license::{LicenseInitiateResult, LicensePendingResult, LicenseTier, LicenseValidator};
use crate::storage::StorageBackend;
use crate::token::engine::TokenEngine;
use crate::rules::{RulesEngine, IdeType};
use crate::intelligence::autonomous::{AutonomousPairProgrammer, ImpactAnalysis, PredictedQuestion};
use crate::intelligence::proactive::ProactiveWarner;
use crate::intelligence::predictor::ContextPredictor;
use crate::migrations;
use crate::git::archaeologist::ProjectGitInsights;
use crate::observer::differ::SemanticDiff;
use crate::observer::call_graph::{CallGraph, FileCausalContext};
use crate::observer::dna::ProjectCodeDna;
use crate::observer::graph::DependencyGraph;
use crate::observer::patterns::PatternEngine;
use crate::observer::importance::{compute_importance, compute_blast_radius};
use crate::recorder::flight::{FlightRecorder, FlightRecord};

pub struct AppState {
    pub storage: Arc<dyn StorageBackend + Send + Sync>,
	pub autonomous: Arc<tokio::sync::Mutex<AutonomousPairProgrammer>>,
	pub recorder: Arc<FlightRecorder>,
	pub code_dna: Arc<tokio::sync::Mutex<ProjectCodeDna>>,
	pub predictor: Arc<ContextPredictor>,
	pub call_graph: Arc<tokio::sync::Mutex<CallGraph>>,
	pub agent_runtime: Arc<tokio::sync::Mutex<AgentRuntime>>,
	pub git_insights: Arc<tokio::sync::Mutex<ProjectGitInsights>>,
	pub workspace_root: Option<String>,
	pub configured_team_id: Option<String>,
	pub configured_team_actor_id: String,
	pub configured_team_secret: Option<String>,
	pub license_validator: Arc<LicenseValidator>,
	pub config: Arc<tokio::sync::RwLock<DaemonConfig>>,
    pub token_tracker: Arc<crate::token::tracker::TokenTracker>,
    pub embedding_store: crate::observer::embedding_store::EmbeddingStore,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DaemonConfig {
    #[serde(default)]
    pub brain_paused: bool,
	#[serde(default)]
	pub license_key: Option<String>,
}

impl DaemonConfig {
    pub fn load(workspace_root: Option<&str>) -> Self {
        if let Some(root) = workspace_root {
            let path = std::path::Path::new(root).join(".memix").join("daemon_config.json");
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<Self>(&content) {
                    tracing::info!("Loaded persistent daemon config from {}", path.display());
                    return config;
                }
            }
        }

        if let Some(home) = dirs::home_dir() {
            let path = home.join(".memix").join("daemon_config.json");
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<Self>(&content) {
                    tracing::info!("Loaded user daemon config from {}", path.display());
                    return config;
                }
            }
        }

        Self::default()
    }

    pub fn target_paths(workspace_root: Option<&str>) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        if let Some(root) = workspace_root {
            paths.push(std::path::Path::new(root).join(".memix").join("daemon_config.json"));
        }
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".memix").join("daemon_config.json"));
        }
        if paths.is_empty() {
            if let Ok(cur) = std::env::current_dir() {
                paths.push(cur.join(".memix").join("daemon_config.json"));
            }
        }
        paths
    }

    pub fn config_path(workspace_root: Option<&str>) -> std::path::PathBuf {
        Self::target_paths(workspace_root)
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join(".memix").join("daemon_config.json")
            })
    }

    pub fn save(&self, workspace_root: Option<&str>) {
        for path in Self::target_paths(workspace_root) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(content) = serde_json::to_string_pretty(self) {
                if let Err(err) = std::fs::write(&path, &content) {
                    tracing::warn!("Failed to persist daemon config to {}: {}", path.display(), err);
                }
            }
        }
    }
}

fn classify_storage_error(msg: &str) -> StatusCode {
    let lowered = msg.to_lowercase();
    if lowered.contains("connection refused")
        || lowered.contains("connection reset")
        || lowered.contains("broken pipe")
        || lowered.contains("timed out")
        || lowered.contains("no route to host")
    {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

pub async fn get_patterns(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // workspace_root must be set — patterns analysis requires a directory to walk.
    // Return a clean empty report rather than crashing if the daemon has no workspace.
    let Some(root) = state.workspace_root.as_deref() else {
        return Json(serde_json::json!({
            "patterns": [],
            "total_files_scanned": 0,
            "total_functions_analyzed": 0,
            "scan_duration_ms": 0
        })).into_response();
    };

    // PatternEngine::analyze is CPU-heavy (walks the entire workspace + parses every file).
    // Wrap it in spawn_blocking so it never stalls the async executor.
    let root_path = std::path::PathBuf::from(root);
    let report = tokio::task::spawn_blocking(move || {
        PatternEngine::new(3).analyze(&root_path)
    }).await;

    match report {
        Ok(r) => Json(serde_json::to_value(r).unwrap_or_default()).into_response(),
        Err(e) => {
            tracing::error!("Pattern scan task failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Pattern scan failed").into_response()
        }
    }
}

async fn import_brain_json(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    if state.config.read().await.brain_paused {
        return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
    }
    match state.storage.import_project_from_json(&project_id).await {
        Ok(imported) => (StatusCode::OK, Json(serde_json::json!({"imported": imported}))).into_response(),
        Err(e) => {
            tracing::error!("Failed to import brain JSON for {}: {}", project_id, e);
            let msg = e.to_string();
            let status = classify_storage_error(&msg);
            (status, msg).into_response()
        }
    }
}

async fn migrate_brain_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    if state.config.read().await.brain_paused {
        return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
    }
    match migrations::run_project_migrations(state.storage.clone(), &project_id).await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => {
            tracing::error!("Failed to migrate brain for {}: {}", project_id, e);
            let msg = e.to_string();
            let status = classify_storage_error(&msg);
            (status, msg).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Deserialize, Default)]
pub struct SessionQuery {
    pub limit: Option<usize>,
    pub since_ms: Option<i64>,
}

#[derive(Deserialize)]
pub struct GenerateRulesRequest {
    pub project_id: String,
    pub redis_url: Option<String>,
    pub ide: String,
    pub workspace_root: String,
}

#[derive(Deserialize)]
pub struct TokenCountRequest {
    pub text: String,
}

#[derive(Deserialize)]
pub struct TokenOptimizeRequest {
    pub sections: Vec<ContextSection>,
    pub budget: usize,
}

#[derive(Deserialize)]
pub struct PromptOptimizeQuery {
    pub task_type: String,
}

#[derive(Deserialize)]
pub struct ProactiveRiskQuery {
    pub project_id: String,
    pub file: String,
}

#[derive(Deserialize)]
pub struct HierarchyResolveRequest {
    pub layers: Vec<String>,
    pub entry_id: String,
    pub merge: Option<bool>,
}

#[derive(Deserialize)]
pub struct TeamSyncRequest {
    pub project_id: String,
    #[serde(default)]
    pub team_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ActivateLicenseRequest {
    pub key: String,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Deserialize)]
pub struct InitiateLicenseRequest {
    pub email: String,
}

#[derive(Deserialize, Default)]
pub struct LicenseStatusQuery {
    pub device_id: Option<String>,
}

#[derive(Deserialize)]
pub struct RedisPingRequest {
    pub redis_url: String,
}

#[derive(Deserialize)]
pub struct ContextSection {
    pub id: String,
    pub content: String,
    pub priority: u8,
}

#[derive(Deserialize)]
pub struct SupersedeRequest {
    pub superseded_by_id: String,
}

#[derive(Deserialize)]
pub struct ContradictionRequest {
    pub contradicts_id: String,
}

#[derive(Deserialize, Default)]
pub struct ReasoningQuery {
    pub depth: Option<usize>,
}

#[derive(serde::Serialize)]
pub struct RelationshipEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

#[derive(serde::Serialize)]
pub struct ReasoningChainResponse {
    pub root_id: String,
    pub count: usize,
    pub nodes: Vec<MemoryEntry>,
    pub edges: Vec<RelationshipEdge>,
}

pub fn build_router(
    storage: Arc<dyn StorageBackend>,
    autonomous: Arc<tokio::sync::Mutex<AutonomousPairProgrammer>>,
    recorder: Arc<FlightRecorder>,
    code_dna: Arc<tokio::sync::Mutex<ProjectCodeDna>>,
    predictor: Arc<ContextPredictor>,
    call_graph: Arc<tokio::sync::Mutex<CallGraph>>,
    agent_runtime: Arc<tokio::sync::Mutex<AgentRuntime>>,
    git_insights: Arc<tokio::sync::Mutex<ProjectGitInsights>>,
    workspace_root: Option<String>,
    configured_team_id: Option<String>,
    configured_team_actor_id: String,
    configured_team_secret: Option<String>,
    license_validator: Arc<LicenseValidator>,
    config: Arc<tokio::sync::RwLock<DaemonConfig>>,
    token_tracker: Arc<crate::token::tracker::TokenTracker>,
    embedding_store: crate::observer::embedding_store::EmbeddingStore,
) -> Router {
    let shared_state = Arc::new(AppState {
        storage,
        autonomous,
        recorder,
        code_dna,
        predictor,
        call_graph,
        agent_runtime,
        git_insights,
        workspace_root,
        configured_team_id,
        configured_team_actor_id,
        configured_team_secret,
        license_validator,
        config,
        token_tracker,
        embedding_store,
    });

    Router::new()
        // Health & Daemon
        .route("/health", get(health_check))
        .route("/api/v1/daemon/status", get(daemon_status))
        .route("/api/v1/daemon/shutdown", post(daemon_shutdown))
		.route("/api/v1/license/initiate", post(initiate_license))
		.route("/api/v1/license/pending/:token", get(get_pending_license))
		.route("/api/v1/license/activate", post(activate_license))
		.route("/api/v1/license/status", get(get_license_status))
		.route("/api/v1/redis/ping", post(redis_ping))
		.route("/api/v1/redis/stats", get(redis_stats))
		
		// Control API
		.route("/api/v1/control/pause", post(control_pause))
		.route("/api/v1/control/resume", post(control_resume))
		.route("/api/v1/control/status", get(control_status))
        
        // Brain CRUD
        .route("/api/v1/memory/:project_id", get(get_memory).post(upsert_memory).delete(purge_project))
        .route("/api/v1/memory/:project_id/search", get(search_memory))
        .route("/api/v1/memory/:project_id/:entry_id", delete(delete_memory))
		.route("/api/v1/memory/:project_id/:entry_id/supersede", post(link_supersede))
		.route("/api/v1/memory/:project_id/:entry_id/contradictions", post(add_contradiction_link))
		.route("/api/v1/memory/:project_id/:entry_id/contradictions/:contradicts_id", delete(resolve_contradiction_link))
		.route("/api/v1/memory/:project_id/:entry_id/reasoning-chain", get(get_reasoning_chain))
		.route("/api/v1/brain/export/:project_id", post(export_brain_json))
		.route("/api/v1/brain/import/:project_id", post(import_brain_json))
		.route("/api/v1/brain/migrate/:project_id", post(migrate_brain_project))
        
        // Rules Generation
        .route("/api/v1/rules/generate", post(generate_rules))
        
        // Token Engine
        .route("/api/v1/tokens/count", post(count_tokens))
        .route("/api/v1/tokens/optimize", post(optimize_tokens))
		.route("/api/v1/context/compile", post(compile_context))
		.route("/api/v1/agents/config", get(get_agent_configs))
		.route("/api/v1/agents/reports", get(get_agent_reports))
		.route("/api/v1/proactive/risk", get(get_proactive_risk))
		.route("/api/v1/learning/prompts/:project_id/record", post(record_prompt))
		.route("/api/v1/learning/prompts/:project_id/optimize", get(optimize_prompt_strategy))
		.route("/api/v1/learning/model-performance/:project_id", get(get_model_performance))
		.route("/api/v1/learning/developer-profile", get(get_developer_profile))
		.route("/api/v1/brain/hierarchy/resolve", post(resolve_brain_hierarchy))
        
        // Autonomous Pair Programmer
        .route("/api/v1/autonomous/impact/:file", get(get_impact))
        .route("/api/v1/autonomous/predict/:file", get(predict_questions))
        .route("/api/v1/autonomous/conflicts", get(detect_conflicts))

		// Observer snapshots
		.route("/api/v1/observer/dna", get(get_observer_dna))
		.route("/api/v1/observer/dna/otel", get(get_observer_dna_otel))
		.route("/api/v1/observer/graph", get(get_observer_graph))
		.route("/api/v1/observer/changes", get(get_observer_changes))
		.route("/api/v1/observer/intent", get(get_observer_intent))
		.route("/api/v1/observer/git", get(get_observer_git))
		.route("/api/v1/observer/call-graph", get(get_causal_chain))

		// Skeleton Index
		.route("/api/v1/skeleton/stats/:project_id", get(skeleton_stats))

		// Structural Intelligence
		.route("/api/v1/importance", get(get_importance))
		.route("/api/v1/blast-radius", get(get_blast_radius))

		// Session recorder
		.route("/api/v1/session/current", get(get_session_current))
		.route("/api/v1/session/replay", get(get_session_replay))
		.route("/api/v1/session/timeline", get(get_session_replay))
        
        // Team Sync
        .route("/api/v1/team/sync", post(team_sync))
        
        // Token Intelligence
        .route("/api/v1/tokens/stats", get(get_token_stats))
        .route("/api/v1/tokens/record", post(record_ai_token_use))

		// Observer patterns
		.route("/observer/patterns", get(get_patterns))

        .with_state(shared_state)
}

fn filter_session_items(mut items: Vec<FlightRecord>, params: &SessionQuery) -> Vec<FlightRecord> {
	if let Some(since_ms) = params.since_ms {
		items.retain(|i| i.timestamp.timestamp_millis() >= since_ms);
	}
	if let Some(limit) = params.limit {
		if items.len() > limit {
			items = items.split_off(items.len() - limit);
		}
	}
	items
}

async fn get_session_current(
	State(state): State<Arc<AppState>>,
	Query(params): Query<SessionQuery>,
) -> impl IntoResponse {
	let items: Vec<FlightRecord> = filter_session_items(state.recorder.dump_blackbox(), &params);
	(StatusCode::OK, Json(serde_json::json!({"count": items.len(), "items": items}))).into_response()
}

async fn get_session_replay(
	State(state): State<Arc<AppState>>,
	Query(params): Query<SessionQuery>,
) -> impl IntoResponse {
	let items: Vec<FlightRecord> = filter_session_items(state.recorder.dump_blackbox(), &params);
	(StatusCode::OK, Json(serde_json::json!({"count": items.len(), "items": items}))).into_response()
}

#[derive(serde::Serialize)]
struct ObserverChangesSnapshot {
	count: usize,
	items: Vec<SemanticDiff>,
}

async fn get_observer_graph(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let autonomous = state.autonomous.lock().await;
	let graph: DependencyGraph = autonomous.dependency_graph.clone();
	(StatusCode::OK, Json(graph)).into_response()
}

async fn get_observer_dna(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let dna = state.code_dna.lock().await.clone();
	(StatusCode::OK, Json(dna)).into_response()
}

async fn get_observer_dna_otel(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let dna = state.code_dna.lock().await.clone();
	(StatusCode::OK, Json(dna.to_otel_export())).into_response()
}

async fn get_observer_intent(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let snapshot = state.predictor.get_current_intent().await;
	(StatusCode::OK, Json(serde_json::json!({
		"intent": snapshot
	}))).into_response()
}

async fn get_observer_git(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let snapshot = state.git_insights.lock().await.clone();
	(StatusCode::OK, Json(snapshot)).into_response()
}

#[derive(Deserialize)]
struct CausalChainQuery {
	file: String,
}

async fn get_causal_chain(
	State(state): State<Arc<AppState>>,
	Query(query): Query<CausalChainQuery>,
) -> impl IntoResponse {
	let context = state.call_graph.lock().await.causal_context_for_file(&query.file);
	(StatusCode::OK, Json(context)).into_response()
}

async fn get_observer_changes(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let autonomous = state.autonomous.lock().await;
	let items: Vec<SemanticDiff> = autonomous
		.change_history
		.iter()
		.rev()
		.take(25)
		.map(|c| c.diff.clone())
		.collect();
	let snapshot = ObserverChangesSnapshot { count: items.len(), items };
	(StatusCode::OK, Json(snapshot)).into_response()
}

async fn export_brain_json(
	State(state): State<Arc<AppState>>,
	Path(project_id): Path<String>,
) -> impl IntoResponse {
	match state.storage.export_project_to_json(&project_id).await {
		Ok(written) => (StatusCode::OK, Json(serde_json::json!({"written": written}))).into_response(),
		Err(e) => {
			tracing::error!("Failed to export brain JSON for {}: {}", project_id, e);
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
		}
	}
}

async fn redis_ping(Json(req): Json<RedisPingRequest>) -> impl IntoResponse {
	let client = match redis::Client::open(req.redis_url) {
		Ok(c) => c,
		Err(e) => {
			let msg = e.to_string();
			return (StatusCode::BAD_REQUEST, msg).into_response();
		}
	};

	match client.get_multiplexed_async_connection().await {
		Ok(mut conn) => {
			let pong: redis::RedisResult<String> = redis::cmd("PING").query_async(&mut conn).await;
			match pong {
				Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
				Err(e) => {
					let msg = e.to_string();
					let status = classify_storage_error(&msg);
					(status, msg).into_response()
				}
			}
		}
		Err(e) => {
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
		}
	}
}

async fn redis_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	match state.storage.redis_stats().await {
		Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
		Err(e) => {
			tracing::error!("Failed to read redis stats: {}", e);
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
		}
	}
}

/// Simple health check for the VS Code extension to poll on boot
async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let config = state.config.read().await.clone();
	if config.brain_paused {
		(StatusCode::OK, Json(serde_json::json!({
			"status": "paused",
			"message": "Brain is sleeping"
		}))).into_response()
	} else {
    	(StatusCode::OK, Json(serde_json::json!({
			"status": "healthy",
			"message": "Memix Daemon is healthy"
		}))).into_response()
	}
}

async fn control_pause(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut config = state.config.write().await;
    config.brain_paused = true;
    config.save(state.workspace_root.as_deref());
    tracing::info!("Brain operations paused globally via /control/pause");
    (StatusCode::OK, Json(config.clone())).into_response()
}

async fn control_resume(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut config = state.config.write().await;
    config.brain_paused = false;
    config.save(state.workspace_root.as_deref());
    tracing::info!("Brain operations resumed globally via /control/resume");
    (StatusCode::OK, Json(config.clone())).into_response()
}


async fn control_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = state.config.read().await.clone();
    let path = DaemonConfig::config_path(state.workspace_root.as_deref());
    (StatusCode::OK, Json(serde_json::json!({"config": config, "config_path": path.to_string_lossy()}))).into_response()
}

async fn initiate_license(
	State(state): State<Arc<AppState>>,
	Json(req): Json<InitiateLicenseRequest>,
) -> impl IntoResponse {
	match proxy_license_post::<_, LicenseInitiateResult>(&state, "/v1/license/initiate", &serde_json::json!({ "email": req.email })).await {
		Ok(result) => (StatusCode::OK, Json(result)).into_response(),
		Err((status, body)) => (status, body).into_response(),
	}
}

async fn get_pending_license(
	State(state): State<Arc<AppState>>,
	Path(token): Path<String>,
) -> impl IntoResponse {
	let path = format!("/v1/license/pending/{}", token);
	match proxy_license_get::<LicensePendingResult>(&state, &path).await {
		Ok(result) => (StatusCode::OK, Json(result)).into_response(),
		Err((status, body)) => (status, body).into_response(),
	}
}

async fn activate_license(
	State(state): State<Arc<AppState>>,
	Json(req): Json<ActivateLicenseRequest>,
) -> impl IntoResponse {
	match state.license_validator.activate(&req.key, req.device_id.as_deref()).await {
		Ok(status) => {
			let mut config = state.config.write().await;
			config.license_key = Some(req.key);
			config.save(state.workspace_root.as_deref());
			(StatusCode::OK, Json(status)).into_response()
		}
		Err(err) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
			"available": state.license_validator.is_available(),
			"active": false,
			"message": err.to_string()
		}))).into_response()
	}
}

async fn skeleton_stats(
	State(state): State<Arc<AppState>>,
	Path(project_id): Path<String>,
) -> impl IntoResponse {
	match state.storage.skeleton_stats(&project_id).await {
		Ok((fsi, fusi, total)) => (StatusCode::OK, Json(serde_json::json!({
			"project_id": project_id,
			"fsi_count": fsi,
			"fusi_count": fusi,
			"total": total,
			"capacity": 2000
		}))).into_response(),
		Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	}
}

#[derive(Deserialize)]
struct ImportanceQuery {
	top_n: Option<usize>,
}

async fn get_importance(
	State(state): State<Arc<AppState>>,
	Query(params): Query<ImportanceQuery>,
) -> impl IntoResponse {
	let autonomous = state.autonomous.lock().await;
	let graph = &autonomous.dependency_graph;
	let top_n = params.top_n.unwrap_or(15);
	let scores = compute_importance(&graph.edges_out, top_n);
	(StatusCode::OK, Json(serde_json::json!({
		"top_files": scores.top_files,
		"scc_groups": scores.scc_groups,
		"circular_files": scores.circular_files,
		"node_count": scores.betweenness.len(),
		"cycle_count": scores.scc_groups.len(),
		"topological_order_length": scores.topological_order.len(),
		"betweenness": scores.betweenness,
		"pagerank": scores.pagerank,
	}))).into_response()
}

#[derive(Deserialize)]
struct BlastRadiusQuery {
	file: String,
	max_depth: Option<usize>,
}

async fn get_blast_radius(
	State(state): State<Arc<AppState>>,
	Query(params): Query<BlastRadiusQuery>,
) -> impl IntoResponse {
	let max_depth = params.max_depth.unwrap_or(
		std::env::var("MEMIX_MAX_BLAST_RADIUS_DEPTH").ok().and_then(|s| s.parse().ok()).unwrap_or(5)
	);
	let autonomous = state.autonomous.lock().await;
	let graph = &autonomous.dependency_graph;
	let blast = compute_blast_radius(&graph.edges_in, &params.file, max_depth);
	(StatusCode::OK, Json(blast)).into_response()
}

fn render_causal_context(context: &FileCausalContext) -> Option<String> {
	if context.symbols.is_empty() {
		return None;
	}

	let mut lines = vec![
		format!("Causal chain for {}", context.file),
		format!(
			"Outgoing edges: {} | Incoming edges: {}",
			context.total_outgoing_edges, context.total_incoming_edges
		),
	];

	for symbol in context.symbols.iter().take(8) {
		lines.push(format!("\nSymbol: {}", symbol.symbol));
		if symbol.called_by.is_empty() {
			lines.push("Called by: none".to_string());
		} else {
			lines.push("Called by:".to_string());
			for caller in symbol.called_by.iter().take(4) {
				let location = if caller.call_line > 0 {
					format!(" line {}", caller.call_line)
				} else {
					String::new()
				};
				lines.push(format!(
					"- {} :: {}{}",
					caller.caller_file, caller.caller_symbol, location
				));
			}
		}

		if symbol.calls.is_empty() {
			lines.push("Calls: none".to_string());
		} else {
			lines.push("Calls:".to_string());
			for callee in symbol.calls.iter().take(5) {
				let target = if callee.callee_file.is_empty() {
					callee.callee_symbol.clone()
				} else if callee.callee_line > 0 {
					format!(
						"{} :: {} (line {})",
						callee.callee_file, callee.callee_symbol, callee.callee_line
					)
				} else {
					format!("{} :: {}", callee.callee_file, callee.callee_symbol)
				};
				lines.push(format!("- {}", target));
			}
		}
	}

	Some(lines.join("\n"))
}

async fn get_license_status(
	State(state): State<Arc<AppState>>,
	Query(query): Query<LicenseStatusQuery>,
) -> impl IntoResponse {
	let config = state.config.read().await.clone();
	let status = state
         .license_validator
         .status_for_key(config.license_key.as_deref(), query.device_id.as_deref())
         .await;
    (StatusCode::OK, Json(status)).into_response()
}

async fn proxy_license_get<T: serde::de::DeserializeOwned>(state: &Arc<AppState>, path: &str) -> Result<T, (StatusCode, String)> {
    let base_url = match state.license_validator.server_base_url() {
        Some(value) => value,
        None => return Err((StatusCode::PRECONDITION_FAILED, "MEMIX_LICENSE_SERVER_URL is not configured".to_string())),
    };
    let url = format!("{}{}", base_url, path);
    let response = state.license_validator.http_client()
        .get(url)
        .send()
        .await
        .map_err(|err| (StatusCode::BAD_GATEWAY, err.to_string()))?;
	let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
	let body = response.text().await.map_err(|err| (StatusCode::BAD_GATEWAY, err.to_string()))?;
	if !status.is_success() {
		return Err((status, body));
	}
	serde_json::from_str(&body).map_err(|err| (StatusCode::BAD_GATEWAY, err.to_string()))
}

async fn proxy_license_post<B: serde::Serialize, T: serde::de::DeserializeOwned>(
	state: &Arc<AppState>,
	path: &str,
	body: &B,
) -> Result<T, (StatusCode, String)> {
	let base_url = match state.license_validator.server_base_url() {
		Some(value) => value,
		None => return Err((StatusCode::PRECONDITION_FAILED, "MEMIX_LICENSE_SERVER_URL is not configured".to_string())),
	};
	let url = format!("{}{}", base_url, path);
    let response = state.license_validator.http_client()
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|err| (StatusCode::BAD_GATEWAY, err.to_string()))?;
	let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
	let text = response.text().await.map_err(|err| (StatusCode::BAD_GATEWAY, err.to_string()))?;
	if !status.is_success() {
		return Err((status, text));
	}
	serde_json::from_str(&text).map_err(|err| (StatusCode::BAD_GATEWAY, err.to_string()))
}

/// Get memory for a specific project
async fn get_memory(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    let validator = BrainValidator::new();
    if let Err(e) = validator.validate_project_id(&project_id) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    tracing::debug!("📥 get_memory called for project: {}", project_id);

    match state.storage.get_entries(&project_id).await {
        Ok(entries) => {
            tracing::debug!("✅ Successfully retrieved {} entries for {}", entries.len(), project_id);

            (StatusCode::OK, Json(entries)).into_response()
        },
        Err(e) => {
            tracing::error!("Failed to read memory for {}: {}", project_id, e);
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
        }
    }
}

/// Upsert a new memory entry for a project
async fn upsert_memory(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(entry): Json<MemoryEntry>,
) -> impl IntoResponse {
	if state.config.read().await.brain_paused {
		return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
	}
	let validator = BrainValidator::new();
	if let Err(e) = validator.validate_project_id(&project_id) {
		return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
	}
	if let Err(e) = validator.validate_entry(&entry) {
		return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
	}
    match state.storage.upsert_entry(&project_id, entry).await {
        Ok(_) => (StatusCode::CREATED, "").into_response(),
        Err(e) => {
            tracing::error!("Failed to upsert memory for {}: {}", project_id, e);
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
        }
    }
}

/// Search memory semantically for a project
async fn search_memory(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    let validator = BrainValidator::new();
    if let Err(e) = validator.validate_project_id(&project_id) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    match state.storage.search_entries(&project_id, &params.q).await {
        Ok(entries) => (StatusCode::OK, Json(entries)).into_response(),
        Err(e) => {
            tracing::error!("Failed to search memory for {}: {}", project_id, e);
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
        }
    }
}

async fn link_supersede(
	State(state): State<Arc<AppState>>,
	Path((project_id, entry_id)): Path<(String, String)>,
	Json(req): Json<SupersedeRequest>,
) -> impl IntoResponse {
	if state.config.read().await.brain_paused {
		return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
	}
	let superseded_by_id = req.superseded_by_id.trim();
	if superseded_by_id.is_empty() {
		return (StatusCode::BAD_REQUEST, "superseded_by_id is required").into_response();
	}

	let mut entry = match state.storage.get_entries(&project_id).await {
		Ok(entries) => match entries.into_iter().find(|e| e.id == entry_id) {
			Some(e) => e,
			None => return (StatusCode::NOT_FOUND, "memory entry not found").into_response(),
		},
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};

	let manager = BrainManager::new();
	manager.link_superseded(&mut entry, superseded_by_id.to_string());

	match state.storage.upsert_entry(&project_id, entry).await {
		Ok(_) => (StatusCode::OK, Json(serde_json::json!({
			"ok": true,
			"entry_id": entry_id,
			"superseded_by": superseded_by_id
		}))).into_response(),
		Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	}
}

async fn add_contradiction_link(
	State(state): State<Arc<AppState>>,
	Path((project_id, entry_id)): Path<(String, String)>,
	Json(req): Json<ContradictionRequest>,
) -> impl IntoResponse {
	if state.config.read().await.brain_paused {
		return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
	}
	let contradicts_id = req.contradicts_id.trim();
	if contradicts_id.is_empty() {
		return (StatusCode::BAD_REQUEST, "contradicts_id is required").into_response();
	}

	let mut entry = match state.storage.get_entries(&project_id).await {
		Ok(entries) => match entries.into_iter().find(|e| e.id == entry_id) {
			Some(e) => e,
			None => return (StatusCode::NOT_FOUND, "memory entry not found").into_response(),
		},
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};

	let manager = BrainManager::new();
	manager.add_contradiction(&mut entry, contradicts_id.to_string());

	match state.storage.upsert_entry(&project_id, entry).await {
		Ok(_) => (StatusCode::OK, Json(serde_json::json!({
			"ok": true,
			"entry_id": entry_id,
			"contradicts_id": contradicts_id
		}))).into_response(),
		Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	}
}

async fn resolve_contradiction_link(
	State(state): State<Arc<AppState>>,
	Path((project_id, entry_id, contradicts_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
	if state.config.read().await.brain_paused {
		return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
	}
	let mut entry = match state.storage.get_entries(&project_id).await {
		Ok(entries) => match entries.into_iter().find(|e| e.id == entry_id) {
			Some(e) => e,
			None => return (StatusCode::NOT_FOUND, "memory entry not found").into_response(),
		},
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};

	let manager = BrainManager::new();
	manager.resolve_contradiction(&mut entry, &contradicts_id);

	match state.storage.upsert_entry(&project_id, entry).await {
		Ok(_) => (StatusCode::OK, Json(serde_json::json!({
			"ok": true,
			"entry_id": entry_id,
			"resolved_contradiction": contradicts_id
		}))).into_response(),
		Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	}
}

async fn get_reasoning_chain(
	State(state): State<Arc<AppState>>,
	Path((project_id, entry_id)): Path<(String, String)>,
	Query(params): Query<ReasoningQuery>,
) -> impl IntoResponse {
	let entries = match state.storage.get_entries(&project_id).await {
		Ok(entries) => entries,
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};

	let by_id: HashMap<String, MemoryEntry> = entries
		.into_iter()
		.map(|e| (e.id.clone(), e))
		.collect();

	if !by_id.contains_key(&entry_id) {
		return (StatusCode::NOT_FOUND, "memory entry not found").into_response();
	}

	let max_depth = params.depth.unwrap_or(2).clamp(1, 6);
	let mut queue: VecDeque<(String, usize)> = VecDeque::new();
	let mut visited: HashSet<String> = HashSet::new();
	let mut edges: Vec<RelationshipEdge> = Vec::new();
	let mut edge_keys: HashSet<String> = HashSet::new();

	queue.push_back((entry_id.clone(), 0));
	visited.insert(entry_id.clone());

	while let Some((current_id, depth)) = queue.pop_front() {
		if depth >= max_depth {
			continue;
		}

		if let Some(current) = by_id.get(&current_id) {
			let mut neighbors: Vec<(String, String, String)> = Vec::new();

			if let Some(parent) = &current.parent_id {
				neighbors.push((parent.clone(), current.id.clone(), "parent".to_string()));
			}
			for cause in &current.caused_by {
				neighbors.push((cause.clone(), current.id.clone(), "caused_by".to_string()));
			}
			for enabled in &current.enables {
				neighbors.push((current.id.clone(), enabled.clone(), "enables".to_string()));
			}
			if let Some(superseded_by) = &current.superseded_by {
				neighbors.push((current.id.clone(), superseded_by.clone(), "superseded_by".to_string()));
			}
			for c in &current.contradicts {
				neighbors.push((current.id.clone(), c.clone(), "contradicts".to_string()));
			}

			for other in by_id.values() {
				if other.id == current.id {
					continue;
				}
				if other.parent_id.as_ref() == Some(&current.id) {
					neighbors.push((current.id.clone(), other.id.clone(), "parent".to_string()));
				}
				if other.caused_by.contains(&current.id) {
					neighbors.push((current.id.clone(), other.id.clone(), "caused_by".to_string()));
				}
				if other.enables.contains(&current.id) {
					neighbors.push((other.id.clone(), current.id.clone(), "enables".to_string()));
				}
				if other.superseded_by.as_ref() == Some(&current.id) {
					neighbors.push((other.id.clone(), current.id.clone(), "superseded_by".to_string()));
				}
				if other.contradicts.contains(&current.id) {
					neighbors.push((other.id.clone(), current.id.clone(), "contradicts".to_string()));
				}
			}

			for (from, to, relation) in neighbors {
				if !by_id.contains_key(&from) || !by_id.contains_key(&to) {
					continue;
				}
				let edge_key = format!("{}|{}|{}", from, to, relation);
				if edge_keys.insert(edge_key) {
					edges.push(RelationshipEdge {
						from: from.clone(),
						to: to.clone(),
						relation: relation.clone(),
					});
				}

				let neighbor_id = if from == current_id { to } else { from };
				if visited.insert(neighbor_id.clone()) {
					queue.push_back((neighbor_id, depth + 1));
				}
			}
		}
	}

	let nodes: Vec<MemoryEntry> = visited
		.iter()
		.filter_map(|id| by_id.get(id).cloned())
		.collect();

	(StatusCode::OK, Json(ReasoningChainResponse {
		root_id: entry_id,
		count: nodes.len(),
		nodes,
		edges,
	})).into_response()
}

/// Delete a specific memory entry
async fn delete_memory(
    State(state): State<Arc<AppState>>,
    Path((project_id, entry_id)): Path<(String, String)>,
) -> impl IntoResponse {
	if state.config.read().await.brain_paused {
		return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
	}
    match state.storage.delete_entry(&project_id, &entry_id).await {
        Ok(_) => (StatusCode::OK, "").into_response(),
        Err(e) => {
            tracing::error!("Failed to delete memory {} for {}: {}", entry_id, project_id, e);
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
        }
    }
}

/// Purge an entire project's memory
async fn purge_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
	if state.config.read().await.brain_paused {
		return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
	}
    match state.storage.purge_project(&project_id).await {
        Ok(_) => (StatusCode::NO_CONTENT, "").into_response(),
        Err(e) => {
            tracing::error!("Failed to purge project {}: {}", project_id, e);
			let msg = e.to_string();
			let status = classify_storage_error(&msg);
			(status, msg).into_response()
        }
    }
}

/// Daemon status
async fn daemon_status() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "healthy",
        "version": "0.3.1-beta",
        "features": [
            "autonomous_watching",
            "semantic_diff",
            "code_dna",
            "dependency_graph",
            "intent_detection",
            "predictive_context",
            "token_engine",
            "crdt_sync",
            "flight_recorder"
        ]
    })))
}

/// Graceful shutdown
async fn daemon_shutdown() -> impl IntoResponse {
    tracing::info!("Shutdown requested via API");
    (StatusCode::OK, "Shutting down...")
}

/// Generate rules files for IDE
async fn generate_rules(
    Json(req): Json<GenerateRulesRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let ide = match req.ide.to_lowercase().as_str() {
        "cursor" => IdeType::Cursor,
        "windsurf" => IdeType::Windsurf,
        "claude-code" | "claude" => IdeType::ClaudeCode,
        "antigravity" => IdeType::Antigravity,
        "vscode" | "copilot" => IdeType::Vscode,
        _ => IdeType::Unknown,
    };

    let result = RulesEngine::generate_for_ide(
        &req.project_id,
        ide,
        &req.workspace_root,
    );

    let write_result = tokio::task::spawn_blocking(move || {
        let ok = result.write_files();
        (ok, result.config)
    }).await.map_err(|e| {
        tracing::error!("Failed to join spawn_blocking for write_files: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match write_result.0 {
        Ok(_) => Ok(Json(serde_json::json!({
            "success": true,
            "config": write_result.1,
            "message": format!("Rules generated for {} in {}/", 
                format!("{:?}", ide), write_result.1.rules_dir)
        }))),
        Err(e) => {
            tracing::error!("Failed to write rules files: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Count tokens exactly
async fn count_tokens(
    Json(req): Json<TokenCountRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match TokenEngine::count_tokens(&req.text) {
        Ok(count) => Ok(Json(serde_json::json!({
            "tokens": count,
            "chars": req.text.len(),
            "ratio": if req.text.len() > 0 { req.text.len() as f64 / count as f64 } else { 0.0 }
        }))),
        Err(e) => {
            tracing::error!("Token count failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
async fn optimize_tokens(
    Json(req): Json<TokenOptimizeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
	if req.budget == 0 {
		return Ok(Json(serde_json::json!({
			"selected": Vec::<String>::new(),
			"total_tokens": 0,
			"budget": req.budget,
			"sections_count": req.sections.len()
		})));
	}

	struct Candidate {
		id: String,
		tokens: usize,
		value: usize,
	}

	let mut candidates: Vec<Candidate> = Vec::new();
	for section in req.sections {
		if let Ok(tokens) = TokenEngine::count_tokens(&section.content) {
			if tokens == 0 || tokens > req.budget {
				continue;
			}
			let value = usize::from(section.priority).saturating_mul(100).saturating_add(1);
			candidates.push(Candidate {
				id: section.id,
				tokens,
				value,
			});
		}
	}

	let n = candidates.len();
	let w = req.budget;
	let mut dp = vec![vec![0usize; w + 1]; n + 1];

	for i in 1..=n {
		let wt = candidates[i - 1].tokens;
		let val = candidates[i - 1].value;
		for cap in 0..=w {
			dp[i][cap] = dp[i - 1][cap];
			if wt <= cap {
				let with_item = dp[i - 1][cap - wt].saturating_add(val);
				if with_item > dp[i][cap] {
					dp[i][cap] = with_item;
				}
			}
		}
	}

	let mut selected: Vec<String> = Vec::new();
	let mut total_tokens = 0usize;
	let mut cap = w;
	for i in (1..=n).rev() {
		if dp[i][cap] != dp[i - 1][cap] {
			selected.push(candidates[i - 1].id.clone());
			total_tokens = total_tokens.saturating_add(candidates[i - 1].tokens);
			cap = cap.saturating_sub(candidates[i - 1].tokens);
		}
	}
	selected.reverse();

	Ok(Json(serde_json::json!({
		"selected": selected,
		"total_tokens": total_tokens,
		"budget": req.budget,
		"sections_count": n
	})))
}

async fn compile_context(
	State(state): State<Arc<AppState>>,
	Json(req): Json<CompileRequest>,
) -> impl IntoResponse {
	let entries = match state.storage.get_entries(&req.project_id).await {
		Ok(entries) => entries,
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};
	let skeleton_entries = state.storage.get_skeleton_entries(&req.project_id).await.unwrap_or_default();
	let graph = state.autonomous.lock().await.dependency_graph.clone();
	let causal_context = render_causal_context(
		&state.call_graph.lock().await.causal_context_for_file(&req.active_file),
	);
	let history = state.recorder.dump_blackbox();
	let root = state.workspace_root.as_deref().filter(|s| s.len() < 1024).map(PathBuf::from);
	        let compiler = ContextCompiler::new(root);
        match compiler.compile(req, &graph, &history, &entries, &skeleton_entries, causal_context) {
                Ok(compiled) => {
                        state.token_tracker.session.record_context_compilation(
                                compiled.total_tokens as u64,
                                compiled.naive_token_estimate,
                        );
                        (StatusCode::OK, Json(compiled)).into_response()
                }
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
}

async fn get_agent_configs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let runtime = state.agent_runtime.lock().await;
	(StatusCode::OK, Json(serde_json::json!({
		"source_path": runtime.source_path(),
		"configs": runtime.configs()
	}))).into_response()
}

async fn get_agent_reports(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let runtime = state.agent_runtime.lock().await;
	(StatusCode::OK, Json(serde_json::json!({
		"reports": runtime.recent_reports()
	}))).into_response()
}

async fn get_proactive_risk(
	State(state): State<Arc<AppState>>,
	Query(query): Query<ProactiveRiskQuery>,
) -> impl IntoResponse {
	let entries = match state.storage.get_entries(&query.project_id).await {
		Ok(entries) => entries,
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};
	let graph = state.autonomous.lock().await.dependency_graph.clone();
	let dna = state.code_dna.lock().await.clone();
	let git = state.git_insights.lock().await.clone();
	let blast = compute_blast_radius(
		&graph.edges_in,
		&query.file,
		std::env::var("MEMIX_MAX_BLAST_RADIUS_DEPTH")
			.ok()
			.and_then(|value| value.parse().ok())
			.unwrap_or(5),
	);
	let warning = ProactiveWarner::assess_risk(&query.file, &entries, &graph, &dna, &git).map(|mut warning| {
		let blast_depth_boost = (blast.affected_count.min(25) as f32 / 25.0) * 0.25;
		warning.dependents = warning.dependents.max(blast.affected_count);
		warning.risk_score = (warning.risk_score + blast_depth_boost).min(1.0);
		if blast.affected_count > 0 {
			let chain = blast.critical_path.join(" -> ");
			warning.recommendation = format!(
				"{} Critical path: {}",
				warning.recommendation,
				chain
			);
		}
		warning
	});
	(StatusCode::OK, Json(serde_json::json!({
		"warning": warning,
		"blast_radius": blast
	}))).into_response()
}

async fn record_prompt(
	State(state): State<Arc<AppState>>,
	Path(project_id): Path<String>,
	Json(record): Json<PromptRecord>,
) -> impl IntoResponse {
	if state.config.read().await.brain_paused {
		return (StatusCode::SERVICE_UNAVAILABLE, "Brain operations are paused globally").into_response();
	}
	let entry = PromptOptimizer::to_memory_entry(&project_id, &record);
	match state.storage.upsert_entry(&project_id, entry).await {
		Ok(_) => (StatusCode::CREATED, Json(serde_json::json!({"ok": true}))).into_response(),
		Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	}
}

async fn optimize_prompt_strategy(
	State(state): State<Arc<AppState>>,
	Path(project_id): Path<String>,
	Query(query): Query<PromptOptimizeQuery>,
) -> impl IntoResponse {
	let entries = match state.storage.get_entries(&project_id).await {
		Ok(entries) => entries,
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};
	let records = PromptOptimizer::records_from_entries(&entries);
	(StatusCode::OK, Json(PromptOptimizer::suggest_context(&query.task_type, &records))).into_response()
}

async fn get_model_performance(
	State(state): State<Arc<AppState>>,
	Path(project_id): Path<String>,
) -> impl IntoResponse {
	let entries = match state.storage.get_entries(&project_id).await {
		Ok(entries) => entries,
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};
	let records = PromptOptimizer::records_from_entries(&entries);
	(StatusCode::OK, Json(PromptOptimizer::model_performance(&records))).into_response()
}

async fn get_developer_profile(State(state): State<Arc<AppState>>) -> impl IntoResponse {
	let projects = match state.storage.list_projects().await {
		Ok(projects) => projects,
		Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	};
	let mut entries_by_project = HashMap::new();
	for project in projects {
		if let Ok(entries) = state.storage.get_entries(&project).await {
			entries_by_project.insert(project, entries);
		}
	}
	(StatusCode::OK, Json(CrossProjectLearner::compute_developer_profile(&entries_by_project))).into_response()
}

async fn resolve_brain_hierarchy(
	State(state): State<Arc<AppState>>,
	Json(req): Json<HierarchyResolveRequest>,
) -> impl IntoResponse {
	let mut layers = Vec::new();
	for project_id in &req.layers {
		let entries = match state.storage.get_entries(project_id).await {
			Ok(entries) => entries,
			Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
		};
		let entries = entries
			.into_iter()
			.map(|entry| (entry.id.clone(), entry))
			.collect::<HashMap<_, _>>();
		layers.push(HierarchyLayer {
			project_id: project_id.clone(),
			entries,
		});
	}
	let hierarchy = BrainHierarchy::new(layers);
	let resolution = if req.merge.unwrap_or(false) {
		hierarchy.resolve_merged(&req.entry_id)
	} else {
		hierarchy.resolve(&req.entry_id)
	};
	match resolution {
		Some(value) => (StatusCode::OK, Json(value)).into_response(),
		None => (StatusCode::NOT_FOUND, "hierarchy entry not found").into_response(),
	}
}

/// Get impact analysis for a file
async fn get_impact(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Result<Json<ImpactAnalysis>, StatusCode> {
    let autonomous = state.autonomous.lock().await;
    let blast = compute_blast_radius(&autonomous.dependency_graph.edges_in, &file, 5);

    let impact = if blast.affected_count == 0 {
        ImpactAnalysis {
            file: file.clone(),
            change_type: crate::intelligence::autonomous::ChangeType::FunctionModified,
            severity: crate::intelligence::autonomous::ImpactSeverity::None,
            impacted_files: vec![],
            recommendations: vec!["No dependencies found".to_string()],
            risk_score: 0.0,
        }
    } else {
        let impacted: Vec<crate::intelligence::autonomous::ImpactedFile> = blast
            .affected_files
            .iter()
            .map(|entry| crate::intelligence::autonomous::ImpactedFile {
                path: entry.path.clone(),
                line: None,
                reason: format!("Reached via {}", entry.via),
                urgency: if entry.depth <= 1 {
                    crate::intelligence::autonomous::ImpactSeverity::High
                } else {
                    crate::intelligence::autonomous::ImpactSeverity::Medium
                },
            })
            .collect();

        ImpactAnalysis {
            file: file.clone(),
            change_type: crate::intelligence::autonomous::ChangeType::FunctionModified,
            severity: if blast.affected_count > 5 {
                crate::intelligence::autonomous::ImpactSeverity::High
            } else {
                crate::intelligence::autonomous::ImpactSeverity::Medium
            },
            impacted_files: impacted,
            recommendations: vec![format!(
                "{} files depend on this. Critical path: {}",
                blast.affected_count,
                blast.critical_path.join(" -> ")
            )],
            risk_score: (blast.affected_count as f32 / 10.0).min(1.0),
        }
    };

    Ok(Json(impact))
}

/// Predict questions based on context
async fn predict_questions(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Result<Json<Vec<PredictedQuestion>>, StatusCode> {
    let autonomous = state.autonomous.lock().await;
    let questions = autonomous.predict_questions(&file);
    Ok(Json(questions))
}

async fn detect_conflicts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<crate::intelligence::autonomous::ConflictReport>>, StatusCode> {
    let autonomous = state.autonomous.lock().await;
    let conflicts = autonomous.detect_conflicts();
    Ok(Json(conflicts))
}

/// Perform CRDT Team Brain Sync
async fn team_sync(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TeamSyncRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await.clone();
    let license_status = state
        .license_validator
        .status_for_key(config.license_key.as_deref(), None)
        .await;
    if !license_status.available {
        tracing::warn!("Rejected team sync because license validation is unavailable");
        return Err(StatusCode::PRECONDITION_FAILED);
    }
    if !license_status.active || license_status.tier != Some(LicenseTier::Pro) {
        tracing::warn!("Rejected team sync because Memix Pro is not active");
        return Err(StatusCode::PAYMENT_REQUIRED);
    }
    let validator = BrainValidator::new();
    if let Err(e) = validator.validate_project_id(&req.project_id) {
        tracing::warn!("Rejected team sync due to invalid project_id: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }
    let team_id = req
        .team_id
        .as_deref()
        .or(state.configured_team_id.as_deref())
        .filter(|value| value.len() <= 200)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    if let Err(e) = validator.validate_project_id(&team_id) {
        tracing::warn!("Rejected team sync due to invalid team_id: {}", e);
        return Err(StatusCode::BAD_REQUEST);
    }
    let team_secret = state
        .configured_team_secret
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or(StatusCode::PRECONDITION_FAILED)?;

    let report = state
        .storage
        .sync_team_project(&req.project_id, &team_id, &state.configured_team_actor_id, &team_secret)
        .await
        .map_err(|e| {
            tracing::error!("Team sync failed for project {} team {}: {}", req.project_id, team_id, e);
            classify_storage_error(&e.to_string())
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "team_id": report.team_id,
        "project_id": report.project_id,
        "recovered_from_gap": report.recovered_from_gap,
        "recovered_entries": report.recovered_entries,
        "pushed_entries": report.pushed_entries,
        "pulled_entries": report.pulled_entries,
        "applied_operations": report.applied_operations,
        "merged_entries": report.merged_entries,
        "conflict_entries": report.conflict_entries,
        "actor_id": report.actor_id,
        "cursor": report.cursor,
        "team_namespace": report.team_namespace,
        "team_brain": report.team_brain,
        "message": format!(
            "Team sync pushed {} entries, applied {} operations, merged {} entries, and recovered {} entries for team {}",
            report.pushed_entries,
            report.applied_operations,
            report.merged_entries,
            report.recovered_entries,
            report.team_id
        )
    })))
}

async fn get_token_stats(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let session = state.token_tracker.get_session();
    let lifetime = state.token_tracker.get_lifetime().await;
    let cache_efficiency_pct = if session.embedding_cache_hits + session.embedding_cache_misses > 0 {
        (session.embedding_cache_hits as f64 / (session.embedding_cache_hits + session.embedding_cache_misses) as f64) * 100.0
    } else {
        0.0
    };
    let compression_ratio = if session.context_tokens_compiled > 0 {
        ((session.context_tokens_compiled + session.estimated_tokens_saved) as f64) / session.context_tokens_compiled as f64
    } else {
        1.0
    };
    
    let stats = crate::token::tracker::TokenStatsResponse {
        session,
        lifetime,
        cache_efficiency_pct,
        compression_ratio,
    };
    
    (StatusCode::OK, Json(stats)).into_response()
}

#[derive(Deserialize)]
struct TokenUsagePayload {
    tokens: u64,
    #[allow(dead_code)]
    task_type: Option<String>,
}

async fn record_ai_token_use(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TokenUsagePayload>,
) -> impl IntoResponse {
    state.token_tracker.session.record_ai_call(payload.tokens);
    StatusCode::OK.into_response()
}
