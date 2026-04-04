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
	manager: tokio::sync::RwLock<Option<redis::aio::ConnectionManager>>,
	data_dir: PathBuf,
	embedding_cache: Cache<u64, Vec<f32>>,

	/// In-memory cache for get_entries results. Avoids redundant Redis full-hash
    /// reads when the panel refreshes rapidly — entries are valid for 20 seconds.
    /// Invalidated immediately when any upsert or delete touches the same project.
	entry_cache: tokio::sync::RwLock<std::collections::HashMap<
        String,
        (std::time::Instant, Vec<crate::brain::schema::MemoryEntry>),
    >>,
	skeleton_cache: tokio::sync::RwLock<std::collections::HashMap<
        String,
        (std::time::Instant, Vec<crate::brain::schema::MemoryEntry>),
    >>,
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
			manager: tokio::sync::RwLock::new(None),
			data_dir,
			embedding_cache: Cache::builder().max_capacity(10_000).build(),
			// Empty cache — populated lazily on first get_entries call per project.
			entry_cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
			skeleton_cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
		})
	}

	async fn get_conn(&self) -> Result<redis::aio::ConnectionManager> {
		{
			let read = self.manager.read().await;
			if let Some(m) = &*read {
				return Ok(m.clone());
			}
		}
		let mut write = self.manager.write().await;
		if let Some(m) = &*write {
			return Ok(m.clone());
		}
		let m = redis::aio::ConnectionManager::new(self._client.clone()).await?;
		*write = Some(m.clone());
		Ok(m)
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
            external_initializers: vec![],
            output_key: None,
            pooling: None,
            quantization: Default::default(),
		}
	}

	fn embedding_model() -> anyhow::Result<&'static parking_lot::Mutex<TextEmbedding>> {
		static MODEL: OnceCell<parking_lot::Mutex<TextEmbedding>> = OnceCell::new();
		if let Some(model) = MODEL.get() {
			return Ok(model);
		}

		let model = TextEmbedding::try_new_from_user_defined(
			Self::bundled_embedding_model(),
			InitOptionsUserDefined::default(),
		)
		.map_err(|e| anyhow::anyhow!("fastembed init failed: {}", e))?;

		let _ = MODEL.set(parking_lot::Mutex::new(model));
		MODEL
			.get()
			.ok_or_else(|| anyhow::anyhow!("fastembed model initialization did not persist"))
	}

	pub async fn embed_text_static(text: &str) -> Vec<f32> {
		match Self::embed_text_real(text.to_string()).await {
			Ok(v) if v.len() == 384 => v,
			_ => vec![0.0; 384],
		}
	}

	async fn embed_text_real(text: String) -> anyhow::Result<Vec<f32>> {
		let handle = tokio::task::spawn_blocking(move || {
			let model_mutex = Self::embedding_model()?;
            let mut model = model_mutex.lock();
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

// ─── Skeleton Index Storage ──────────────────────────────────────────────────
// Skeleton entries (FSI + FuSI) live in a separate Redis hash:
//   key = "{project_id}_skeletons"
//   cap = 2,000 entries (independent of the 1,000-entry brain cap)
//
// This prevents skeleton entries from competing with brain entries for budget.

impl RedisStorage {
	fn skeleton_hash_key(project_id: &str) -> String {
		format!("{}_skeletons", project_id)
	}

	fn max_skeleton_entries() -> u64 {
		std::env::var("MEMIX_MAX_SKELETON_ENTRIES").ok().and_then(|s| s.parse().ok()).unwrap_or(2_000)
	}

	/// Upsert a skeleton entry (FSI or FuSI) into the isolated skeleton hash.
	/// If at capacity and this is a new entry, evicts the oldest entry by updated_at.
	pub async fn upsert_skeleton_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
		{
			let mut cache = self.skeleton_cache.write().await;
			cache.remove(project_id);
		}

		let mut conn = self.get_conn().await?;
		let hash_key = Self::skeleton_hash_key(project_id);
		let already_exists: bool = conn.hexists(&hash_key, &entry.id).await.unwrap_or(false);

		if !already_exists {
			let count: u64 = conn.hlen(&hash_key).await.unwrap_or(0);
			if count >= Self::max_skeleton_entries() {
				// LRU eviction: find and delete the entry with the oldest updated_at
				tracing::warn!(
					"Skeleton index at capacity ({}) for project {}, evicting oldest entry",
					Self::max_skeleton_entries(),
					project_id
				);
				if let Ok(values) = conn.hgetall::<&str, HashMap<String, String>>(&hash_key).await {
					let oldest = values.iter()
						.filter_map(|(id, json)| {
							serde_json::from_str::<MemoryEntry>(json)
								.ok()
								.map(|e| (id.clone(), e.updated_at))
						})
						.min_by_key(|(_, ts)| *ts);
					if let Some((old_id, _)) = oldest {
						let _: Result<(), _> = conn.hdel(&hash_key, &old_id).await.map_err(|e| {
							tracing::error!("Failed to evict skeleton entry {}: {}", old_id, e);
							e
						});
					}
				}
			}
		}

		let json_str = serde_json::to_string(&entry)?;
		let _: () = conn.hset(&hash_key, &entry.id, json_str).await?;
		Ok(())
	}

	/// Get all skeleton entries (FSI + FuSI) for a project.
	pub async fn get_skeleton_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
		// Check cache first (TTL 60 seconds since files change less rapidly than AI chat)
        {
            let cache = self.skeleton_cache.read().await;
            if let Some((fetched_at, entries)) = cache.get(project_id) {
                if fetched_at.elapsed().as_secs() < 60 {
                    return Ok(entries.clone());
                }
            }
        }

		// Cache miss: hit Redis
		let mut conn = self.get_conn().await?;
		let hash_key = Self::skeleton_hash_key(project_id);
		let values: Vec<String> = conn.hvals(&hash_key).await.unwrap_or_default();
		let entries: Vec<MemoryEntry> = values
			.iter()
			.filter_map(|v| serde_json::from_str::<MemoryEntry>(v).ok())
			.collect();

        // Update cache
        {
            let mut cache = self.skeleton_cache.write().await;
            cache.insert(
                project_id.to_string(),
                (std::time::Instant::now(), entries.clone()),
            );
        }

		Ok(entries)
	}

	/// Delete a specific skeleton entry by ID.
	pub async fn delete_skeleton_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
		{
			let mut cache = self.skeleton_cache.write().await;
			cache.remove(project_id);
		}

		let mut conn = self.get_conn().await?;
		let hash_key = Self::skeleton_hash_key(project_id);
		let _: () = conn.hdel(&hash_key, entry_id).await?;
		Ok(())
	}

	/// Purge all skeleton entries for a project (clear the entire skeleton index).
	pub async fn purge_skeleton_entries(&self, project_id: &str) -> Result<usize> {
		{
			let mut cache = self.skeleton_cache.write().await;
			cache.remove(project_id);
		}

		let mut conn = self.get_conn().await?;
		let hash_key = Self::skeleton_hash_key(project_id);
		
		// Get count before deletion
		let entries: Vec<MemoryEntry> = self.get_skeleton_entries(project_id).await?;
		let count = entries.len();
		
		// Delete the entire hash
		let _: () = conn.del(&hash_key).await?;
		
		// Also clear the embedding store for this project
		let emb_key = format!("embeddings:{}", project_id);
		let _: () = conn.del(&emb_key).await?;
		
		Ok(count)
	}

	/// Get the current project ID (first available from brain keys)
	pub async fn get_project_id(&self) -> Option<String> {
		// Try to get identity.json which contains project info
		if let Ok(entries) = self.get_entries("default").await {
			for entry in entries {
				if entry.id == "identity.json" {
					if let Ok(json) = serde_json::from_str::<serde_json::Value>(&entry.content) {
						if let Some(name) = json.get("name").and_then(|n| n.as_str()) {
							return Some(name.to_string());
						}
					}
				}
			}
		}
		Some("default".to_string())
	}

	/// Returns (fsi_count, fusi_count, total, size_bytes) for the skeleton index.
	pub async fn skeleton_stats(&self, project_id: &str) -> Result<(usize, usize, usize, usize)> {
		let entries = self.get_skeleton_entries(project_id).await?;
		let fsi = entries.iter().filter(|e| e.tags.contains(&"fsi".to_string())).count();
		let fusi = entries.iter().filter(|e| e.tags.contains(&"fusi".to_string())).count();
		let mut size_bytes = 0;
		for e in &entries {
			if let Ok(json) = serde_json::to_string(e) {
				size_bytes += json.len();
			}
		}
		Ok((fsi, fusi, entries.len(), size_bytes))
	}

	/// Performs the actual Redis HVALS read for a project. Called only on cache miss.
	/// Separated from get_entries so the cache wrapper stays clean and readable.
	async fn fetch_entries_from_redis(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
		match self.get_conn().await {
			Ok(mut conn) => {
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
					}
					Err(e) => {
						tracing::error!("❌ Redis HVALS failed: {}", e);
						Err(anyhow::anyhow!("Redis command failed: {}", e))
					}
				}
			}
			Err(e) => {
				tracing::error!("❌ Failed to get Redis connection: {}", e);
				if e.to_string().contains("refused") {
					tracing::error!("🔴 REDIS CONNECTION REFUSED — verify MEMIX_REDIS_URL and host reachability");
				}
				Err(anyhow::anyhow!("Redis connection failed: {}", e))
			}
		}
	}
}

