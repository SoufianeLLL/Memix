use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Arc;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ring::digest::{digest, SHA256};
use tracing_subscriber;

pub mod error;
pub mod retry;

const BUNDLED_LICENSE_PUBLIC_KEY_DER: &[u8] = include_bytes!("../keys/memix_public.der");

fn install_panic_hook() {
	std::panic::set_hook(Box::new(|panic_info| {
		let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
			(*s).to_string()
		} else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
			s.clone()
		} else {
			"<non-string panic payload>".to_string()
		};

		let location = if let Some(loc) = panic_info.location() {
			format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
		} else {
			"<unknown location>".to_string()
		};

		// Backtrace requires RUST_BACKTRACE=1 to be useful.
		let bt = std::backtrace::Backtrace::capture();
		tracing::error!("panic at {}: {}\n{:?}", location, payload, bt);
	}));
}

pub mod config;
pub mod brain;
pub mod server;
pub mod storage;
pub mod observer;
pub mod intelligence;
pub mod token;
pub mod search;
pub mod crypto;
pub mod sync;
pub mod git;
pub mod recorder;
pub mod rules;
pub mod migrations;
pub mod agents;
pub mod context;
pub mod learning;
pub mod license;
pub mod workspace_registry;
pub mod indexer_manager;
pub mod runtime;
mod constants;

use crate::agents::{AgentRuntime, SessionStartContext};
use crate::intelligence::autonomous::AutonomousPairProgrammer;
use crate::intelligence::predictor::ContextPredictor;
use crate::git::archaeologist::ProjectGitInsights;
use crate::observer::call_graph::CallGraph;
use crate::observer::dna::ProjectCodeDna;
use crate::observer::parser::{AstNodeFeature, AstParser};
use crate::recorder::flight::FlightRecorder;
use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};
use chrono::Utc;
use notify::EventKind;

