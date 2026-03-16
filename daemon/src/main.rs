use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Arc;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ring::digest::{digest, SHA256};
use tracing_subscriber;

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

use crate::agents::{AgentRuntime, FileSaveAgentContext, SessionStartContext};
use crate::intelligence::autonomous::AutonomousPairProgrammer;
use crate::intelligence::intent::IntentEngine;
use crate::intelligence::predictor::ContextPredictor;
use crate::git::archaeologist::{GitArchaeologist, ProjectGitInsights};
use crate::observer::differ::AstDiffer;
use crate::observer::dna::{DnaRuleConfig, ProjectCodeDna};
use crate::observer::imports::{extract_imports, signature_head};
use crate::observer::parser::{AstNodeFeature, AstParser};
use crate::recorder::flight::{FlightRecorder, SessionEvent};
use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};
use crate::token::engine::TokenEngine;
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
	// 1. Prioritize .env from workspace root if provided
	if let Ok(root) = std::env::var("MEMIX_WORKSPACE_ROOT") {
		let env_path = std::path::Path::new(&root).join(".env");
		if env_path.exists() {
			let _ = dotenvy::from_path(env_path);
		}
	}
	
	// 2. Fallback to standard dotenv or explicit relative path
	if dotenvy::dotenv().is_err() {
		let _ = dotenvy::from_filename("../extension/.env");
	}

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

	if let Some(project_id) = app_config.project_id.clone() {
		match migrations::run_project_migrations(storage_backend.clone(), &project_id).await {
			Ok(report) => tracing::info!(
				"Applied migrations for project {} (schema_version={}, migrated_entries={})",
				report.project_id,
				report.schema_version,
				report.migrated_entries
			),
			Err(e) => tracing::error!("Migration run failed for project {}: {}", project_id, e),
		}
	}

    let daemon_config = Arc::new(tokio::sync::RwLock::new(
        server::DaemonConfig::load(app_config.workspace_root.as_deref())
    ));
	let team_actor_id = derive_team_actor_id(&app_config);

    // 3. Build the Axum server routes and pass the shared storage state
    let app = server::build_router(
		storage_backend.clone(),
		autonomous.clone(),
		recorder.clone(),
		code_dna.clone(),
		predictor.clone(),
		agent_runtime.clone(),
		git_insights.clone(),
		app_config.workspace_root.clone(),
		app_config.team_id.clone(),
		team_actor_id,
		app_config.team_secret.clone(),
		license_validator,
        daemon_config.clone(),
	);

	{
		let mut runtime = agent_runtime.lock().await;
		for report in runtime.process_session_start(&SessionStartContext {
			project_id: project_id_for_events.clone(),
			workspace_root: agent_workspace_root.to_string_lossy().to_string(),
		}) {
			let kind = if report.severity >= crate::agents::AgentSeverity::Warning {
				MemoryKind::Warning
			} else {
				MemoryKind::Context
			};
			let entry = MemoryEntry {
				id: report.entry_id.clone(),
				project_id: project_id_for_events.clone(),
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
			let _ = storage_backend.upsert_entry(&project_id_for_events, entry).await;
			if let Some(output_entry) = build_agent_output_entry(&project_id_for_events, &report) {
				let _ = storage_backend.upsert_entry(&project_id_for_events, output_entry).await;
			}
		}
	}

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

    // 4. Bind and serve HTTP over Unix Domain Sockets
    let home_dir = dirs::home_dir().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    let socket_path = home_dir.join(".memix").join("daemon.sock");
    if let Some(parent) = socket_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    
    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

    let listener = tokio::net::UnixListener::bind(&socket_path)?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
    }

    tracing::info!("Daemon listening on Unix Socket at {:?}", socket_path);

    // Hyper-util accept loop since Axum 0.7's `serve` convenience method only takes TcpListener
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use hyper_util::server::conn::auto::Builder;
    use hyper_util::service::TowerToHyperService;

    // 5. Spawn the AST Background Observer
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    
    // Watch directory is dynamically resolved via the workspace mounting path or defaults to execution context.
    let watch_dir = app_config
        .workspace_root
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
	let archaeology_root = watch_dir.clone();
    tokio::spawn(async move {
        let _ = observer::watcher::start_watcher(watch_dir.to_string_lossy().to_string(), tx).await;
    });

    // Handle AST Events concurrently without blocking the HTTP listener
	let pending_path = app_config
		.workspace_root
		.clone()
		.map(|root| std::path::PathBuf::from(root).join(".memix").join("brain").join("pending.json"));
 	let pending_ack_path = app_config
		.workspace_root
		.clone()
		.map(|root| std::path::PathBuf::from(root).join(".memix").join("brain").join("pending.ack.json"));
	let pending_path_poll = pending_path.clone();
	let pending_ack_path_poll = pending_ack_path.clone();
	if pending_path.is_none() {
		tracing::warn!(
			"No workspace_root configured — pending.json writeback and observer pipeline disabled. Set MEMIX_WORKSPACE_ROOT or workspace_root in config.toml."
		);
	}
 	let pending_processing_lock = Arc::new(tokio::sync::Mutex::new(()));
 	let pending_processing_lock_for_poll = pending_processing_lock.clone();
 	let storage_for_pending_poll = storage_backend.clone();
 	let storage_for_events = storage_backend.clone();
 	let autonomous_for_events = autonomous.clone();
	let recorder_for_events = recorder.clone();
	let code_dna_for_events = code_dna.clone();
	let predictor_for_events = predictor.clone();
	let agent_runtime_for_events = agent_runtime.clone();
	let git_insights_for_events = git_insights.clone();
 	let project_id_for_events = project_id_for_events.clone();
 	let dna_rules_root = archaeology_root.clone();
 	let config_for_events = daemon_config.clone();

    tokio::spawn(async move {
		let Some(pending_path_poll) = pending_path_poll else {
			return;
		};
		let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
		let mut last_processed_mtime: Option<std::time::SystemTime> = None;
		loop {
			interval.tick().await;
			let metadata = match tokio::fs::metadata(&pending_path_poll).await {
				Ok(metadata) => metadata,
				Err(_) => continue,
			};
			let modified = match metadata.modified() {
				Ok(modified) => modified,
				Err(e) => {
					tracing::debug!("Failed reading pending.json mtime: {}", e);
					continue;
				}
			};
			if last_processed_mtime.is_some_and(|last| modified <= last) {
				continue;
			}
			let _guard = pending_processing_lock_for_poll.lock().await;
			if process_pending_brain_update(
				storage_for_pending_poll.clone(),
				&pending_path_poll,
				pending_ack_path_poll.as_deref(),
				"poller",
			)
			.await
			{
				last_processed_mtime = Some(modified);
			}
		}
	});

    tokio::spawn(async move {
		// Find the real git root by walking up from workspace_root
		let git_root = {
			let mut dir = archaeology_root.clone();
			let mut found: Option<std::path::PathBuf> = None;
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
		let archaeologist = if let Some(ref root) = git_root {
			GitArchaeologist::open(root).ok()
		} else {
			None
		};
		if archaeologist.is_none() {
			tracing::debug!("Git archaeology unavailable for workspace {:?} (no .git found)", archaeology_root);
		}

 		let mut parser = match AstParser::new() {
			Ok(p) => p,
			Err(e) => {
				tracing::error!("Failed to initialize AstParser: {}", e);
				return;
			}
		};
		let mut cache: std::collections::HashMap<String, (Vec<u8>, Option<(tree_sitter::Tree, tree_sitter::Language)>)> = std::collections::HashMap::new();
		let mut feature_snapshots: std::collections::HashMap<String, Vec<AstNodeFeature>> = std::collections::HashMap::new();
		let mut last_observer_persist = std::time::Instant::now();
		let mut recent_deleted_files: std::collections::VecDeque<String> = std::collections::VecDeque::with_capacity(32);

        while let Some(event) = rx.recv().await {
			let cfg = config_for_events.read().await;
			if cfg.brain_paused {
				tracing::debug!("Brain is paused, ignoring AST daemon event");
				continue;
			}
			drop(cfg); // Release the read lock before async work
            tracing::debug!("AST Daemon Event detected: {:?}", event);

 			let Some(pending_path) = pending_path.as_ref() else {
				continue;
			};
			let matches_pending = event.paths.iter().any(|p| p == pending_path);

			// --- Option C writeback: pending.json ---
			if matches_pending {
				let _guard = pending_processing_lock.lock().await;
				let _ = process_pending_brain_update(
					storage_for_events.clone(),
					pending_path,
					pending_ack_path.as_deref(),
					"watcher",
				)
				.await;
				continue;
			}

			// --- Observer pipeline: compute semantic diffs + dependency graph ---
			if matches!(&event.kind, EventKind::Remove(_)) {
				for path in &event.paths {
					if path.exists() {
						continue;
					}
					let key = path.to_string_lossy().to_string();
					let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
					if !AstParser::is_supported(ext) && !cache.contains_key(&key) && !feature_snapshots.contains_key(&key) {
						continue;
					}
					cache.remove(&key);
					feature_snapshots.remove(&key);
					{
						let mut autonomous = autonomous_for_events.lock().await;
						autonomous.dependency_graph.remove_file(&key);
					}
					while recent_deleted_files.len() >= 32 {
						recent_deleted_files.pop_front();
					}
					recent_deleted_files.push_back(key.clone());
					recorder_for_events.record_event(SessionEvent::AstMutation { file: key.clone(), nodes_changed: 0 });
					tracing::debug!("Observer removed file from live graph: {}", key);
				}
			}

			for path in &event.paths {
				if !path.is_file() {
					continue;
				}
				let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
				if !AstParser::is_supported(ext) {
					continue;
				}

				let new_bytes = match tokio::fs::read(path).await {
					Ok(b) => b,
					Err(_) => continue,
				};
				let new_tree = match parser.parse_file(path) {
					Ok(t) => t,
					Err(e) => {
						tracing::error!("AST parse failed for {:?}: {}", path, e);
						continue;
					}
				};
				let Some(new_tree) = new_tree else { continue; };

				let key = path.to_string_lossy().to_string();
				let (old_bytes, old_tree) = cache
					.get(&key)
					.cloned()
					.unwrap_or_else(|| (Vec::new(), None));

				let diff = AstDiffer::compute_diff(
					&key,
					&parser,
					old_tree.as_ref(),
					&new_tree,
					&old_bytes,
					&new_bytes,
					ext,
				);
				let new_features = parser.extract_features(&new_tree.0, new_tree.1.clone(), &new_bytes, ext);
				let new_features_for_agents = new_features.clone();

				let mut breaking_signatures: Vec<(String, String, String)> = Vec::new();
				if let Some((old_tree_ref, old_lang)) = old_tree.as_ref() {
					let old_features = parser.extract_features(old_tree_ref, old_lang.clone(), &old_bytes, ext);
					let old_map: std::collections::HashMap<String, crate::observer::parser::AstNodeFeature> = old_features
						.into_iter()
						.map(|f| (f.name.clone(), f))
						.collect();

					for nf in &new_features {
						if let Some(of) = old_map.get(&nf.name) {
							let old_sig = signature_head(&of.body);
							let new_sig = signature_head(&nf.body);
							if !old_sig.is_empty() && !new_sig.is_empty() && old_sig != new_sig {
								breaking_signatures.push((nf.name.clone(), old_sig, new_sig));
							}
						}
					}
				}

				cache.insert(key.clone(), (new_bytes.clone(), Some(new_tree)));
				feature_snapshots.insert(key.clone(), new_features);

				let nodes_changed = diff.nodes_added.len() + diff.nodes_removed.len() + diff.nodes_modified.len();
				recorder_for_events.record_event(SessionEvent::AstMutation { file: key.clone(), nodes_changed });
				predictor_for_events.record_activity(&key, nodes_changed).await;

				let intent = IntentEngine::classify_intent(&diff);
				let intent_confidence = IntentEngine::confidence(&diff);
				recorder_for_events.record_event(SessionEvent::IntentDetected {
					intent_type: intent.as_str().to_string(),
				});

				if !breaking_signatures.is_empty() {
					let now = Utc::now();
					let details = breaking_signatures
						.iter()
						.map(|(name, old_sig, new_sig)| format!("- {}: '{}' -> '{}'", name, old_sig, new_sig))
						.collect::<Vec<_>>()
						.join("\n");
					let warning_entry = MemoryEntry {
						id: format!(
							"warning_signature_{}_{}.json",
							now.timestamp_millis(),
							uuid::Uuid::new_v4()
						),
						project_id: project_id_for_events.clone(),
						kind: MemoryKind::Warning,
						content: format!(
							"Potential breaking signature change detected in {}:\n{}",
							key,
							details
						),
						tags: vec![
							"warning".to_string(),
							"semantic-diff".to_string(),
							"signature-change".to_string(),
						],
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
					let _ = storage_for_events
						.upsert_entry(&project_id_for_events, warning_entry)
						.await;
				}

				let imports = extract_imports(ext, &String::from_utf8_lossy(&new_bytes));
				let diff_for_agents = diff.clone();
				let (intent_entry_json, related_files_for_agents, graph_snapshot_for_agents) = {
					let mut a = autonomous_for_events.lock().await;
					a.update_dependency_graph(&key, &imports);
					let related_files = {
						let mut files = Vec::new();
						if let Some(deps) = a.dependency_graph.edges_out.get(&key) {
							files.extend(deps.iter().cloned());
						}
						if let Some(deps) = a.dependency_graph.edges_in.get(&key) {
							files.extend(deps.iter().cloned());
						}
						files.sort();
						files.dedup();
						files.truncate(8);
						files
					};
					let preloaded_memory_ids = vec![
						"observer_graph.json".to_string(),
						"observer_changes.json".to_string(),
						"file_map.json".to_string(),
						"known_issues.json".to_string(),
					];

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
					))
						.unwrap_or(0);
					predictor_for_events.preload_context(
						&key,
						preloaded_memory_ids.clone(),
						related_files.clone(),
						token_weight,
						intent.as_str().to_string(),
						intent_confidence,
						rationale.clone(),
					).await;
					let intent_entry_json = predictor_for_events
						.get_cached_context(&key)
						.await
						.and_then(|snapshot| serde_json::to_string_pretty(&snapshot).ok());
					a.record_change(key.clone(), diff);
					(intent_entry_json, related_files, a.dependency_graph.clone())
				};

				let recent_change_files = {
					let a = autonomous_for_events.lock().await;
					a.change_history
						.iter()
						.rev()
						.take(20)
						.map(|change| change.file.clone())
						.collect::<Vec<_>>()
				};
				let reports = {
					let mut runtime = agent_runtime_for_events.lock().await;
					runtime.process_file_save(&FileSaveAgentContext {
						project_id: project_id_for_events.clone(),
						file_path: key.clone(),
						file_content: String::from_utf8_lossy(&new_bytes).to_string(),
						diff: diff_for_agents,
						features: new_features_for_agents,
						dependency_graph: graph_snapshot_for_agents,
						intent_type: intent.as_str().to_string(),
						intent_confidence,
						breaking_signatures: breaking_signatures.clone(),
						recent_change_files,
					})
				};
				for report in reports {
					let kind = if report.severity >= crate::agents::AgentSeverity::Warning {
						MemoryKind::Warning
					} else {
						MemoryKind::Context
					};
					let entry = MemoryEntry {
						id: report.entry_id.clone(),
						project_id: project_id_for_events.clone(),
						kind,
						content: serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
						tags: vec![
							"agent".to_string(),
							report.agent_name.to_lowercase(),
							intent.as_str().to_string(),
						],
						source: MemorySource::AgentExtracted,
						superseded_by: None,
						contradicts: vec![],
						parent_id: None,
						caused_by: vec![],
						enables: related_files_for_agents.clone(),
						created_at: report.generated_at,
						updated_at: report.generated_at,
						access_count: 0,
						last_accessed_at: None,
					};
					let _ = storage_for_events
						.upsert_entry(&project_id_for_events, entry)
						.await;
					if let Some(output_entry) = build_agent_output_entry(&project_id_for_events, &report) {
						let _ = storage_for_events
							.upsert_entry(&project_id_for_events, output_entry)
							.await;
					}
				}

				// Persist observer snapshots into brain keys (throttled)
				if last_observer_persist.elapsed() >= std::time::Duration::from_secs(2) {
					last_observer_persist = std::time::Instant::now();
					let recent_reports = {
						let runtime = agent_runtime_for_events.lock().await;
						runtime.recent_reports()
					};
					let (graph_json, changes_json, dna_json, dna_snapshot, git_json, git_snapshot, file_map_json, known_issues_json) = {
						let a = autonomous_for_events.lock().await;
						let graph_json = serde_json::to_string_pretty(&a.dependency_graph)
							.unwrap_or_else(|_| "{}".to_string());
						let changes: Vec<_> = a
							.change_history
							.iter()
							.rev()
							.take(25)
							.map(|c| c.diff.clone())
							.collect();
						let changes_json = serde_json::to_string_pretty(&changes)
							.unwrap_or_else(|_| "[]".to_string());
						let recent_change_files = a
							.change_history
							.iter()
							.rev()
							.take(50)
							.map(|c| c.file.clone())
							.collect::<Vec<_>>();
						let dna_rules = DnaRuleConfig::resolve_for_workspace(&dna_rules_root);
						let tracked_git_files = recent_change_files
							.iter()
							.cloned()
							.chain(feature_snapshots.keys().take(12).cloned())
							.collect::<std::collections::HashSet<_>>()
							.into_iter()
							.collect::<Vec<_>>();

						let snapshot = archaeologist
							.as_ref()
							.and_then(|arch| arch.project_insights(&tracked_git_files, 75).ok());
						let json = snapshot.as_ref().and_then(|s| serde_json::to_string_pretty(s).ok());
						let (git_json, git_snapshot) = (json, snapshot);

						let snapshot = ProjectCodeDna::summarize(
							&feature_snapshots,
							&a.dependency_graph,
							&recent_change_files,
							&dna_rules,
						);
						let json = serde_json::to_string_pretty(&snapshot).ok();
						let (dna_json, dna_snapshot) = (json, Some(snapshot));

						let file_map_json = serde_json::to_string_pretty(&build_file_map_snapshot(&feature_snapshots, &a.dependency_graph))
							.unwrap_or_else(|_| "{}".to_string());
						let known_issues_json = serde_json::to_string_pretty(&build_known_issues_snapshot(&recent_reports, &recent_deleted_files))
							.unwrap_or_else(|_| "[]".to_string());
						(
							Some(graph_json),
							Some(changes_json),
							dna_json,
							dna_snapshot,
							git_json,
							git_snapshot,
							Some(file_map_json),
							Some(known_issues_json),
						)
					};

					if let Some(dna_snapshot) = dna_snapshot {
						{
							let mut shared_dna = code_dna_for_events.lock().await;
							*shared_dna = dna_snapshot;
						}
					}
					if let Some(git_snapshot) = git_snapshot {
						{
							let mut shared_git = git_insights_for_events.lock().await;
							*shared_git = git_snapshot;
						}
					}

					if let (Some(graph_json), Some(changes_json), Some(dna_json), Some(file_map_json), Some(known_issues_json)) = (graph_json, changes_json, dna_json, file_map_json, known_issues_json) {
						let graph_entry = make_observer_entry(
							&project_id_for_events,
							"observer_graph.json",
							graph_json,
							vec!["observer".to_string(), "graph".to_string()],
							MemorySource::FileWatcher,
							MemoryKind::Context,
						);
						let changes_entry = make_observer_entry(
							&project_id_for_events,
							"observer_changes.json",
							changes_json,
							vec!["observer".to_string(), "changes".to_string()],
							MemorySource::FileWatcher,
							MemoryKind::Context,
						);
						let dna_entry = make_observer_entry(
							&project_id_for_events,
							"observer_dna.json",
							dna_json,
							vec!["observer".to_string(), "dna".to_string(), "architecture".to_string()],
							MemorySource::FileWatcher,
							MemoryKind::Context,
						);
						let intent_entry = intent_entry_json.as_ref().map(|intent_json| {
							make_observer_entry(
								&project_id_for_events,
								"observer_intent.json",
								intent_json.clone(),
								vec!["observer".to_string(), "intent".to_string(), "predictive".to_string()],
								MemorySource::FileWatcher,
								MemoryKind::Context,
							)
						});
						let git_entry = git_json.as_ref().map(|git_json| {
							make_observer_entry(
								&project_id_for_events,
								"observer_git.json",
								git_json.clone(),
								vec!["observer".to_string(), "git".to_string(), "archaeology".to_string()],
								MemorySource::GitArchaeology,
								MemoryKind::Context,
							)
						});
						let file_map_entry = make_observer_entry(
							&project_id_for_events,
							"file_map",
							file_map_json,
							vec!["observer".to_string(), "file_map".to_string(), "generated".to_string()],
							MemorySource::FileWatcher,
							MemoryKind::Context,
						);
						let known_issues_entry = make_observer_entry(
							&project_id_for_events,
							"known_issues",
							known_issues_json,
							vec!["observer".to_string(), "known_issues".to_string(), "generated".to_string()],
							MemorySource::FileWatcher,
							MemoryKind::Warning,
						);

						let _ = storage_for_events
							.upsert_entry(&project_id_for_events, graph_entry)
							.await;
						let _ = storage_for_events
							.upsert_entry(&project_id_for_events, dna_entry)
							.await;
						let _ = storage_for_events
							.upsert_entry(&project_id_for_events, changes_entry)
							.await;
						let _ = storage_for_events
							.upsert_entry(&project_id_for_events, file_map_entry)
							.await;
						let _ = storage_for_events
							.upsert_entry(&project_id_for_events, known_issues_entry)
							.await;
						if let Some(intent_entry) = intent_entry {
							let _ = storage_for_events
								.upsert_entry(&project_id_for_events, intent_entry)
								.await;
						}
						if let Some(git_entry) = git_entry {
							let _ = storage_for_events
								.upsert_entry(&project_id_for_events, git_entry)
								.await;
						}
					}
				}
			}
		}
	});

	loop {
		let (socket, _addr) = listener.accept().await?;
		let app = app.clone();
		tokio::spawn(async move {
			let io = TokioIo::new(socket);
			let hyper_service = TowerToHyperService::new(app);
			if let Err(e) = Builder::new(TokioExecutor::new())
				.serve_connection(io, hyper_service)
				.await
			{
				tracing::error!("Server error on UDS connection: {}", e);
			}
		});
	}
}
