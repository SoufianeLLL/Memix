use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Instant, Duration};
use tokio::sync::RwLock;

pub struct ContextPredictor {
    pub hot_context: DashMap<String, CachedContext>,
	pub recent_activity: RwLock<VecDeque<FileActivity>>,
	pub current_intent: RwLock<Option<IntentSnapshot>>,
}

#[derive(Debug)]
pub struct CachedContext {
    pub memory_ids: Vec<String>,
	pub related_files: Vec<String>,
    pub last_accessed: Instant,
	pub updated_at_ms: i64,
    pub token_weight: usize,
	pub intent_type: String,
	pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileActivity {
	pub path: String,
	pub updated_at_ms: i64,
	pub nodes_changed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSnapshot {
	pub active_file: String,
	pub intent_type: String,
	pub confidence: f32,
	pub related_files: Vec<String>,
	pub preloaded_memory_ids: Vec<String>,
	pub token_weight: usize,
	pub updated_at_ms: i64,
	pub rationale: Vec<String>,
}

impl ContextPredictor {
    pub fn new() -> Self {
        Self {
            hot_context: DashMap::new(),
			recent_activity: RwLock::new(VecDeque::with_capacity(32)),
			current_intent: RwLock::new(None),
        }
    }

    pub fn prune_stale_predictions(&self, ttl_seconds: u64) {
        let now = Instant::now();
        let threshold = Duration::from_secs(ttl_seconds);
        
        self.hot_context.retain(|_, v| {
            now.duration_since(v.last_accessed) < threshold
        });
    }

	pub async fn record_activity(&self, active_file: &str, nodes_changed: usize) {
		let mut activity = self.recent_activity.write().await;
		activity.push_back(FileActivity {
			path: active_file.to_string(),
			updated_at_ms: chrono::Utc::now().timestamp_millis(),
			nodes_changed,
		});
		while activity.len() > 32 {
			activity.pop_front();
		}
	}

	pub async fn preload_context(
		&self,
		active_file: &str,
		predicted_ids: Vec<String>,
		related_files: Vec<String>,
		exact_tokens: usize,
		intent_type: String,
		confidence: f32,
		rationale: Vec<String>,
	) {
		let updated_at_ms = chrono::Utc::now().timestamp_millis();
		let preloaded_memory_ids = predicted_ids.clone();
        self.hot_context.insert(active_file.to_string(), CachedContext {
            memory_ids: predicted_ids,
			related_files: related_files.clone(),
            last_accessed: Instant::now(),
			updated_at_ms,
            token_weight: exact_tokens,
			intent_type: intent_type.clone(),
			confidence,
        });
		let mut current = self.current_intent.write().await;
		*current = Some(IntentSnapshot {
			active_file: active_file.to_string(),
			intent_type,
			confidence,
			related_files,
			preloaded_memory_ids,
			token_weight: exact_tokens,
			updated_at_ms,
			rationale,
		});
	}

	pub async fn get_current_intent(&self) -> Option<IntentSnapshot> {
		self.current_intent.read().await.clone()
	}

	pub async fn get_cached_context(&self, active_file: &str) -> Option<IntentSnapshot> {
		let current = self.get_current_intent().await;
		if let Some(snapshot) = current {
			if snapshot.active_file == active_file {
				return Some(snapshot);
			}
		}

		self.hot_context.get(active_file).map(|cached| IntentSnapshot {
			active_file: active_file.to_string(),
			intent_type: cached.intent_type.clone(),
			confidence: cached.confidence,
			related_files: cached.related_files.clone(),
			preloaded_memory_ids: cached.memory_ids.clone(),
			token_weight: cached.token_weight,
			updated_at_ms: cached.updated_at_ms,
			rationale: vec!["cache-hit".to_string()],
		})
    }
}
