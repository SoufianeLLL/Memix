use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Session-scoped counters: reset to zero when the daemon starts.
/// All fields are AtomicU64 so they can be updated from any async task
/// without taking a lock — critical for the hot path of context compilation.
#[derive(Default)]
pub struct SessionCounters {
    // Context compiler calls
    pub context_compilations: AtomicU64,
    pub context_tokens_compiled: AtomicU64,

    // AI model consumption (set when orchestrator or learning layer records a call)
    pub ai_calls: AtomicU64,
    pub ai_tokens_consumed: AtomicU64,
    pub ai_tokens_last: AtomicU64,
    pub ai_tokens_max: AtomicU64,
    pub ai_tokens_min: AtomicU64,   // initialized to u64::MAX, set to 0 before display

    // Tokens Memix saved by compiling instead of dumping raw files
    pub estimated_tokens_saved: AtomicU64,

    // OXC + skeleton analysis work
    pub files_skeleton_indexed: AtomicU64,
    pub files_oxc_analyzed: AtomicU64,
    pub embedding_cache_hits: AtomicU64,
    pub embedding_cache_misses: AtomicU64,
}

impl SessionCounters {
    pub fn record_context_compilation(&self, tokens: u64, naive_estimate: u64) {
        self.context_compilations.fetch_add(1, Ordering::Relaxed);
        self.context_tokens_compiled.fetch_add(tokens, Ordering::Relaxed);
        // The saving is the difference between what the developer would have sent
        // (the naive full-file estimate) and what Memix actually compiled.
        let saved = naive_estimate.saturating_sub(tokens);
        self.estimated_tokens_saved.fetch_add(saved, Ordering::Relaxed);
    }

    pub fn record_ai_call(&self, tokens: u64) {
        self.ai_calls.fetch_add(1, Ordering::Relaxed);
        self.ai_tokens_consumed.fetch_add(tokens, Ordering::Relaxed);
        self.ai_tokens_last.store(tokens, Ordering::Relaxed);

        // Update max
        let mut current = self.ai_tokens_max.load(Ordering::Relaxed);
        while tokens > current {
            match self.ai_tokens_max.compare_exchange_weak(current, tokens, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => current = x,
            }
        }

        // Update min (u64::MAX means "not yet set")
        let mut current = self.ai_tokens_min.load(Ordering::Relaxed);
        loop {
            if current != u64::MAX && tokens >= current {
                break;
            }
            match self.ai_tokens_min.compare_exchange_weak(current, tokens, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => current = x,
            }
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let min_raw = self.ai_tokens_min.load(Ordering::Relaxed);
        let count = self.ai_calls.load(Ordering::Relaxed);
        SessionSnapshot {
            context_compilations: self.context_compilations.load(Ordering::Relaxed),
            context_tokens_compiled: self.context_tokens_compiled.load(Ordering::Relaxed),
            ai_calls: count,
            ai_tokens_consumed: self.ai_tokens_consumed.load(Ordering::Relaxed),
            ai_tokens_last: self.ai_tokens_last.load(Ordering::Relaxed),
            ai_tokens_max: self.ai_tokens_max.load(Ordering::Relaxed),
            ai_tokens_min: if min_raw == u64::MAX { 0 } else { min_raw },
            ai_tokens_avg: if count == 0 { 0 } else {
                self.ai_tokens_consumed.load(Ordering::Relaxed) / count
            },
            estimated_tokens_saved: self.estimated_tokens_saved.load(Ordering::Relaxed),
            files_skeleton_indexed: self.files_skeleton_indexed.load(Ordering::Relaxed),
            files_oxc_analyzed: self.files_oxc_analyzed.load(Ordering::Relaxed),
            embedding_cache_hits: self.embedding_cache_hits.load(Ordering::Relaxed),
            embedding_cache_misses: self.embedding_cache_misses.load(Ordering::Relaxed),
        }
    }
}

impl SessionCounters {
    pub fn new() -> Self {
        let s = Self::default();
        // Initialize min to sentinel so the first real record wins
        s.ai_tokens_min.store(u64::MAX, Ordering::Relaxed);
        s
    }
}

/// Lifetime totals persisted to disk across daemon restarts.
/// These are additive — each new session's totals are added to the existing record.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LifetimeTotals {
    pub project_id: String,
    pub total_ai_tokens_consumed: u64,
    pub total_ai_calls: u64,
    pub total_context_compilations: u64,
    pub total_context_tokens_compiled: u64,
    pub total_estimated_tokens_saved: u64,
    pub total_files_indexed: u64,
    pub sessions_recorded: u64,
    pub last_updated: String,
}

impl LifetimeTotals {
    pub fn absorb_session(&mut self, session: &SessionSnapshot, project_id: &str) {
        self.project_id = project_id.to_string();
        self.total_ai_tokens_consumed += session.ai_tokens_consumed;
        self.total_ai_calls += session.ai_calls;
        self.total_context_compilations += session.context_compilations;
        self.total_context_tokens_compiled += session.context_tokens_compiled;
        self.total_estimated_tokens_saved += session.estimated_tokens_saved;
        self.total_files_indexed += session.files_skeleton_indexed;
        self.last_updated = chrono::Utc::now().to_rfc3339();
    }
}

