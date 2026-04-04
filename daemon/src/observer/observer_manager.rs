// Multi-workspace observer manager.
// Spawns and manages per-workspace file watchers with tagged events.
// Routes events to workspace-specific processor state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc::{channel, Sender, Receiver};
use tracing::{info, warn, debug};

use crate::storage::StorageBackend;
use crate::observer::watcher;
use crate::observer::embedding_store::EmbeddingStore;
use crate::observer::workspace_processor::WorkspaceProcessor;
use crate::token::tracker::TokenTracker;
use crate::intelligence::autonomous::AutonomousPairProgrammer;
use crate::intelligence::predictor::ContextPredictor;
use crate::observer::call_graph::CallGraph;
use crate::recorder::flight::FlightRecorder;
use crate::observer::dna::ProjectCodeDna;
use crate::git::archaeologist::ProjectGitInsights;

/// Tagged file event that includes the originating project_id
#[derive(Debug, Clone)]
pub struct TaggedEvent {
    pub project_id: String,
    pub workspace_root: String,
    pub event: notify::Event,
}

/// Shared state for the event processor
pub struct ProcessorState {
    pub storage: Arc<dyn StorageBackend + Send + Sync>,
    pub autonomous: Arc<tokio::sync::Mutex<AutonomousPairProgrammer>>,
    pub recorder: Arc<FlightRecorder>,
    pub predictor: Arc<ContextPredictor>,
    pub call_graph: Arc<tokio::sync::Mutex<CallGraph>>,
    pub agent_runtime: Arc<tokio::sync::Mutex<crate::agents::AgentRuntime>>,
    pub code_dna: Arc<tokio::sync::Mutex<ProjectCodeDna>>,
    pub git_insights: Arc<tokio::sync::Mutex<ProjectGitInsights>>,
    pub daemon_config: Arc<tokio::sync::RwLock<crate::server::DaemonConfig>>,
}

/// Manages per-workspace file watchers with a shared event processor
pub struct ObserverManager {
    /// Running watcher tasks keyed by project_id
    watchers: HashMap<String, tokio::task::JoinHandle<()>>,
    /// Per-workspace processors keyed by project_id
    processors: HashMap<String, WorkspaceProcessor>,
    /// Shared storage backend
    storage: Arc<dyn StorageBackend + Send + Sync>,
    /// Shared embedding store
    embedding_store: EmbeddingStore,
    /// Shared token tracker
    token_tracker: Arc<TokenTracker>,
    /// Channel for all tagged events from all watchers
    event_tx: Sender<TaggedEvent>,
    /// Receiver for the event processor loop
    event_rx: Option<Receiver<TaggedEvent>>,
}

impl ObserverManager {
    pub fn new(
        storage: Arc<dyn StorageBackend + Send + Sync>,
        embedding_store: EmbeddingStore,
        token_tracker: Arc<TokenTracker>,
    ) -> Self {
        // Create a bounded channel for tagged events
        // Large enough buffer to handle burst of file changes
        let (event_tx, event_rx) = channel(500);
        
        Self {
            watchers: HashMap::new(),
            processors: HashMap::new(),
            storage,
            embedding_store,
            token_tracker,
            event_tx,
            event_rx: Some(event_rx),
        }
    }
    
    /// Get a clone of the event sender for spawning the processor
    pub fn event_sender(&self) -> Sender<TaggedEvent> {
        self.event_tx.clone()
    }
    
    /// Take the event receiver (can only be called once)
    pub fn take_event_receiver(&mut self) -> Option<Receiver<TaggedEvent>> {
        self.event_rx.take()
    }
    
    /// Spawn a file watcher for a workspace if not already running
    pub fn spawn_for_workspace(
        &mut self,
        project_id: String,
        workspace_root: String,
    ) -> bool {
        // Don't spawn if already running for this project
        if self.watchers.contains_key(&project_id) {
            debug!("ObserverManager: watcher already running for {}", project_id);
            return false;
        }
        
        // Create workspace processor if not exists
        if !self.processors.contains_key(&project_id) {
            let processor = WorkspaceProcessor::new(
                project_id.clone(),
                PathBuf::from(&workspace_root),
            );
            self.processors.insert(project_id.clone(), processor);
        }
        
        let event_tx = self.event_tx.clone();
        let pid = project_id.clone();
        let root = workspace_root.clone();
        
        let handle = tokio::spawn(async move {
            // Create a channel for raw events from the watcher
            let (raw_tx, mut raw_rx) = tokio::sync::mpsc::channel(100);
            
            // Start the file watcher for this workspace
            match watcher::start_watcher(root.clone(), raw_tx).await {
                Ok(()) => {
                    info!("ObserverManager: watcher started for {} ({})", pid, root);
                    
                    // Bridge raw events to tagged events
                    while let Some(event) = raw_rx.recv().await {
                        let tagged = TaggedEvent {
                            project_id: pid.clone(),
                            workspace_root: root.clone(),
                            event,
                        };
                        
                        if let Err(e) = event_tx.send(tagged).await {
                            warn!("ObserverManager: failed to forward tagged event: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    warn!("ObserverManager: watcher failed for {}: {}", pid, e);
                }
            }
            
            info!("ObserverManager: watcher stopped for {}", pid);
        });
        
        self.watchers.insert(project_id, handle);
        true
    }
    
    /// Cancel a watcher for a workspace (called when workspace is unregistered)
    pub fn cancel(&mut self, project_id: &str) -> bool {
        // Remove processor state
        self.processors.remove(project_id);
        
        // Cancel watcher task
        if let Some(handle) = self.watchers.remove(project_id) {
            handle.abort();
            info!("ObserverManager: cancelled watcher for {}", project_id);
            true
        } else {
            false
        }
    }
    
    /// Get mutable access to a workspace processor
    pub fn get_processor(&mut self, project_id: &str) -> Option<&mut WorkspaceProcessor> {
        self.processors.get_mut(project_id)
    }
    
    /// Get cloned references to a processor's Code DNA and Git Insights (for read-only access)
    pub fn get_processor_insights(&self, project_id: &str) -> Option<(
        Arc<tokio::sync::Mutex<ProjectCodeDna>>,
        Arc<tokio::sync::Mutex<ProjectGitInsights>>,
    )> {
        self.processors.get(project_id).map(|p| {
            (p.code_dna.clone(), p.git_insights.clone())
        })
    }
    
    /// Check if a watcher is running for a project
    pub fn is_running(&self, project_id: &str) -> bool {
        self.watchers.get(project_id)
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }
    
    /// Get count of running watchers
    pub fn running_count(&self) -> usize {
        self.watchers.iter()
            .filter(|(_, h)| !h.is_finished())
            .count()
    }
    
    /// Clean up finished watchers
    pub fn cleanup_finished(&mut self) {
        self.watchers.retain(|_, h| !h.is_finished());
    }
    
    /// Get storage reference
    pub fn storage(&self) -> Arc<dyn StorageBackend + Send + Sync> {
        self.storage.clone()
    }
}
