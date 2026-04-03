// Multi-tenant workspace registry for supporting multiple VS Code windows/projects
// simultaneously. Each workspace registers itself on window open and unregisters on close.
// Background indexing runs independently per-workspace.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Per-workspace state tracked by the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub project_id: String,
    pub workspace_root: String,
    /// When this workspace was last active (focused window)
    pub last_active_at: u64,
    /// Whether background indexing has completed for this workspace
    pub indexing_complete: bool,
    /// Number of files indexed
    pub files_indexed: u64,
}

/// Active workspace context - which project is currently "focused"
#[derive(Debug, Clone)]
pub struct ActiveContext {
    pub project_id: String,
    pub workspace_root: String,
    pub activated_at: Instant,
}

/// Registry managing all open workspaces
pub struct WorkspaceRegistry {
    /// All registered workspaces keyed by project_id
    workspaces: HashMap<String, WorkspaceEntry>,
    /// Currently active (focused) workspace
    active: Option<ActiveContext>,
    /// Per-workspace indexer handles for cancellation
    indexer_handles: HashMap<String, tokio::task::JoinHandle<()>>,
}

impl WorkspaceRegistry {
    pub fn new() -> Self {
        Self {
            workspaces: HashMap::new(),
            active: None,
            indexer_handles: HashMap::new(),
        }
    }

    /// Register a new workspace. Returns true if newly registered.
    pub fn register(&mut self, project_id: String, workspace_root: String) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let is_new = !self.workspaces.contains_key(&project_id);
        
        self.workspaces.insert(project_id.clone(), WorkspaceEntry {
            project_id: project_id.clone(),
            workspace_root: workspace_root.clone(),
            last_active_at: now,
            indexing_complete: false,
            files_indexed: 0,
        });
        
        // Set as active if this is the first workspace or update timestamp
        if self.active.is_none() {
            self.active = Some(ActiveContext {
                project_id,
                workspace_root,
                activated_at: Instant::now(),
            });
        }
        
        is_new
    }

    /// Unregister a workspace (window closed)
    pub fn unregister(&mut self, project_id: &str) -> Option<WorkspaceEntry> {
        // Cancel any running indexer for this workspace
        if let Some(handle) = self.indexer_handles.remove(project_id) {
            handle.abort();
        }
        
        // If this was the active workspace, clear active context
        if let Some(ref active) = self.active {
            if active.project_id == project_id {
                self.active = None;
            }
        }
        
        self.workspaces.remove(project_id)
    }

    /// Set a workspace as active (window focused)
    pub fn set_active(&mut self, project_id: &str) -> bool {
        if let Some(entry) = self.workspaces.get_mut(project_id) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            entry.last_active_at = now;
            
            self.active = Some(ActiveContext {
                project_id: project_id.to_string(),
                workspace_root: entry.workspace_root.clone(),
                activated_at: Instant::now(),
            });
            true
        } else {
            false
        }
    }

    /// Get the currently active workspace
    pub fn get_active(&self) -> Option<&ActiveContext> {
        self.active.as_ref()
    }

    /// Get workspace entry by project_id
    pub fn get(&self, project_id: &str) -> Option<&WorkspaceEntry> {
        self.workspaces.get(project_id)
    }

    /// Get mutable workspace entry
    pub fn get_mut(&mut self, project_id: &str) -> Option<&mut WorkspaceEntry> {
        self.workspaces.get_mut(project_id)
    }

    /// List all registered workspaces
    pub fn list(&self) -> Vec<&WorkspaceEntry> {
        self.workspaces.values().collect()
    }

    /// Check if a workspace is registered
    pub fn contains(&self, project_id: &str) -> bool {
        self.workspaces.contains_key(project_id)
    }

    /// Store an indexer handle for later cancellation
    pub fn set_indexer_handle(&mut self, project_id: String, handle: tokio::task::JoinHandle<()>) {
        self.indexer_handles.insert(project_id, handle);
    }

    /// Mark indexing complete for a workspace
    pub fn mark_indexing_complete(&mut self, project_id: &str, files_indexed: u64) {
        if let Some(entry) = self.workspaces.get_mut(project_id) {
            entry.indexing_complete = true;
            entry.files_indexed = files_indexed;
        }
    }

    /// Get count of registered workspaces
    pub fn len(&self) -> usize {
        self.workspaces.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.workspaces.is_empty()
    }
}

impl Default for WorkspaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable snapshot for health endpoint
#[derive(Debug, Serialize)]
pub struct RegistrySnapshot {
    pub workspaces: Vec<WorkspaceEntry>,
    pub active_project_id: Option<String>,
    pub active_workspace_root: Option<String>,
}

impl From<&WorkspaceRegistry> for RegistrySnapshot {
    fn from(registry: &WorkspaceRegistry) -> Self {
        Self {
            workspaces: registry.list().into_iter().cloned().collect(),
            active_project_id: registry.active.as_ref().map(|a| a.project_id.clone()),
            active_workspace_root: registry.active.as_ref().map(|a| a.workspace_root.clone()),
        }
    }
}