#[async_trait]
impl StorageBackend for RedisStorage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
	async fn embed_text(&self, text: &str) -> Vec<f32> {
        // Calls your internal `embed_text` which uses `self.embedding_cache`
        RedisStorage::embed_text(self, text).await
    }
	
	async fn get_entries(&self, project_id: &str) -> Result<Vec<MemoryEntry>> {
		tracing::debug!("📦 get_entries called for project: {}", project_id);

		// Cache TTL: 20 seconds. This collapses rapid panel refreshes — which each
		// independently call get_entries — into at most one Redis round-trip per window.
		// Short enough that stale data is never a practical problem in a dev session.
		const CACHE_TTL_SECS: u64 = 20;

		// Check cache under a read lock first (cheap, no Redis round-trip)
		{
			let cache = self.entry_cache.read().await;
			if let Some((fetched_at, entries)) = cache.get(project_id) {
				if fetched_at.elapsed().as_secs() < CACHE_TTL_SECS {
					tracing::debug!("📦 get_entries cache hit for project: {}", project_id);
					return Ok(entries.clone());
				}
			}
		}

		// Cache miss or TTL expired — fetch from Redis
		tracing::debug!("📦 get_entries cache miss — fetching from Redis for: {}", project_id);
		let entries = self.fetch_entries_from_redis(project_id).await?;

		// Populate cache under a write lock
		{
			let mut cache = self.entry_cache.write().await;
			cache.insert(
				project_id.to_string(),
				(std::time::Instant::now(), entries.clone()),
			);
		}

		Ok(entries)
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
		// Invalidate the entry cache immediately so the next read reflects this write.
		// Without this, a pending.json update would be invisible for up to 20 seconds.
		{
			let mut cache = self.entry_cache.write().await;
			if let Some((_, entries)) = cache.get_mut(project_id) {
				if let Some(pos) = entries.iter().position(|e| e.id == entry.id) {
					entries[pos] = entry.clone(); // Update existing
				} else {
					entries.push(entry.clone()); // Add new
				}
			}
		}

		let mut conn = self.get_conn().await?;
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
		// Invalidate cache so the deletion is visible immediately on next read.
		{
			let mut cache = self.entry_cache.write().await;
			cache.remove(project_id);
		}

		let mut conn = self.get_conn().await?;
		let _: () = conn.hdel(project_id, entry_id).await?;
		self.delete_entry_json_at(entry_id).await;
		Ok(())
	}

	async fn purge_project(&self, project_id: &str) -> Result<()> {
		// Invalidate the entry cache on delete for the same reason as upsert.
		{
			let mut cache = self.entry_cache.write().await;
			cache.remove(project_id);
		}
		
		let mut conn = self.get_conn().await?;
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
		let mut conn = self.get_conn().await?;

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
		let mut conn = self.get_conn().await?;
		let mut projects = std::collections::HashSet::new();
		
		// Use SCAN instead of KEYS to avoid blocking Redis (O(1) per iteration vs O(N) blocking)
		// This is production-safe for large Redis instances
		let mut cursor: u64 = 0;
		loop {
			let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
				.arg(cursor)
				.arg("TYPE")
				.arg("HASH")
				.query_async(&mut conn)
				.await?;
			
			for key in keys {
				// Skip skeleton hashes - they're not projects
				if !key.ends_with("_skeletons") {
					projects.insert(key);
				}
			}
			
			cursor = new_cursor;
			if cursor == 0 {
				break;
			}
		}
		
		let mut projects: Vec<String> = projects.into_iter().collect();
		projects.sort();
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

	// ─── Skeleton Index overrides ────────────────────────────────────
	async fn upsert_skeleton_entry(&self, project_id: &str, entry: MemoryEntry) -> Result<()> {
		RedisStorage::upsert_skeleton_entry(self, project_id, entry).await
	}

	async fn get_entry(&self, project_id: &str, entry_id: &str) -> Result<MemoryEntry> {
		// Fetch all entries and find the specific one
		let entries = RedisStorage::get_entries(self, project_id).await?;
		entries.into_iter()
			.find(|e| e.id == entry_id)
			.ok_or_else(|| anyhow::anyhow!("Entry not found: {}", entry_id))
	}

	async fn delete_skeleton_entry(&self, project_id: &str, entry_id: &str) -> Result<()> {
		RedisStorage::delete_skeleton_entry(self, project_id, entry_id).await
	}

	async fn purge_skeleton_entries(&self, project_id: &str) -> Result<usize> {
		RedisStorage::purge_skeleton_entries(self, project_id).await
	}

	async fn skeleton_stats(&self, project_id: &str) -> Result<(usize, usize, usize, usize)> {
		RedisStorage::skeleton_stats(self, project_id).await
	}

	async fn get_project_id(&self) -> Option<String> {
		RedisStorage::get_project_id(self).await
	}
 }
