// Multi-workspace background indexer manager.
// Spawns and manages BackgroundIndexer instances per registered workspace.
// Prioritizes indexing for the active workspace.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::observer::background_indexer::BackgroundIndexer;
use crate::observer::embedding_store::EmbeddingStore;
use crate::storage::StorageBackend;
use crate::token::tracker::TokenTracker;

/// Manages background indexers across multiple workspaces
pub struct IndexerManager {
    /// Running indexer tasks keyed by project_id
    indexers: HashMap<String, tokio::task::JoinHandle<()>>,
    /// Shared storage backend
    storage: Arc<dyn StorageBackend + Send + Sync>,
    /// Shared embedding store (cloned per indexer)
    embedding_store: EmbeddingStore,
    /// Shared token tracker
    token_tracker: Arc<TokenTracker>,
    /// Path to SQLite database
    db_path: PathBuf,
}

impl IndexerManager {
    pub fn new(
        storage: Arc<dyn StorageBackend + Send + Sync>,
        embedding_store: EmbeddingStore,
        token_tracker: Arc<TokenTracker>,
        db_path: PathBuf,
    ) -> Self {
        Self {
            indexers: HashMap::new(),
            storage,
            embedding_store,
            token_tracker,
            db_path,
        }
    }

    /// Spawn an indexer for a workspace if not already running
    pub fn spawn_for_workspace(
        &mut self,
        project_id: String,
        workspace_root: String,
    ) -> bool {
        // Don't spawn if already running for this project
        if self.indexers.contains_key(&project_id) {
            tracing::debug!(
                "IndexerManager: indexer already running for {}",
                project_id
            );
            return false;
        }

        let storage = self.storage.clone();
        let embedding_store = self.embedding_store.clone();
        let token_tracker = self.token_tracker.clone();
        let pid = project_id.clone();
        let root = PathBuf::from(workspace_root);
        let db_path = self.db_path.clone();

        let handle = tokio::spawn(async move {
            // Small delay to allow workspace registration to complete
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            let indexer = BackgroundIndexer::new(
                root,
                pid.clone(),
                storage,
                embedding_store,
                token_tracker,
                db_path,
            );
            
            tracing::info!("IndexerManager: starting indexer for {}", pid);
            indexer.run_if_needed().await;
            tracing::info!("IndexerManager: indexer completed for {}", pid);
        });

        self.indexers.insert(project_id, handle);
        true
    }

    /// Cancel an indexer for a workspace (called when workspace is unregistered)
    pub fn cancel(&mut self, project_id: &str) -> bool {
        if let Some(handle) = self.indexers.remove(project_id) {
            handle.abort();
            tracing::info!("IndexerManager: cancelled indexer for {}", project_id);
            true
        } else {
            false
        }
    }

    /// Check if an indexer is running for a project
    pub fn is_running(&self, project_id: &str) -> bool {
        self.indexers.get(project_id)
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    /// Get count of running indexers
    pub fn running_count(&self) -> usize {
        self.indexers.iter()
            .filter(|(_, h)| !h.is_finished())
            .count()
    }

    /// Clean up finished indexers
    pub fn cleanup_finished(&mut self) {
        self.indexers.retain(|_, h| !h.is_finished());
    }
}
