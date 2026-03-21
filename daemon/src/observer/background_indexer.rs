// Background task that walks the workspace, builds FSI + FuSI skeleton entries,
// and computes embeddings for all entries. Runs once at daemon startup when the
// skeleton index is empty, then stays dormant until the next cold start.
//
// Throttle: processes at most 10 files per second to avoid starving the
// main event loop. The developer can adjust this with MEMIX_INDEXER_RATE_LIMIT.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::observer::call_graph::CallGraph;
use crate::observer::embedding_store::EmbeddingStore;
use crate::observer::parser::AstParser;
use crate::observer::skeleton::FileSkeleton;
use crate::storage::StorageBackend;
use crate::token::tracker::TokenTracker;

const DEFAULT_RATE_LIMIT_FILES_PER_SEC: u64 = 10;

pub struct BackgroundIndexer {
    workspace_root: PathBuf,
    project_id: String,
    storage: Arc<dyn StorageBackend + Send + Sync>,
    embedding_store: EmbeddingStore,
    token_tracker: Arc<TokenTracker>,
}

impl BackgroundIndexer {
    pub fn new(
        workspace_root: PathBuf,
        project_id: String,
        storage: Arc<dyn StorageBackend + Send + Sync>,
        embedding_store: EmbeddingStore,
        token_tracker: Arc<TokenTracker>,
    ) -> Self {
        Self { workspace_root, project_id, storage, embedding_store, token_tracker }
    }

    pub async fn run_if_needed(&self) {
        // Only run if the embedding store is empty (cold start) or
        // if explicitly requested via environment variable
        let force = std::env::var("MEMIX_FORCE_REINDEX").unwrap_or_default() == "true";
        if !force && !self.embedding_store.is_empty().await {
            tracing::debug!("BackgroundIndexer: skeleton index already populated, skipping full scan");
            return;
        }

        tracing::info!(
            "BackgroundIndexer: starting background index of workspace {:?}",
            self.workspace_root
        );

        let rate_limit = std::env::var("MEMIX_INDEXER_RATE_LIMIT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_RATE_LIMIT_FILES_PER_SEC);

        let delay_between_files = Duration::from_millis(1000 / rate_limit.max(1));
        let graph = crate::observer::graph::DependencyGraph::new();
        // let call_graph = CallGraph::new(); // unused, passing into skeleton was removed earlier? Let's check. 
        // Wait, the FSI Fusi code in implementation_plan doesn't use call_graph. Let's keep it as is.

        // Walk the workspace and collect all supported source files
        let files = collect_supported_files(&self.workspace_root, 10_000);
        tracing::info!("BackgroundIndexer: found {} files to index", files.len());

        let mut parser = match AstParser::new() {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("BackgroundIndexer: failed to init parser: {}", e);
                return;
            }
        };

        let mut indexed_count = 0u64;

        for file_path in &files {
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !AstParser::is_supported(ext) {
                continue;
            }

            let Ok(bytes) = std::fs::read(file_path) else { continue; };
            let Ok(tree) = parser.parse_file(file_path) else { continue; };
            let Some(tree) = tree else { continue; };

            let features = parser.extract_features(&tree.0, tree.1.clone(), &bytes, ext);
            let key = file_path.to_string_lossy().to_string();

            // Layer 1 + Layer 2: build skeleton (structural analysis)
            let skeleton = FileSkeleton::build(
                &key,
                &features,
                &graph,
                &String::from_utf8_lossy(&bytes),
            );

            let fsi_entry = skeleton.to_memory_entry(&self.project_id);
            let entry_id = fsi_entry.id.clone();
            let content_for_embedding = fsi_entry.content.clone();

            // Persist FSI to Redis
            let _ = self.storage.upsert_skeleton_entry(&self.project_id, fsi_entry).await;

            // Layer 3: compute embedding for this skeleton entry
            // embed_text is async and uses spawn_blocking internally, so this
            // doesn't block the executor even though it's CPU-heavy.
            let embedding = crate::storage::redis::RedisStorage::embed_text_static(
                &content_for_embedding
            ).await;

            self.embedding_store.upsert(&entry_id, embedding).await;
            self.token_tracker.session.files_skeleton_indexed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            indexed_count += 1;

            // Throttle: yield to the event loop between files
            tokio::time::sleep(delay_between_files).await;
        }

        tracing::info!(
            "BackgroundIndexer: completed. Indexed {} files, {} embeddings computed",
            indexed_count,
            self.embedding_store.len().await
        );

        // Flush to disk + Redis after the full scan
        let redis_client = None; // Pass actual client from AppState in real integration
        let _ = self.embedding_store.flush(redis_client).await;
    }
}

fn collect_supported_files(root: &Path, limit: usize) -> Vec<PathBuf> {
    let supported_extensions = ["ts", "tsx", "js", "jsx", "rs", "py", "go", "java",
                                 "kt", "swift", "cs", "cpp", "cc", "rb", "php"];

    let mut files = Vec::new();
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            // Skip directories that are noise, not signal
            !matches!(name.as_ref(), "node_modules" | ".git" | "target" | "dist"
                | "build" | ".next" | ".cache" | "__pycache__" | ".venv" | "vendor")
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if files.len() >= limit {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        if supported_extensions.contains(&ext) {
            files.push(entry.path().to_path_buf());
        }
    }

    files
}