/// The combined tracker holds both session counters and a handle to lifetime persistence.
pub struct TokenTracker {
    pub session: Arc<SessionCounters>,
    lifetime_path: PathBuf,
    lifetime: RwLock<LifetimeTotals>,
    session_recorded: std::sync::atomic::AtomicBool,
}

impl TokenTracker {
    /// Load the tracker for a project. Reads existing lifetime totals from disk
    /// (or starts fresh if none exist). Session counters always start at zero.
    pub async fn load(project_id: &str, data_dir: &std::path::Path) -> Self {
        let lifetime_path = data_dir.join("token_lifetime.json");
        let lifetime = if let Ok(bytes) = tokio::fs::read(&lifetime_path).await {
            serde_json::from_slice::<LifetimeTotals>(&bytes).unwrap_or_default()
        } else {
            LifetimeTotals {
                project_id: project_id.to_string(),
                ..Default::default()
            }
        };

        Self {
            session: Arc::new(SessionCounters::new()),
            lifetime_path,
            lifetime: RwLock::new(lifetime),
            // AtomicBool starts false — the first flush will set it true
            // and count exactly one session, not one per flush cycle
            session_recorded: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Loads lifetime totals from disk into an existing tracker instance.
    /// Called after the daemon is already running to restore historical stats
    /// without blocking startup. Session counters are untouched.
    pub async fn load_lifetime_into(
        tracker: &Arc<TokenTracker>,
        _project_id: &str,
        data_dir: &std::path::Path,
    ) -> anyhow::Result<()> {
        let lifetime_path = data_dir.join("token_lifetime.json");
        if let Ok(bytes) = tokio::fs::read(&lifetime_path).await {
            if let Ok(loaded) = serde_json::from_slice::<LifetimeTotals>(&bytes) {
                let mut lifetime = tracker.lifetime.write().await;
                *lifetime = loaded;
                tracing::debug!("TokenTracker lifetime totals restored from disk");
            }
        }
        Ok(())
    }

    /// Synchronous fallback constructor used when the async load path times out.
    /// Session counters always start at zero. Lifetime totals start empty.
    /// The path is preserved so the next successful flush writes to the correct file.
    pub fn default_empty(project_id: &str, data_dir: &std::path::Path) -> Self {
        Self {
            session: Arc::new(SessionCounters::new()),
            lifetime_path: data_dir.join("token_lifetime.json"),
            lifetime: RwLock::new(LifetimeTotals {
                project_id: project_id.to_string(),
                ..Default::default()
            }),
            session_recorded: std::sync::atomic::AtomicBool::new(false),
        }
    }
    
    /// Create with an explicit lifetime path (for per-workspace trackers)
    pub fn default_empty_with_path(project_id: &str, lifetime_path: std::path::PathBuf) -> Self {
        Self {
            session: Arc::new(SessionCounters::new()),
            lifetime_path,
            lifetime: RwLock::new(LifetimeTotals {
                project_id: project_id.to_string(),
                ..Default::default()
            }),
            session_recorded: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Call this when the daemon is shutting down gracefully, or on a periodic
    /// flush timer (every 5 minutes is a good interval).
    pub async fn flush_session_to_lifetime(&self, project_id: &str) -> anyhow::Result<()> {
        tracing::info!("!!! session_recorded: {}", self.session_recorded.load(Ordering::Relaxed));
        let snapshot = self.session.snapshot();
        if snapshot.ai_calls == 0 && snapshot.context_compilations == 0 {
            return Ok(()); // Nothing to flush
        }

        let mut lifetime = self.lifetime.write().await;
        lifetime.absorb_session(&snapshot, project_id);

        // Only count a new session once per daemon process lifetime,
        // not once per 5-minute flush cycle
        if !self.session_recorded.swap(true, std::sync::atomic::Ordering::Relaxed) {
            lifetime.sessions_recorded += 1;
        }

        if let Some(parent) = self.lifetime_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(
            &self.lifetime_path,
            serde_json::to_vec_pretty(&*lifetime)?,
        ).await?;

        Ok(())
    }

    pub async fn get_lifetime(&self) -> LifetimeTotals {
        self.lifetime.read().await.clone()
    }

    pub fn get_session(&self) -> SessionSnapshot {
        self.session.snapshot()
    }
}

/// The serializable snapshot that gets returned by the API endpoint.
/// Both session and lifetime data travel together so the panel shows
/// everything in one call.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionSnapshot {
    pub context_compilations: u64,
    pub context_tokens_compiled: u64,
    pub ai_calls: u64,
    pub ai_tokens_consumed: u64,
    pub ai_tokens_last: u64,
    pub ai_tokens_max: u64,
    pub ai_tokens_min: u64,
    pub ai_tokens_avg: u64,
    pub estimated_tokens_saved: u64,
    pub files_skeleton_indexed: u64,
    pub files_oxc_analyzed: u64,
    pub embedding_cache_hits: u64,
    pub embedding_cache_misses: u64,
}

/// Combined response that the panel receives from GET /api/v1/tokens/stats
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenStatsResponse {
    pub session: SessionSnapshot,
    pub lifetime: LifetimeTotals,
    pub cache_efficiency_pct: f64,   // hits / (hits + misses) * 100
    pub compression_ratio: f64,      // naive_estimate / compiled — how much we compressed
}
