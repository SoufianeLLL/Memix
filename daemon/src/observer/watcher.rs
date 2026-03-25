use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher, event::EventKind};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tracing::{info, error};

// Directories to exclude from watching - these generate noise, not signal
const EXCLUDE_PATTERNS: &[&str] = &[
    ".next/dev",      // Next.js dev builds (high churn, no signal)
    "node_modules",   // Dependencies
    ".git",           // Git internals
    "target",         // Rust build artifacts
    "dist", "build",  // Build outputs
    ".cache",         // Cache directories
    "__pycache__",    // Python cache
    ".venv", "venv",  // Python virtual environments
    "vendor",         // Vendored dependencies
];

fn should_exclude(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    EXCLUDE_PATTERNS.iter().any(|pattern| path_str.contains(pattern))
}

pub async fn start_watcher(workspace_root: String, tx: Sender<Event>) -> anyhow::Result<()> {
    // We use a standard std::sync::mpsc channel for the notify callback,
    // then bridge it to the tokio async world using the provided Sender.
    let (std_tx, std_rx) = channel();

    // Create the watcher
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, _>| {
            if let Ok(event) = res {
                // Filter out noise directories
                if event.paths.iter().any(|p| should_exclude(p)) {
                    return;
                }
                // Filter out low-value events
                if matches!(event.kind, EventKind::Any | EventKind::Other) {
                    return;
                }
                let _ = std_tx.send(event);
            }
        },
        Config::default().with_poll_interval(Duration::from_secs(2)),
    )?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    info!("Starting native FSEvents watcher on: {}", workspace_root);
    watcher.watch(Path::new(&workspace_root), RecursiveMode::Recursive)?;

    // Spawn a blocking task to bridge from std channel to tokio channel
    tokio::task::spawn_blocking(move || {
        // Keep the watcher alive in this thread
        let _keep_watcher_alive = watcher;
        
        while let Ok(event) = std_rx.recv() {
            // Forward event to the main async event loop
            if let Err(e) = tx.blocking_send(event) {
                error!("Failed to forward file event to daemon core: {}", e);
                break;
            }
        }
    });

    Ok(())
}