fn derive_team_actor_id(app_config: &config::AppConfig) -> String {
	if let Some(actor_id) = app_config.team_actor_id.clone() {
		return actor_id;
	}
	let root = app_config.workspace_root.clone().unwrap_or_else(|| "workspace".to_string());
	let project = app_config.project_id.clone().unwrap_or_else(|| "default".to_string());
	let material = format!("{}::{}", root, project);
	let hash = digest(&SHA256, material.as_bytes());
	format!("actor-{}", hex::encode(&hash.as_ref()[..8]))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingBrainUpdate {
	project_id: String,
	#[serde(default)]
	upserts: Vec<brain::schema::MemoryEntry>,
	#[serde(default)]
	deletes: Vec<String>,
}

async fn process_pending_brain_update(
	storage: Arc<dyn crate::storage::StorageBackend + Send + Sync>,
	pending_path: &std::path::Path,
	pending_ack_path: Option<&std::path::Path>,
	trigger: &str,
) -> bool {
	let brain_dir = pending_path.parent().map(|p| p.to_path_buf());
	if let Some(dir) = &brain_dir {
		let _ = tokio::fs::create_dir_all(dir).await;
	}

	let pending_bytes = match tokio::fs::read(pending_path).await {
		Ok(b) => b,
		Err(e) => {
			tracing::error!("Failed reading pending.json via {}: {}", trigger, e);
			return false;
		}
	};
	let pending_trimmed = String::from_utf8_lossy(&pending_bytes);
	if pending_trimmed.trim().is_empty() {
		return false;
	}

	const MAX_PENDING_BYTES: usize = 512 * 1024;
	const MAX_UPSERTS: usize = 200;
	const MAX_DELETES: usize = 200;
	if pending_bytes.len() > MAX_PENDING_BYTES {
		tracing::error!("pending.json too large via {}: {} bytes", trigger, pending_bytes.len());
		if let Some(ack) = pending_ack_path {
			let _ = tokio::fs::write(
				ack,
				serde_json::json!({"ok": false, "error": "pending_too_large"}).to_string(),
			)
			.await;
		}
		return true;
	}

	let parsed: PendingBrainUpdate = match serde_json::from_slice(&pending_bytes) {
		Ok(v) => v,
		Err(e) => {
			tracing::error!("Invalid pending.json schema via {}: {}", trigger, e);
			if let Some(ack) = pending_ack_path {
				let _ = tokio::fs::write(
					ack,
					serde_json::json!({"ok": false, "error": format!("invalid_schema: {}", e)}).to_string(),
				)
				.await;
			}
			return true;
		}
	};

	if parsed.project_id.trim().is_empty() {
		tracing::error!("pending.json missing project_id via {}", trigger);
		if let Some(ack) = pending_ack_path {
			let _ = tokio::fs::write(
				ack,
				serde_json::json!({"ok": false, "error": "missing_project_id"}).to_string(),
			)
			.await;
		}
		return true;
	}

	if parsed.upserts.len() > MAX_UPSERTS || parsed.deletes.len() > MAX_DELETES {
		tracing::error!(
			"pending.json too many operations via {}: upserts={}, deletes={}",
			trigger,
			parsed.upserts.len(),
			parsed.deletes.len()
		);
		if let Some(ack) = pending_ack_path {
			let _ = tokio::fs::write(
				ack,
				serde_json::json!({"ok": false, "error": "too_many_operations"}).to_string(),
			)
			.await;
		}
		return true;
	}

	let mut applied_upserts: u64 = 0;
	let mut applied_deletes: u64 = 0;
	let mut failed_upserts: u64 = 0;
	let mut failed_deletes: u64 = 0;
	let mut errors: Vec<String> = Vec::new();

	for entry in parsed.upserts {
		match storage.upsert_entry(&parsed.project_id, entry).await {
			Ok(_) => applied_upserts = applied_upserts.saturating_add(1),
			Err(e) => {
				failed_upserts = failed_upserts.saturating_add(1);
				tracing::error!("pending upsert failed via {}: {}", trigger, e);
				if errors.len() < 20 {
					errors.push(format!("upsert_failed: {}", e));
				}
			}
		}
	}
	for entry_id in parsed.deletes {
		match storage.delete_entry(&parsed.project_id, &entry_id).await {
			Ok(_) => applied_deletes = applied_deletes.saturating_add(1),
			Err(e) => {
				failed_deletes = failed_deletes.saturating_add(1);
				tracing::error!("pending delete failed via {}: {}", trigger, e);
				if errors.len() < 20 {
					errors.push(format!("delete_failed: {}", e));
				}
			}
		}
	}

	if let Some(ack) = pending_ack_path {
		let _ = tokio::fs::write(
			ack,
			serde_json::json!({
				"ok": failed_upserts == 0 && failed_deletes == 0,
				"project_id": parsed.project_id,
				"upserts": applied_upserts,
				"deletes": applied_deletes,
				"failed_upserts": failed_upserts,
				"failed_deletes": failed_deletes,
				"errors": errors,
				"cleared_pending": true
			})
			.to_string(),
		)
		.await;
	}
	let _ = tokio::fs::write(pending_path, "").await;
	true
}

fn make_observer_entry(
	project_id: &str,
	id: &str,
	content: String,
	tags: Vec<String>,
	source: MemorySource,
	kind: MemoryKind,
) -> MemoryEntry {
	let now = Utc::now();
	MemoryEntry {
		id: id.to_string(),
		project_id: project_id.to_string(),
		kind,
		content,
		tags,
		source,
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

fn normalize_agent_output_key(project_id: &str, output_key: &str) -> String {
	let normalized = output_key
		.replace("brain:{project}:", "")
		.replace(&format!("brain:{}:", project_id), "")
		.replace("brain:", "")
		.trim_matches(':')
		.to_string();
	if normalized.is_empty() {
		"agent_output.json".to_string()
	} else {
		normalized
	}
}

fn build_agent_output_entry(project_id: &str, report: &crate::agents::AgentReport) -> Option<MemoryEntry> {
	let output_id = normalize_agent_output_key(project_id, &report.output_key);
	if output_id == report.entry_id {
		return None;
	}
	let now = report.generated_at;
	Some(MemoryEntry {
		id: output_id,
		project_id: project_id.to_string(),
		kind: if report.severity >= crate::agents::AgentSeverity::Warning {
			MemoryKind::Warning
		} else {
			MemoryKind::Context
		},
		content: serde_json::to_string_pretty(&report.data).unwrap_or_else(|_| "{}".to_string()),
		tags: vec!["agent-output".to_string(), report.agent_name.to_lowercase()],
		source: MemorySource::AgentExtracted,
		superseded_by: None,
		contradicts: vec![],
		parent_id: Some(report.entry_id.clone()),
		caused_by: vec![report.entry_id.clone()],
		enables: vec![],
		created_at: now,
		updated_at: now,
		access_count: 0,
		last_accessed_at: None,
	})
}

fn summarize_file_map_entry(
	file_path: &str,
	features: &[AstNodeFeature],
	graph: &crate::observer::graph::DependencyGraph,
) -> String {
	let language = features
		.first()
		.map(|feature| feature.language.clone())
		.unwrap_or_else(|| "unknown".to_string());
	let fan_in = graph.edges_in.get(file_path).map(|deps| deps.len()).unwrap_or(0);
	let fan_out = graph.edges_out.get(file_path).map(|deps| deps.len()).unwrap_or(0);
	let exported = features
		.iter()
		.filter(|feature| feature.is_exported)
		.map(|feature| feature.name.clone())
		.take(5)
		.collect::<Vec<_>>();
	let mut kinds = features
		.iter()
		.map(|feature| feature.kind.clone())
		.collect::<std::collections::HashSet<_>>()
		.into_iter()
		.collect::<Vec<_>>();
	kinds.sort();
	kinds.truncate(4);
	let mut patterns = features
		.iter()
		.flat_map(|feature| feature.pattern_tags.iter().cloned())
		.collect::<std::collections::HashSet<_>>()
		.into_iter()
		.collect::<Vec<_>>();
	patterns.sort();
	patterns.truncate(6);
	let avg_complexity = if features.is_empty() {
		0.0
	} else {
		features
			.iter()
			.map(|feature| feature.cyclomatic_complexity as f32)
			.sum::<f32>() / features.len() as f32
	};
	let mut parts = vec![format!(
		"language={} symbols={} fan_in={} fan_out={} avg_complexity={:.1}",
		language,
		features.len(),
		fan_in,
		fan_out,
		avg_complexity
	)];
	if !kinds.is_empty() {
		parts.push(format!("kinds={}", kinds.join(", ")));
	}
	if !exported.is_empty() {
		parts.push(format!("exports={}", exported.join(", ")));
	}
	if !patterns.is_empty() {
		parts.push(format!("patterns={}", patterns.join(", ")));
	}
	parts.join(" | ")
}

fn build_file_map_snapshot(
	feature_snapshots: &std::collections::HashMap<String, Vec<AstNodeFeature>>,
	graph: &crate::observer::graph::DependencyGraph,
) -> serde_json::Value {
	let mut entries = feature_snapshots.iter().collect::<Vec<_>>();
	entries.sort_by(|a, b| a.0.cmp(b.0));
	let mut map = serde_json::Map::new();
	for (file_path, features) in entries {
		map.insert(
			file_path.clone(),
			serde_json::Value::String(summarize_file_map_entry(file_path, features, graph)),
		);
	}
	serde_json::Value::Object(map)
}

fn build_known_issue_value(
	issue: String,
	file: Option<String>,
	severity: &str,
	source: &str,
	notes: Vec<String>,
) -> serde_json::Value {
	serde_json::json!({
		"status": "OPEN",
		"issue": issue,
		"file": file,
		"severity": severity,
		"source": source,
		"notes": notes,
	})
}

fn push_issue(
	issues: &mut Vec<serde_json::Value>,
	fingerprints: &mut std::collections::HashSet<String>,
	issue: serde_json::Value,
) {
	let fingerprint = serde_json::to_string(&issue).unwrap_or_default();
	if fingerprints.insert(fingerprint) {
		issues.push(issue);
	}
}

fn build_known_issues_snapshot(
	reports: &[crate::agents::AgentReport],
	recent_deleted_files: &std::collections::VecDeque<String>,
) -> serde_json::Value {
	let mut issues = Vec::new();
	let mut fingerprints = std::collections::HashSet::new();

	for file in recent_deleted_files.iter().rev().take(10) {
		push_issue(
			&mut issues,
			&mut fingerprints,
			build_known_issue_value(
				format!("Recently deleted file observed: {}", file),
				Some(file.clone()),
				"warning",
				"observer",
				vec!["Verify this deletion was intentional and update dependent files if needed.".to_string()],
			),
		);
	}

	for report in reports.iter().rev().take(64) {
		let file = report
			.data
			.get("file")
			.and_then(|value| value.as_str())
			.map(|value| value.to_string());
		let severity = match report.severity {
			crate::agents::AgentSeverity::Critical => "critical",
			crate::agents::AgentSeverity::Warning => "warning",
			crate::agents::AgentSeverity::Info => "info",
		};
		match report.agent_name.as_str() {
			"SecurityScanner" => {
				let finding_count = report
					.data
					.get("findings")
					.and_then(|value| value.as_array())
					.map(|items| items.len())
					.unwrap_or(0);
				push_issue(
					&mut issues,
					&mut fingerprints,
					build_known_issue_value(
						format!("Security scanner flagged {} finding(s)", finding_count),
						file.clone(),
						severity,
						"SecurityScanner",
						report.notifications.iter().map(|n| n.message.clone()).collect(),
					),
				);
			}
			"TestSentinel" => {
				let impacted_tests = report
					.data
					.get("impacted_tests")
					.and_then(|value| value.as_array())
					.map(|items| items.iter().filter_map(|item| item.as_str().map(|s| s.to_string())).collect::<Vec<_>>())
					.unwrap_or_default();
				push_issue(
					&mut issues,
					&mut fingerprints,
					build_known_issue_value(
						format!("Tests may need updates: {} impacted test file(s)", impacted_tests.len()),
						file.clone(),
						severity,
						"TestSentinel",
						impacted_tests,
					),
				);
			}
			"ComplexityWatcher" => {
				let risky = report
					.data
					.get("risky_functions")
					.and_then(|value| value.as_array())
					.map(|items| {
						items.iter().filter_map(|item| {
							let name = item.get("name").and_then(|value| value.as_str())?;
							let complexity = item.get("complexity").and_then(|value| value.as_u64()).unwrap_or(0);
							Some(format!("{} (complexity {})", name, complexity))
						}).collect::<Vec<_>>()
					})
					.unwrap_or_default();
				push_issue(
					&mut issues,
					&mut fingerprints,
					build_known_issue_value(
						format!("Complexity threshold exceeded in {} function(s)", risky.len()),
						file.clone(),
						severity,
						"ComplexityWatcher",
						risky,
					),
				);
			}
			"DocumentationTracker" => {
				push_issue(
					&mut issues,
					&mut fingerprints,
					build_known_issue_value(
						"Documentation may be stale after signature changes".to_string(),
						file.clone(),
						severity,
						"DocumentationTracker",
						report.notifications.iter().map(|n| n.message.clone()).collect(),
					),
				);
			}
			"CodeObserver" => {
				let breaking = report
					.data
					.get("breaking_signatures")
					.and_then(|value| value.as_array())
					.map(|items| items.len())
					.unwrap_or(0);
				if breaking > 0 {
					push_issue(
						&mut issues,
						&mut fingerprints,
						build_known_issue_value(
							format!("Breaking signature drift detected in {} symbol(s)", breaking),
							file.clone(),
							severity,
							"CodeObserver",
							report.notifications.iter().map(|n| n.message.clone()).collect(),
						),
					);
				}
			}
			_ => {
				if report.severity >= crate::agents::AgentSeverity::Warning {
					push_issue(
						&mut issues,
						&mut fingerprints,
						build_known_issue_value(
							format!("{} reported a warning", report.agent_name),
							file.clone(),
							severity,
							report.agent_name.as_str(),
							report.notifications.iter().map(|n| n.message.clone()).collect(),
						),
					);
				}
			}
		}
	}

	issues.truncate(40);
	serde_json::Value::Array(issues)
}

fn acquire_daemon_lock() -> anyhow::Result<Option<std::fs::File>> {
	let home_dir = dirs::home_dir().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
	let lock_path = home_dir
		.join(".memix")
		.join("daemon.pid");

	if let Some(parent) = lock_path.parent() {
		if !parent.exists() {
			fs::create_dir_all(parent)?;
		}
	}

	let mut attempts = 0_u8;
	loop {
		attempts = attempts.saturating_add(1);
		match OpenOptions::new().write(true).create_new(true).open(&lock_path) {
			Ok(mut file) => {
				write!(file, "{}", std::process::id())?;
				return Ok(Some(file));
			}
			Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
				let pid_str = fs::read_to_string(&lock_path).unwrap_or_default();
				if let Ok(pid) = pid_str.trim().parse::<u32>() {
					#[cfg(unix)]
					let is_alive = unsafe { libc::kill(pid as i32, 0) } == 0;
					#[cfg(not(unix))]
					let is_alive = false; // Fallback for Windows/Non-Unix

					if is_alive {
						tracing::info!(
							"Daemon already running with PID {}. Only one instance allowed. Exiting.",
							pid
						);
						return Ok(None);
					}
				}

				// Stale or invalid PID file: remove once and retry atomic creation.
				let _ = fs::remove_file(&lock_path);
				if attempts >= 2 {
					return Err(anyhow::anyhow!(
						"Failed to acquire daemon lock at {:?} after stale-lock recovery",
						lock_path
					));
				}
			}
			Err(e) => return Err(e.into()),
		}
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	// Install rustls crypto provider first - required before any TLS operations
	rustls::crypto::ring::default_provider()
		.install_default()
		.expect("Failed to install rustls crypto provider");

	// 1. Load .env from current directory first (daemon/.env when running locally)
	// This sets MEMIX_WORKSPACE_ROOT and other vars BEFORE we check them
	let _ = dotenvy::dotenv();
	
	// 2. Also try loading from workspace root if specified (for project-specific overrides)
	if let Ok(root) = std::env::var("MEMIX_WORKSPACE_ROOT") {
		let env_path = std::path::Path::new(&root).join(".env");
		if env_path.exists() {
			let _ = dotenvy::from_path(env_path);
		}
	}
	
	// 3. Fallback to extension .env for dev convenience
	let _ = dotenvy::from_filename("../extension/.env");

    // Initialize structured logging
    tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.init();
	install_panic_hook();
    tracing::info!("Starting Memix memory-bridge daemon...");

	let _lock = acquire_daemon_lock()?;
	if _lock.is_none() {
		return Ok(());
	}

    // 1. Load config (JSON vs Redis)
    let app_config = config::load_config()?;

    // 2. Initialize the generic storage backend implementation
    let storage_backend = storage::initialize_storage(&app_config).await?;
	let autonomous = Arc::new(tokio::sync::Mutex::new(AutonomousPairProgrammer::new()));
	let recorder = Arc::new(FlightRecorder::new(2000));
	let code_dna = Arc::new(tokio::sync::Mutex::new(ProjectCodeDna::default()));
	let predictor = Arc::new(ContextPredictor::new());
	let call_graph = Arc::new(tokio::sync::Mutex::new(CallGraph::new()));
	let agent_workspace_root = app_config
		.workspace_root
		.clone()
		.map(std::path::PathBuf::from)
		.unwrap_or(std::env::current_dir()?);
	let agent_runtime = Arc::new(tokio::sync::Mutex::new(AgentRuntime::new(agent_workspace_root.clone())));
	let git_insights = Arc::new(tokio::sync::Mutex::new(ProjectGitInsights {
		available: false,
		repo_root: None,
		hot_files: vec![],
		stable_files: vec![],
		recent_authors: vec![],
		summary: vec!["git_archaeology_unavailable".to_string()],
	}));
	let project_id_for_events = app_config
		.project_id
		.clone()
		.unwrap_or_else(|| "default".to_string());
	let license_cache_path = app_config
		.data_dir
		.clone()
		.map(std::path::PathBuf::from)
		.unwrap_or_else(|| std::path::PathBuf::from(".memix"))
		.join("license_cache.json");
	let default_license_public_key = if BUNDLED_LICENSE_PUBLIC_KEY_DER.is_empty() {
		None
	} else {
		Some(STANDARD.encode(BUNDLED_LICENSE_PUBLIC_KEY_DER))
	};
	let license_validator = Arc::new(license::LicenseValidator::new(
		app_config.license_public_key.clone().or(default_license_public_key),
		app_config.license_server_url.clone(),
		license_cache_path,
	)?);

	let daemon_config = Arc::new(tokio::sync::RwLock::new(
		server::DaemonConfig::load(app_config.workspace_root.as_deref())
	));
	let team_actor_id = derive_team_actor_id(&app_config);

	let data_dir = app_config
		.data_dir
		.clone()
		.map(std::path::PathBuf::from)
		.unwrap_or_else(|| std::path::PathBuf::from(".memix"));

	// ── Start with empty EmbeddingStore and TokenTracker synchronously ──────────
	// Both are initialized to empty/default here so build_router can proceed
	// immediately with no Redis I/O. Background tasks below will load the real
	// state from SQLite after the socket is already bound and serving
	// health checks. This is the only way to guarantee the socket binds within
	// the extension's 5-second health check window.
	//
	// IMPORTANT: Use workspace_root from app_config so brain is stored IN the project
	// at {workspace_root}/.memix/brain.db, not in global data_dir
	let workspace_root_for_embedding = app_config
		.workspace_root
		.clone()
		.map(std::path::PathBuf::from)
		.unwrap_or_else(|| data_dir.join(&project_id_for_events));
	let embedding_store = crate::observer::embedding_store::EmbeddingStore::empty(
		&project_id_for_events,
		&workspace_root_for_embedding,
	);

	// Initialize multi-tenant workspace registry
	let workspace_registry = Arc::new(tokio::sync::Mutex::new(
		crate::workspace_registry::WorkspaceRegistry::new()
	));

	// Initialize multi-workspace token tracker manager (per-workspace stats)
	let token_tracker_manager = Arc::new(tokio::sync::Mutex::new(
		crate::token::TokenTrackerManager::new(data_dir.clone())
	));

	// Get tracker for initial workspace (needed for indexer/observer)
	let initial_tracker = token_tracker_manager.lock().await.get_or_create(&project_id_for_events).await;

	// Initialize multi-workspace indexer manager
	let indexer_manager = Arc::new(tokio::sync::Mutex::new(
		crate::indexer_manager::IndexerManager::new(
			storage_backend.clone(),
			embedding_store.clone(),
			initial_tracker.clone(),
			data_dir.clone(), // indexer_manager still uses data_dir for fallback
		)
	));

	// Initialize multi-workspace observer manager (file watchers)
	let observer_manager = Arc::new(tokio::sync::Mutex::new(
		crate::observer::observer_manager::ObserverManager::new(
			storage_backend.clone(),
			embedding_store.clone(),
			initial_tracker,
		)
	));

	// If a workspace was provided at startup, register it as the initial workspace
	if let (Some(root), Some(pid)) = (&app_config.workspace_root, &app_config.project_id) {
		workspace_registry.lock().await.register(pid.clone(), root.clone());
		tracing::info!("Initial workspace registered: {} ({})", pid, root);
	}

    // 3. Build the Axum server routes and pass the shared storage state.
	// Happens before any Redis I/O so the socket can bind immediately after.
    let app = server::build_router(
		storage_backend.clone(),
		autonomous.clone(),
		recorder.clone(),
		code_dna.clone(),
		predictor.clone(),
		call_graph.clone(),
		agent_runtime.clone(),
		git_insights.clone(),
		workspace_registry.clone(),
		indexer_manager.clone(),
		observer_manager.clone(),
		token_tracker_manager.clone(),
		app_config.workspace_root.clone(),
		app_config.project_id.clone(),
		app_config.team_id.clone(),
		team_actor_id,
		app_config.team_secret.clone(),
		license_validator,
		daemon_config.clone(),
		embedding_store.clone(),
	);

	// ─── BIND UNIX SOCKET ────────────────────────────────────────────────────
	// All pre-bind work is now timeout-bounded. Migrations, EmbeddingStore, and
	// TokenTracker all have hard timeouts so this point is reached within ~3s
	// of daemon startup, well inside the extension's 5-second health check window.
	#[cfg(unix)]
	let socket_path = {
		let home_dir = dirs::home_dir()
			.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
		let p = home_dir.join(".memix").join("daemon.sock");
		if let Some(parent) = p.parent() {
			if !parent.exists() { fs::create_dir_all(parent)?; }
		}
		if p.exists() { fs::remove_file(&p)?; }
		p
	};

	#[cfg(unix)]
	let unix_listener = tokio::net::UnixListener::bind(&socket_path)?;

	// Signals deferred startup tasks that the socket is bound and the daemon
	// is accepting connections. Tasks wait on this rather than sleeping for
	// an arbitrary duration — they start exactly when the daemon is ready.
	let startup_ready = Arc::new(tokio::sync::Notify::new());

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
		tracing::info!("Daemon listening on Unix Socket at {:?}", socket_path);
		startup_ready.notify_waiters();
	}

	// ─── Run the Unix accept loop in a spawned task ────────────────────────────
	// This means the health check can succeed immediately while the rest of
	// startup (migrations, session agents, watcher) continues below.
	#[cfg(unix)]
	{
		use hyper_util::rt::{TokioExecutor, TokioIo};
		use hyper_util::server::conn::auto::Builder;
		use hyper_util::service::TowerToHyperService;
		let unix_app = app.clone();
		tokio::spawn(async move {
			loop {
				match unix_listener.accept().await {
					Ok((socket, _)) => {
						let svc = unix_app.clone();
						tokio::spawn(async move {
							let io = TokioIo::new(socket);
							if let Err(e) = Builder::new(TokioExecutor::new())
								.serve_connection(io, TowerToHyperService::new(svc))
								.await
							{
								tracing::debug!("UDS connection error: {}", e);
							}
						});
					}
					Err(e) => tracing::debug!("Unix accept error: {}", e),
				}
			}
		});
	}

	let agent_runtime_clone = agent_runtime.clone();
	let storage_backend_clone = storage_backend.clone();
	let project_id_clone = project_id_for_events.clone();
	let workspace_root_clone = agent_workspace_root.to_string_lossy().to_string();

	tokio::spawn(async move {
		let mut runtime = agent_runtime_clone.lock().await;
		for report in runtime.process_session_start(&SessionStartContext {
			project_id: project_id_clone.clone(),
			workspace_root: workspace_root_clone,
		}) {
			let kind = if report.severity >= crate::agents::AgentSeverity::Warning {
				MemoryKind::Warning
			} else {
				MemoryKind::Context
			};
			let entry = MemoryEntry {
				id: report.entry_id.clone(),
				project_id: project_id_clone.clone(),
				kind,
				content: serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
				tags: vec!["agent".to_string(), report.agent_name.to_lowercase()],
				source: MemorySource::AgentExtracted,
				superseded_by: None,
				contradicts: vec![],
				parent_id: None,
				caused_by: vec![],
				enables: vec![],
				created_at: report.generated_at,
				updated_at: report.generated_at,
				access_count: 0,
				last_accessed_at: None,
			};
			let _ = storage_backend_clone.upsert_entry(&project_id_clone, entry).await;
			if let Some(output_entry) = build_agent_output_entry(&project_id_clone, &report) {
				let _ = storage_backend_clone.upsert_entry(&project_id_clone, output_entry).await;
			}
		}
	});

    // 3.5 Bind and serve HTTP over localhost TCP as well (useful for dev + multi-IDE compatibility)
    let port = app_config.port.unwrap_or(3456);
    let tcp_app = app.clone();
    tokio::spawn(async move {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!("Daemon listening on TCP at http://{}", addr);
                if let Err(e) = axum::serve(listener, tcp_app).await {
                    tracing::error!("TCP server error: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to bind TCP listener on {}: {}", addr, e);
            }
        }
    });

	// ── Deferred startup tasks — run after socket is bound ─────────────────────
	// These tasks hit Redis and disk. By spawning them here, the socket is
	// already accepting health check connections before any I/O begins.
	// If Redis is slow (throttled free tier, network hiccup), the daemon
	// remains healthy from the extension's perspective while data loads quietly.

	// Deferred: run schema migrations for the project
	if let Some(project_id) = app_config.project_id.clone() {
		let storage_for_migrations = storage_backend.clone();
		let startup_ready_migrations = startup_ready.clone();
		tokio::spawn(async move {
			// Wait for daemon socket to be bound before hitting Redis
			startup_ready_migrations.notified().await;
			match tokio::time::timeout(
				std::time::Duration::from_secs(10),
				migrations::run_project_migrations(storage_for_migrations, &project_id),
			).await {
				Ok(Ok(report)) => tracing::info!(
					"Migrations complete for {} (schema_version={}, migrated_entries={})",
					report.project_id, report.schema_version, report.migrated_entries
				),
				Ok(Err(e)) => tracing::error!("Migration run failed: {}", e),
				Err(_) => tracing::warn!("Migrations timed out after 10s — will retry on next start"),
			}
		});
	}

	// Deferred: load TokenTracker lifetime totals from disk
	// Session counters already start at zero which is correct for a new session.
	// Loading the lifetime file just restores the historical totals display.
	{
		let token_tracker_manager_for_load = token_tracker_manager.clone();
		let project_id_for_load = project_id_for_events.clone();
		let startup_ready_tokens = startup_ready.clone();
		tokio::spawn(async move {
			startup_ready_tokens.notified().await;
			// Get the tracker for the initial project and load its lifetime data
			let tracker = token_tracker_manager_for_load.lock().await.get_or_create(&project_id_for_load).await;
			match tokio::time::timeout(
				std::time::Duration::from_secs(3),
				crate::token::tracker::TokenTracker::load_lifetime_into(
					&tracker,
					&project_id_for_load,
					&std::path::PathBuf::from("."),
				),
			).await {
				Ok(Ok(())) => tracing::debug!("TokenTracker lifetime totals loaded"),
				Ok(Err(e)) => tracing::warn!("TokenTracker lifetime load failed: {} — session stats only", e),
				Err(_) => tracing::warn!("TokenTracker lifetime load timed out — session stats only"),
			}
		});
	}

	// Deferred: load EmbeddingStore from disk (and Redis fallback)
	// If the binary file exists this is fast (< 50ms). Redis fallback only
	// runs when binary file is absent — new machine or first run.
	{
		let embedding_store_for_load = embedding_store.clone();
		let workspace_root_for_emb = workspace_root_for_embedding.clone();
		let project_id_for_emb = project_id_for_events.clone();
		let startup_ready_embeddings = startup_ready.clone();
		tokio::spawn(async move {
			startup_ready_embeddings.notified().await;
			// Load embeddings from SQLite (stored at {workspace_root}/.memix/brain.db)
			match crate::observer::embedding_store::EmbeddingStore::load(&project_id_for_emb, &workspace_root_for_emb).await {
				Ok(loaded_store) => {
					// Copy loaded data into the existing store
					embedding_store_for_load.copy_from(&loaded_store).await;
					tracing::info!("EmbeddingStore: loaded {} vectors from SQLite", loaded_store.len().await);
				}
				Err(e) => {
					tracing::warn!("EmbeddingStore load failed: {} — background indexer will rebuild", e);
				}
			}
		});
	}

	// Spawn background indexer for initial workspace via indexer_manager
	// (Multi-workspace: additional indexers are spawned via workspace registration API)
	if let (Some(root), Some(pid)) = (&app_config.workspace_root, &app_config.project_id) {
		let mut im = indexer_manager.lock().await;
		im.spawn_for_workspace(pid.clone(), root.clone());
		
		// Spawn observer for initial workspace via observer_manager
		let mut om = observer_manager.lock().await;
		om.spawn_for_workspace(pid.clone(), root.clone());
	}

	// Periodic flush: token stats, embeddings, and warning cleanup
	let flush_token_tracker_manager = token_tracker_manager.clone();
	let flush_embedding_store = embedding_store.clone();
	let flush_storage = storage_backend.clone();
	let flush_project_id = project_id_for_events.clone();
	tokio::spawn(async move {
		let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
		loop {
			interval.tick().await;
			// Flush all workspace token trackers
			flush_token_tracker_manager.lock().await.flush_all().await;
			
			// Prune stale warning entries: keep only the 10 most recent per project,
			// and never keep any older than 48 hours. Warnings are diagnostic signals,
			// not permanent memory — they should not accumulate indefinitely.
			if let Ok(entries) = flush_storage.get_entries(&flush_project_id).await {
				let cutoff = chrono::Utc::now() - chrono::Duration::hours(48);
				let mut warnings: Vec<_> = entries.into_iter()
					.filter(|e| e.kind == MemoryKind::Warning &&
								e.id.starts_with("warning_signature_"))
					.collect();
				
				// Sort newest first
				warnings.sort_by(|a, b| b.created_at.cmp(&a.created_at));
				
				// Delete anything beyond the 10 most recent or older than 48h
				for entry in warnings.iter().skip(10).chain(
					warnings.iter().filter(|e| e.created_at < cutoff)
				) {
					let _ = flush_storage.delete_entry(&flush_project_id, &entry.id).await;
				}
			}

			// ── MULTI-IDE NOTE ────────────────────────────────────────────────────
			// When multi-IDE support is built, change flush_disk_only() back to
			// flush() and pass the actual Redis client so the embedding store syncs
			// to Redis for sharing between IDE instances on the same project.
			// ─────────────────────────────────────────────────────────────────────
			let _ = flush_embedding_store.flush_disk_only().await;
		}
	});

	// ─── Multi-Workspace Observer Event Processor ──────────────────────────────
	// The ObserverManager spawns per-workspace file watchers that send TaggedEvents.
	// This single processor loop receives tagged events and dispatches to the correct
	// WorkspaceProcessor based on project_id.
	
	// Take the event receiver from ObserverManager (can only be done once)
	let event_rx = observer_manager.lock().await.take_event_receiver();
	
	// Shared references for the event processor
	let processor_storage = storage_backend.clone();
	let processor_autonomous = autonomous.clone();
	let processor_recorder = recorder.clone();
	let processor_predictor = predictor.clone();
	let processor_call_graph = call_graph.clone();
	let processor_agent_runtime = agent_runtime.clone();
	let processor_config = daemon_config.clone();
	let processor_observer_manager = observer_manager.clone();
	
	// Spawn the event processor loop
	tokio::spawn(async move {
		let Some(mut event_rx) = event_rx else {
			tracing::warn!("ObserverManager event receiver not available — observer pipeline disabled");
			return;
		};
		
		let mut parser = match AstParser::new() {
			Ok(p) => p,
			Err(e) => {
				tracing::error!("Failed to initialize AstParser for observer: {}", e);
				return;
			}
		};
		
		tracing::info!("Multi-workspace observer event processor started");
		
		while let Some(tagged_event) = event_rx.recv().await {
			let project_id = tagged_event.project_id.clone();
			let event = tagged_event.event;
			
			// Check if brain is paused for this workspace
			let (is_paused, config_snapshot) = {
				let cfg = processor_config.read().await;
				(cfg.brain_paused, cfg.clone())
			};
			
			if is_paused {
				tracing::debug!("Brain is paused, ignoring event for {}", project_id);
				continue;
			}
			
			tracing::debug!("Observer event for {}: {:?}", project_id, event.kind);
			
			// Get or create the workspace processor
			{
				let mut om = processor_observer_manager.lock().await;
				// Ensure processor exists
				if om.get_processor(&project_id).is_none() {
					// Processor should have been created during spawn_for_workspace
					// If missing, skip this event
					tracing::warn!("No processor found for project {} — skipping event", project_id);
					continue;
				}
				// We need to process inside the lock, so we'll handle it here
				// Process deletion events
				if matches!(&event.kind, EventKind::Remove(_)) {
					for path in &event.paths {
						om.get_processor(&project_id).unwrap().process_deletion(
							path,
							&processor_storage,
							&processor_autonomous,
							&processor_recorder,
							&processor_call_graph,
						).await;
					}
				} else {
					// Process modification events
					for path in &event.paths {
						om.get_processor(&project_id).unwrap().process_modification(
							path,
							&mut parser,
							&processor_storage,
							&processor_autonomous,
							&processor_recorder,
							&processor_predictor,
							&processor_call_graph,
							&processor_agent_runtime,
							&config_snapshot,
						).await;
					}
				}
			}
		}
		
		tracing::info!("Multi-workspace observer event processor stopped");
	});

	// ─── Per-Workspace pending.json Poller ──────────────────────────────────────
	// Polls pending.json for each registered workspace and processes brain writebacks.
	// This is now workspace-aware: each workspace has its own pending.json path.
	
	let poller_storage = storage_backend.clone();
	let poller_observer_manager = observer_manager.clone();
	let poller_workspace_registry = workspace_registry.clone();
	
	tokio::spawn(async move {
		let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
		let mut last_processed_mtimes: std::collections::HashMap<String, std::time::SystemTime> = std::collections::HashMap::new();
		
		loop {
			interval.tick().await;
			
			// Get all registered workspaces
			let workspaces: Vec<(String, String)> = {
				let registry = poller_workspace_registry.lock().await;
				registry.list().into_iter()
					.map(|e| (e.project_id.clone(), e.workspace_root.clone()))
					.collect()
			};
			
			for (project_id, workspace_root) in workspaces {
				let pending_path = std::path::PathBuf::from(&workspace_root)
					.join(".memix")
					.join("brain")
					.join("pending.json");
				
				let metadata = match tokio::fs::metadata(&pending_path).await {
					Ok(m) => m,
					Err(_) => continue,
				};
				
				let modified = match metadata.modified() {
					Ok(m) => m,
					Err(_) => continue,
				};
				
				// Check if we've already processed this version
				let should_process = match last_processed_mtimes.get(&project_id) {
					Some(last) => modified > *last,
					None => true,
				};
				
				if !should_process {
					continue;
				}
				
				// Process pending.json for this workspace
				if process_pending_brain_update(
					poller_storage.clone(),
					&pending_path,
					Some(&pending_path.with_extension("ack.json")),
					&format!("poller:{}", project_id),
				).await {
					last_processed_mtimes.insert(project_id, modified);
				}
			}
		}
	});



	std::future::pending::<()>().await;
	#[allow(unreachable_code)]
	Ok(())
}
