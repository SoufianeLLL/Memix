use crate::config::AppConfig;
use crate::brain::schema::MemoryEntry;
use crate::storage::{RedisStats, StorageBackend, TeamSyncReport};
use crate::sync::team::TeamManager;
use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use redis::AsyncCommands;
use std::path::{Path, PathBuf};
use tokio::fs;
use mini_moka::sync::Cache;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};

static MODEL_CONFIG_JSON: &[u8] = include_bytes!("../../models/all-MiniLM-L6-v2/config.json");
static MODEL_TOKENIZER_JSON: &[u8] = include_bytes!("../../models/all-MiniLM-L6-v2/tokenizer.json");
static MODEL_TOKENIZER_CONFIG_JSON: &[u8] = include_bytes!("../../models/all-MiniLM-L6-v2/tokenizer_config.json");
static MODEL_SPECIAL_TOKENS_JSON: &[u8] = include_bytes!("../../models/all-MiniLM-L6-v2/special_tokens_map.json");
static MODEL_ONNX: &[u8] = include_bytes!("../../models/all-MiniLM-L6-v2/onnx/model.onnx");

pub struct RedisStorage {
	_client: redis::Client,
	data_dir: PathBuf,
	embedding_cache: Cache<u64, Vec<f32>>,
}

impl RedisStorage {
	const MAX_ENTRIES_PER_PROJECT: u64 = 1000;

	async fn atomic_write(path: &Path, content: &str) -> Result<()> {
		let tmp = path.with_extension("json.tmp");
		fs::write(&tmp, content)
			.await
			.with_context(|| format!("Failed writing temp file {:?}", tmp))?;
		fs::rename(&tmp, path)
			.await
			.with_context(|| format!("Failed renaming temp file {:?} to {:?}", tmp, path))?;
		Ok(())
	}

	fn safe_entry_filename(entry_id: &str) -> Option<String> {
		let file_name = Path::new(entry_id).file_name()?.to_string_lossy().to_string();
		if file_name.is_empty() {
			return None;
		}
		if file_name.ends_with(".json") {
			Some(file_name)
		} else {
			Some(format!("{}.json", file_name))
		}
	}

	async fn mirror_entry_to_json_at(data_dir: PathBuf, entry: MemoryEntry) {
		let Some(file_name) = Self::safe_entry_filename(&entry.id) else {
			tracing::warn!("Skipping JSON mirror for unsafe entry id: {}", entry.id);
			return;
		};

		let dir = data_dir.join("brain");
		if let Err(e) = fs::create_dir_all(&dir).await {
			tracing::error!("Failed to create brain dir {:?}: {}", dir, e);
			return;
		}

		let path = dir.join(file_name);
		match serde_json::to_string_pretty(&entry) {
			Ok(json) => {
				if let Err(e) = Self::atomic_write(&path, &json).await {
					tracing::error!("Failed to write brain mirror {:?}: {}", path, e);
				}
			}
			Err(e) => {
				tracing::error!("Failed to serialize entry {} for JSON mirror: {}", entry.id, e);
			}
		}
	}

	async fn delete_entry_json_at(&self, entry_id: &str) {
		let Some(file_name) = Self::safe_entry_filename(entry_id) else {
			return;
		};
		let path = self.data_dir.join("brain").join(file_name);
		match fs::remove_file(&path).await {
			Ok(_) => {}
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
			Err(e) => tracing::error!("Failed to delete brain mirror {:?}: {}", path, e),
		}
	}

	async fn purge_brain_dir(&self) {
		let dir = self.data_dir.join("brain");
		let mut rd = match fs::read_dir(&dir).await {
			Ok(rd) => rd,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
			Err(e) => {
				tracing::error!("Failed to read brain dir {:?}: {}", dir, e);
				return;
			}
		};

		while let Ok(Some(entry)) = rd.next_entry().await {
			let path = entry.path();
			if let Err(e) = fs::remove_file(&path).await {
				if e.kind() != std::io::ErrorKind::IsADirectory && e.kind() != std::io::ErrorKind::NotFound {
					tracing::error!("Failed to remove brain file {:?}: {}", path, e);
				}
			}
		}
	}

