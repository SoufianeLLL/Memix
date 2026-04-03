// Per-workspace token tracker management.
// Each workspace has its own TokenTracker with separate lifetime stats.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::tracker::TokenTracker;

/// Manages per-workspace token trackers.
/// Each workspace has its own stats file and session counters.
pub struct TokenTrackerManager {
    trackers: Mutex<HashMap<String, Arc<TokenTracker>>>,
    data_dir: std::path::PathBuf,
}

impl TokenTrackerManager {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self {
            trackers: Mutex::new(HashMap::new()),
            data_dir,
        }
    }
    
    /// Get or create a token tracker for a workspace.
    /// Each workspace stores its lifetime stats in .memix/data/{project_id}_token_lifetime.json
    pub async fn get_or_create(&self, project_id: &str) -> Arc<TokenTracker> {
        let mut trackers = self.trackers.lock().await;
        if let Some(tracker) = trackers.get(project_id) {
            return tracker.clone();
        }
        
        // Each workspace gets its own lifetime file
        let lifetime_path = self.data_dir.join(format!("{}_token_lifetime.json", project_id));
        
        let tracker = Arc::new(TokenTracker::default_empty_with_path(
            project_id,
            lifetime_path,
        ));
        
        trackers.insert(project_id.to_string(), tracker.clone());
        tracker
    }
    
    /// Get a tracker for an existing workspace (returns None if not registered)
    pub async fn get(&self, project_id: &str) -> Option<Arc<TokenTracker>> {
        let trackers = self.trackers.lock().await;
        trackers.get(project_id).cloned()
    }
    
    /// Remove a tracker when a workspace is unregistered
    pub async fn remove(&self, project_id: &str) -> Option<Arc<TokenTracker>> {
        let mut trackers = self.trackers.lock().await;
        trackers.remove(project_id)
    }
    
    /// Flush all trackers to disk
    pub async fn flush_all(&self) {
        let trackers = self.trackers.lock().await;
        for (project_id, tracker) in trackers.iter() {
            let _ = tracker.flush_session_to_lifetime(project_id).await;
        }
    }
    
    /// List all registered project IDs
    pub async fn project_ids(&self) -> Vec<String> {
        let trackers = self.trackers.lock().await;
        trackers.keys().cloned().collect()
    }
}
