use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tracing::{info, error};

pub async fn start_watcher(workspace_root: String, tx: Sender<Event>) -> anyhow::Result<()> {
    // We use a standard std::sync::mpsc channel for the notify callback,
    // then bridge it to the tokio async world using the provided Sender.
    let (std_tx, std_rx) = channel();

    // Create the watcher
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
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