	pub async fn new(config: &AppConfig) -> Result<Self> {
		tracing::info!(
			"Initializing RedisStorage (backend={}, port={}, data_dir={})",
			config.backend.clone().unwrap_or_else(|| "redis".to_string()),
			config.port.unwrap_or(3456),
			config.data_dir.clone().unwrap_or_else(|| ".memix".to_string())
		);

		let dir_str = config.data_dir.clone().unwrap_or_else(|| ".memix".to_string());
		let data_dir = PathBuf::from(&dir_str);
		
		let url = config.redis_url.clone().unwrap_or_else(|| "redis://127.0.0.1/".to_string());
		let redacted = {
			let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
			let (scheme, rest) = url.split_at(scheme_end);
			if let Some(at) = rest.find('@') {
				format!("{}***@{}", scheme, &rest[(at + 1)..])
			} else {
				format!("{}{}", scheme, rest)
			}
		};
		tracing::info!("Connecting to Redis at: {}", redacted);
		let _client = redis::Client::open(url)?;
		match tokio::time::timeout(std::time::Duration::from_secs(2), _client.get_multiplexed_async_connection()).await {
			Ok(Ok(mut conn)) => {
				let pong: redis::RedisResult<String> = redis::cmd("PING").query_async(&mut conn).await;
				match pong {
					Ok(_) => tracing::info!("✓ Redis ping ok ({})", redacted),
					Err(e) => tracing::warn!("Redis ping failed ({}): {}", redacted, e),
				}
			}
			Ok(Err(e)) => tracing::warn!("Redis connection failed ({}): {}", redacted, e),
			Err(_) => tracing::warn!("Redis connection timed out ({})", redacted),
		}
		
		if !data_dir.exists() {
         	tracing::info!("Creating data directory: {:?}", data_dir);
			fs::create_dir_all(&data_dir).await.context("Failed to create data directory")?;
		}

		Ok(Self {
			_client,
			data_dir,
			embedding_cache: Cache::builder().max_capacity(10_000).build(),
		})
	}

	fn generate_dummy_embedding(&self, text: &str) -> Vec<f32> {
		tracing::warn!("WARNING: Generating dummy embedding. Real embeddings are disabled/failed! Context searches will return random unrelated nodes.");
		let hash = text.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
		(0..384).map(|i| {
			let h = hash.wrapping_add(i as u64);
			((h % 1000) as f32 / 1000.0) * 2.0 - 1.0
		}).collect()
	}

	fn content_hash(text: &str) -> u64 {
		let mut hasher = DefaultHasher::new();
		text.hash(&mut hasher);
		hasher.finish()
	}

	fn bundled_embedding_model() -> UserDefinedEmbeddingModel {
		UserDefinedEmbeddingModel {
			onnx_file: MODEL_ONNX.to_vec(),
			tokenizer_files: TokenizerFiles {
				tokenizer_file: MODEL_TOKENIZER_JSON.to_vec(),
				config_file: MODEL_CONFIG_JSON.to_vec(),
				special_tokens_map_file: MODEL_SPECIAL_TOKENS_JSON.to_vec(),
				tokenizer_config_file: MODEL_TOKENIZER_CONFIG_JSON.to_vec(),
			},
		}
	}

	fn embedding_model() -> anyhow::Result<&'static TextEmbedding> {
		static MODEL: OnceCell<TextEmbedding> = OnceCell::new();
		if let Some(model) = MODEL.get() {
			return Ok(model);
		}

		let model = TextEmbedding::try_new_from_user_defined(
			Self::bundled_embedding_model(),
			InitOptionsUserDefined::default(),
		)
		.map_err(|e| anyhow::anyhow!("fastembed init failed: {}", e))?;

		let _ = MODEL.set(model);
		MODEL
			.get()
			.ok_or_else(|| anyhow::anyhow!("fastembed model initialization did not persist"))
	}

	async fn embed_text_real(text: String) -> anyhow::Result<Vec<f32>> {
		let handle = tokio::task::spawn_blocking(move || {
			let model = Self::embedding_model()?;
			let out = model
				.embed(vec![text], None)
				.map_err(|e| anyhow::anyhow!("fastembed embed failed: {}", e))?;
			Ok::<_, anyhow::Error>(out.into_iter().next().unwrap_or_default())
		});
		handle
			.await
			.map_err(|e| anyhow::anyhow!("embedding task join failed: {}", e))?
	}

	async fn embed_text(&self, text: &str) -> Vec<f32> {
		let key = Self::content_hash(text);
		if let Some(hit) = self.embedding_cache.get(&key) {
			return hit;
		}

		match Self::embed_text_real(text.to_string()).await {
			Ok(v) if v.len() == 384 => {
				self.embedding_cache.insert(key, v.clone());
				return v;
			}
			Ok(v) => {
				tracing::warn!("Real embedding returned dim={} (expected 384); falling back", v.len());
			}
			Err(e) => {
				tracing::error!("Real embedding failed; falling back to dummy: {}", e);
			}
		}
		let fallback = self.generate_dummy_embedding(text);
		self.embedding_cache.insert(key, fallback.clone());
		fallback
	}

	fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
		let mut dot = 0.0_f32;
		let mut na = 0.0_f32;
		let mut nb = 0.0_f32;
		for (x, y) in a.iter().zip(b.iter()) {
			dot += x * y;
			na += x * x;
			nb += y * y;
		}
		if na <= f32::EPSILON || nb <= f32::EPSILON {
			0.0
		} else {
			dot / (na.sqrt() * nb.sqrt())
		}
	}

	fn keyword_score(query: &str, content: &str) -> f32 {
		let q = query.to_lowercase();
		let c = content.to_lowercase();
		let terms: Vec<&str> = q
			.split_whitespace()
			.map(str::trim)
			.filter(|t| t.len() >= 3)
			.collect();
		if terms.is_empty() {
			return 0.0;
		}
		let matched = terms.iter().filter(|t| c.contains(**t)).count();
		matched as f32 / terms.len() as f32
	}

	fn hybrid_similarity(query_embedding: &[f32], entry_embedding: &[f32], query: &str, content: &str) -> f32 {
		let cosine = Self::cosine_similarity(query_embedding, entry_embedding);
		let cosine_01 = (cosine + 1.0) * 0.5;
		let keyword = Self::keyword_score(query, content);
		(0.8 * cosine_01) + (0.2 * keyword)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::brain::schema::{MemoryKind, MemorySource};
	use crate::config::AppConfig;

	fn test_entry(project_id: &str, id: &str, content: &str) -> MemoryEntry {
		let now = chrono::Utc::now();
		MemoryEntry {
			id: id.to_string(),
			project_id: project_id.to_string(),
			kind: MemoryKind::Context,
			content: content.to_string(),
			tags: vec!["test".to_string()],
			source: MemorySource::UserManual,
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

	#[tokio::test]
	#[ignore]
	async fn integration_mirror_export_import_roundtrip_under_load() -> Result<()> {
		let redis_url = std::env::var("MEMIX_TEST_REDIS_URL")
			.unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
		let tmp = std::env::temp_dir().join(format!("memix_it_{}", uuid::Uuid::new_v4()));
		fs::create_dir_all(&tmp).await?;

		let cfg = AppConfig {
			port: None,
			backend: Some("redis".to_string()),
			redis_url: Some(redis_url),
			data_dir: Some(tmp.to_string_lossy().to_string()),
			workspace_root: None,
			project_id: None,
			team_id: None,
			team_secret: None,
			team_actor_id: None,
			license_public_key: None,
			license_server_url: None,
		};

		let storage = RedisStorage::new(&cfg).await?;
		let project_id = format!("it-project-{}", uuid::Uuid::new_v4());

		storage.purge_project(&project_id).await?;

		for i in 0..150_u32 {
			let id = format!("it_entry_{}", i);
			let content = format!("entry content {} vector payload", i);
			storage
				.upsert_entry(&project_id, test_entry(&project_id, &id, &content))
				.await?;
		}

		let written = storage.export_project_to_json(&project_id).await?;
		assert!(written >= 150);

		storage.purge_project(&project_id).await?;
		let imported = storage.import_project_from_json(&project_id).await?;
		assert!(imported >= 150);

		let entries = storage.get_entries(&project_id).await?;
		assert!(entries.len() >= 150);

		storage.purge_project(&project_id).await?;
		Ok(())
	}
}

#[async_trait]
impl StorageBackend for RedisStorage {
	async fn get_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
		tracing::debug!("📦 get_entries called for project: {}", project_id);

		// Log the Redis URL (without credentials)
	    	tracing::debug!("Attempting to get Redis connection...");

		match self._client.get_multiplexed_async_connection().await {
			Ok(mut conn) => {
				tracing::debug!("Got Redis connection");
				
				tracing::debug!("Executing HGETALL for key: {}", project_id);
				match conn.hvals::<&str, Vec<String>>(project_id).await {
					Ok(values) => {
						tracing::debug!("Got {} values from Redis", values.len());
						let mut entries = Vec::new();
						for val in values {
							if let Ok(entry) = serde_json::from_str::<MemoryEntry>(&val) {
								entries.push(entry);
							}
						}
						Ok(entries)
					},
					Err(e) => {
						tracing::error!("❌ Redis HGETALL failed: {}", e);
						Err(anyhow::anyhow!("Redis command failed: {}", e))
					}
				}
			},
			Err(e) => {
				tracing::error!("❌ Failed to get Redis connection: {}", e);
				
				if e.to_string().contains("refused") {
					tracing::error!("🔴 REDIS CONNECTION REFUSED");
					tracing::error!("Verify daemon is using the expected redis_url (config.toml or MEMIX_REDIS_URL), and that the Redis host/port is reachable from this machine.");
				}
				
				return Err(anyhow::anyhow!("Redis connection failed: {}", e));
			}
		}
	}

	async fn export_project_to_json(&self, project_id: &str) -> Result<u64> {
		let entries = self.get_entries(project_id).await?;
		let data_dir = self.data_dir.clone();
		let mut written: u64 = 0;
		for entry in entries {
			if Self::safe_entry_filename(&entry.id).is_none() {
				continue;
			}
			Self::mirror_entry_to_json_at(data_dir.clone(), entry).await;
			written = written.saturating_add(1);
		}
		Ok(written)
	}

	async fn import_project_from_json(&self, project_id: &str) -> Result<u64> {
		let brain_dir = self.data_dir.join("brain");
		let mut rd = match fs::read_dir(&brain_dir).await {
			Ok(v) => v,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
			Err(e) => return Err(anyhow::anyhow!("Failed to read brain dir {:?}: {}", brain_dir, e)),
		};

		let mut imported: u64 = 0;
		while let Ok(Some(entry)) = rd.next_entry().await {
			let path = entry.path();
			let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue; };
			if !name.ends_with(".json") {
				continue;
			}
			if name == "pending.json" || name == "pending.ack.json" {
				continue;
			}

			let raw = match fs::read_to_string(&path).await {
				Ok(v) => v,
				Err(e) => {
					tracing::warn!("Skipping unreadable brain file {:?}: {}", path, e);
					continue;
				}
			};

			let parsed: MemoryEntry = match serde_json::from_str(&raw) {
				Ok(v) => v,
				Err(e) => {
					tracing::warn!("Skipping invalid brain JSON {:?}: {}", path, e);
					continue;
				}
			};

			if parsed.project_id != project_id {
				tracing::debug!(
					"Skipping brain file {:?}: project_id mismatch ({} != {})",
					path,
					parsed.project_id,
					project_id
				);
				continue;
			}

			self.upsert_entry(project_id, parsed).await?;
			imported = imported.saturating_add(1);
		}

		Ok(imported)
	}

	async fn upsert_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
		let mut conn = self._client.get_multiplexed_async_connection().await?;
		let existing_count: u64 = conn.hlen(project_id).await?;
		let already_exists: bool = conn.hexists(project_id, &entry.id).await?;
		if existing_count >= Self::MAX_ENTRIES_PER_PROJECT && !already_exists {
			return Err(anyhow::anyhow!(
				"Project entry limit reached ({})",
				Self::MAX_ENTRIES_PER_PROJECT
			));
		}
		let json_str = serde_json::to_string(&entry)?;
		
		let _: () = conn.hset(project_id, &entry.id, json_str).await?;

		let mirror_data_dir = self.data_dir.clone();
		let mirror_entry = entry.clone();
		tokio::spawn(async move {
			// Best-effort; do not block the upsert response.
			RedisStorage::mirror_entry_to_json_at(mirror_data_dir, mirror_entry).await;
		});

		Ok(())
	}

	async fn search_entries(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>> {
		self.search_similar(project_id, query).await
	}

	async fn delete_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
		let mut conn = self._client.get_multiplexed_async_connection().await?;
		let _: () = conn.hdel(project_id, entry_id).await?;
		self.delete_entry_json_at(entry_id).await;
		Ok(())
	}

	async fn purge_project(&self, project_id: &str) -> Result<()> {
		let mut conn = self._client.get_multiplexed_async_connection().await?;
		let _: () = conn.del(project_id).await?;
		self.purge_brain_dir().await;
		Ok(())
	}

	async fn search_similar(&self, project_id: &str, query: &str) -> Result<Vec<MemoryEntry>> {
		let all_entries = self.get_entries(project_id).await?;
		if all_entries.is_empty() {
			return Ok(Vec::new());
		}

		let query_embedding = self.embed_text(query).await;

		let mut scored: Vec<(MemoryEntry, f32)> = Vec::with_capacity(all_entries.len());
		for e in all_entries {
			let entry_embedding = self.embed_text(&e.content).await;
			let similarity = Self::hybrid_similarity(&query_embedding, &entry_embedding, query, &e.content);
			scored.push((e, similarity));
		}

		scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
		Ok(scored.into_iter().take(10).map(|(e, _)| e).collect())
	}

	async fn redis_stats(&self) -> Result<RedisStats> {
		let mut conn = self._client.get_multiplexed_async_connection().await?;

		let mut max_bytes: Option<u64> = None;
		let cfg: redis::RedisResult<Vec<String>> = redis::cmd("CONFIG")
			.arg("GET")
			.arg("maxmemory")
			.query_async(&mut conn)
			.await;
		if let Ok(cfg) = cfg {
			if let Some(v) = cfg.get(1).and_then(|v| v.parse::<u64>().ok()) {
				if v > 0 {
					max_bytes = Some(v);
				}
			}
		}

		let info: String = redis::cmd("INFO")
			.arg("memory")
			.query_async(&mut conn)
			.await?;

		let mut used_bytes: u64 = 0;
		for line in info.lines() {
			if let Some(rest) = line.strip_prefix("used_memory:") {
				used_bytes = rest.trim().parse::<u64>().unwrap_or(0);
				continue;
			}
			// Redis INFO memory includes maxmemory on many setups (incl. managed Redis)
			if let Some(rest) = line.strip_prefix("maxmemory:") {
				if max_bytes.is_none() {
					let v = rest.trim().parse::<u64>().unwrap_or(0);
					if v > 0 {
						max_bytes = Some(v);
					}
				}
				continue;
			}
		}

		Ok(RedisStats { used_bytes, max_bytes: None })
	}

	async fn list_projects(&self) -> Result<Vec<String>> {
		let mut conn = self._client.get_multiplexed_async_connection().await?;
		let keys: Vec<String> = conn.keys("*").await?;
		let mut projects = Vec::new();
		for key in keys {
			let key_type: String = redis::cmd("TYPE").arg(&key).query_async(&mut conn).await?;
			if key_type == "hash" {
				projects.push(key);
			}
		}
		projects.sort();
		projects.dedup();
		Ok(projects)
	}

	async fn sync_team_project(
		&self,
		project_id: &str,
		team_id: &str,
		actor_id: &str,
		shared_secret: &str,
	) -> Result<TeamSyncReport> {
		if shared_secret.trim().len() < 16 {
			return Err(anyhow::anyhow!("team_secret must be configured and at least 16 characters"));
		}
		let local_entries = self.get_entries(project_id).await?;
		let local_by_id: HashMap<String, MemoryEntry> = local_entries
			.iter()
			.cloned()
			.map(|entry| (entry.id.clone(), entry))
			.collect();
		let manager = TeamManager::new(self._client.clone(), actor_id.to_string(), shared_secret.to_string());
		let pushed_entries = manager.publish_entries(team_id, project_id, &local_entries).await?;
		let pulled = manager.pull_operations(team_id, project_id).await?;
		let pulled_entries = pulled.entries.len() as u64;
		let mut remote_by_id: HashMap<String, MemoryEntry> = HashMap::new();
		for remote in pulled.entries {
			match remote_by_id.get(&remote.id) {
				Some(existing) if existing.updated_at >= remote.updated_at => {}
				_ => {
					remote_by_id.insert(remote.id.clone(), remote);
				}
			}
		}
		let mut merged_entries = 0u64;
		let mut conflict_entries = pulled.conflict_entries;

		for mut remote in remote_by_id.into_values() {
			remote.project_id = project_id.to_string();
			let should_apply = match local_by_id.get(&remote.id) {
				Some(local) if local.updated_at > remote.updated_at => {
					if local.content != remote.content {
						conflict_entries = conflict_entries.saturating_add(1);
					}
					false
				}
				Some(local) if local.updated_at == remote.updated_at && local.content != remote.content => {
					conflict_entries = conflict_entries.saturating_add(1);
					remote.id >= local.id
				}
				_ => true,
			};
			if should_apply {
				self.upsert_entry(project_id, remote).await?;
				merged_entries = merged_entries.saturating_add(1);
			}
		}

		Ok(TeamSyncReport {
			project_id: project_id.to_string(),
			team_id: team_id.to_string(),
			recovered_from_gap: pulled.recovered_from_gap,
			recovered_entries: pulled.recovered_entries,
			pushed_entries,
			pulled_entries,
			applied_operations: pulled.applied_operations,
			merged_entries,
			conflict_entries,
			actor_id: actor_id.to_string(),
			cursor: pulled.cursor,
			team_namespace: pulled.namespace,
			team_brain: pulled.team_brain,
		})
	}
 }
