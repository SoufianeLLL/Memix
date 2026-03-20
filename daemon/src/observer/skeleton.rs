use crate::brain::schema::{MemoryEntry, MemoryKind, MemorySource};
use crate::observer::call_graph::CallGraph;
use crate::observer::graph::DependencyGraph;
use crate::observer::imports::extract_imports;
use crate::observer::parser::AstNodeFeature;
use chrono::Utc;

// ─── Production safeguards ───────────────────────────────────────────────────

fn max_functions_per_file() -> usize {
    std::env::var("MEMIX_MAX_FUNCTIONS_PER_FILE").ok().and_then(|s| s.parse().ok()).unwrap_or(50)
}
fn max_types_per_file() -> usize {
    std::env::var("MEMIX_MAX_TYPES_PER_FILE").ok().and_then(|s| s.parse().ok()).unwrap_or(30)
}
fn max_imports_per_file() -> usize {
    std::env::var("MEMIX_MAX_IMPORTS_PER_FILE").ok().and_then(|s| s.parse().ok()).unwrap_or(20)
}
fn max_deps_per_file() -> usize {
    std::env::var("MEMIX_MAX_DEPS_PER_FILE").ok().and_then(|s| s.parse().ok()).unwrap_or(20)
}
fn max_symbols_per_hot_file() -> usize {
    std::env::var("MEMIX_MAX_SYMBOLS_PER_HOT_FILE").ok().and_then(|s| s.parse().ok()).unwrap_or(50)
}

// ─── Public structs ──────────────────────────────────────────────────────────

pub struct FileSkeleton {
    pub path: String,
    pub language: String,
    pub types: Vec<String>,
    pub functions: Vec<FunctionShape>,
    pub exports: Vec<String>,
    pub imports: Vec<String>,
    pub depends_on: Vec<String>,
    pub depended_by: Vec<String>,
    pub avg_complexity: f32,
    pub symbol_count: usize,
}

pub struct FunctionShape {
    pub name: String,
    pub kind: String,
    pub visibility: String,
    pub is_async: bool,
    pub complexity: u32,
    pub pattern_tags: Vec<String>,
    pub line_count: u32,
}

// ─── Entry ID helpers ────────────────────────────────────────────────────────

pub fn file_skeleton_id(file_path: &str) -> String {
    format!("fsi::{}", normalize_path(file_path))
}

pub fn symbol_entry_id(file_path: &str, symbol_name: &str, kind: &str) -> String {
    // Sanitize symbol_name to prevent key injection (strip :: and whitespace)
    let safe_name = symbol_name
        .replace("::", "_")
        .replace(char::is_whitespace, "_");
    format!(
        "fusi::{}::{}::{}",
        normalize_path(file_path),
        safe_name,
        kind
    )
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

// ─── Builder ─────────────────────────────────────────────────────────────────

impl FileSkeleton {
    pub fn build(
        file_path: &str,
        features: &[AstNodeFeature],
        graph: &DependencyGraph,
        file_content: &str,
    ) -> Self {
        let language = features
            .first()
            .map(|f| f.language.clone())
            .unwrap_or_else(|| detect_language_from_path(file_path));

        let ext = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let types: Vec<String> = features
            .iter()
            .filter(|f| {
                matches!(
                    f.kind.as_str(),
                    "class" | "interface" | "type" | "enum" | "struct"
                )
            })
            .take(max_types_per_file())
            .map(|f| f.name.clone())
            .collect();

        let functions: Vec<FunctionShape> = features
            .iter()
            .filter(|f| matches!(f.kind.as_str(), "function" | "method" | "constructor"))
            .take(max_functions_per_file())
            .map(|f| FunctionShape {
                name: f.name.clone(),
                kind: f.kind.clone(),
                visibility: if f.is_exported {
                    "public".to_string()
                } else {
                    "private".to_string()
                },
                is_async: f.pattern_tags.contains(&"async".to_string()),
                complexity: f.cyclomatic_complexity,
                pattern_tags: f.pattern_tags.clone(),
                line_count: f.line_count.unwrap_or(0),
            })
            .collect();

        let exports: Vec<String> = features
            .iter()
            .filter(|f| f.is_exported)
            .map(|f| f.name.clone())
            .collect();

        let imports: Vec<String> = extract_imports(ext, file_content)
            .into_iter()
            .take(max_imports_per_file())
            .collect();

        let depends_on: Vec<String> = graph
            .edges_out
            .get(file_path)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(max_deps_per_file())
            .collect();

        let depended_by: Vec<String> = graph
            .edges_in
            .get(file_path)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(max_deps_per_file())
            .collect();

        let avg_complexity = if functions.is_empty() {
            0.0
        } else {
            functions.iter().map(|f| f.complexity as f32).sum::<f32>() / functions.len() as f32
        };

        let symbol_count = features.len();

        Self {
            path: normalize_path(file_path),
            language,
            types,
            functions,
            exports,
            imports,
            depends_on,
            depended_by,
            avg_complexity,
            symbol_count,
        }
    }

    /// Renders the skeleton to the compact string stored in MemoryEntry.content.
    pub fn render(&self) -> String {
        let mut lines = vec![
            "[skeleton:file]".to_string(),
            format!("path: {}", self.path),
            format!("language: {}", self.language),
            format!("symbols: {}", self.symbol_count),
        ];

        if !self.types.is_empty() {
            lines.push(format!("types: {}", self.types.join(", ")));
        }

        for func in &self.functions {
            let async_prefix = if func.is_async { "async " } else { "" };
            let vis_prefix = if func.visibility == "public" {
                "pub "
            } else {
                ""
            };
            lines.push(format!(
                "  {}{}fn {} [cx={}]",
                vis_prefix, async_prefix, func.name, func.complexity
            ));
        }

        if !self.exports.is_empty() {
            lines.push(format!(
                "exports: {}",
                self.exports
                    .iter()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.imports.is_empty() {
            lines.push(format!(
                "imports: [{}]",
                self.imports
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.depends_on.is_empty() {
            lines.push(format!(
                "depends_on: [{}]",
                self.depends_on
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.depended_by.is_empty() {
            lines.push(format!(
                "depended_by: [{}]",
                self.depended_by
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if self.avg_complexity > 0.0 {
            lines.push(format!("avg_complexity: {:.1}", self.avg_complexity));
        }

        lines.join("\n")
    }

    /// Converts to a MemoryEntry for upsert into Redis (FSI layer).
    pub fn to_memory_entry(&self, project_id: &str) -> MemoryEntry {
        let now = Utc::now();
        MemoryEntry {
            id: file_skeleton_id(&self.path),
            project_id: project_id.to_string(),
            kind: MemoryKind::Context,
            content: self.render(),
            tags: vec![
                "skeleton".to_string(),
                "fsi".to_string(),
                self.language.clone(),
            ],
            source: MemorySource::FileWatcher,
            superseded_by: None,
            contradicts: vec![],
            parent_id: None,
            caused_by: vec![],
            enables: self.exports.clone(),
            created_at: now,
            updated_at: now,
            access_count: 0,
            last_accessed_at: None,
        }
    }

    /// Returns one MemoryEntry per function/symbol for the FuSI layer.
    /// Only call this for active/hot files — not for every file.
    pub fn to_symbol_entries(
        &self,
        project_id: &str,
        call_graph: &CallGraph,
    ) -> Vec<MemoryEntry> {
        let now = Utc::now();
        self.functions
            .iter()
            .take(max_symbols_per_hot_file())
            .map(|func| {
                let calls = call_graph.calls_from(&self.path, &func.name);
                let called_by = call_graph.callers_of(&self.path, &func.name);

                let content = render_symbol_entry(
                    &func.name,
                    &func.kind,
                    &self.path,
                    &func.visibility,
                    func.is_async,
                    func.complexity,
                    &calls,
                    &called_by,
                    &func.pattern_tags,
                    func.line_count,
                );

                MemoryEntry {
                    id: symbol_entry_id(&self.path, &func.name, &func.kind),
                    project_id: project_id.to_string(),
                    kind: MemoryKind::Context,
                    content,
                    tags: vec![
                        "skeleton".to_string(),
                        "fusi".to_string(),
                        func.kind.clone(),
                        self.language.clone(),
                    ],
                    source: MemorySource::FileWatcher,
                    superseded_by: None,
                    contradicts: vec![],
                    parent_id: Some(file_skeleton_id(&self.path)),
                    caused_by: vec![],
                    enables: vec![func.name.clone()],
                    created_at: now,
                    updated_at: now,
                    access_count: 0,
                    last_accessed_at: None,
                }
            })
            .collect()
    }
}

// ─── Renderers ───────────────────────────────────────────────────────────────

fn render_symbol_entry(
    name: &str,
    kind: &str,
    file_path: &str,
    visibility: &str,
    is_async: bool,
    complexity: u32,
    calls: &[String],
    called_by: &[String],
    patterns: &[String],
    line_count: u32,
) -> String {
    let mut lines = vec![
        format!("[skeleton:{}]", kind),
        format!("name: {}", name),
        format!("file: {}", file_path),
        format!("visibility: {}", visibility),
    ];
    if is_async {
        lines.push("async: true".to_string());
    }
    lines.push(format!("complexity: {}", complexity));
    if line_count > 0 {
        lines.push(format!("lines: {}", line_count));
    }
    if !calls.is_empty() {
        lines.push(format!(
            "calls: [{}]",
            calls
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !called_by.is_empty() {
        lines.push(format!(
            "called_by: [{}]",
            called_by
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !patterns.is_empty() {
        lines.push(format!(
            "patterns: [{}]",
            patterns
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines.join("\n")
}

fn detect_language_from_path(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" => "kotlin",
        "swift" => "swift",
        "cs" => "csharp",
        "cpp" | "cc" | "cxx" => "cpp",
        "rb" => "ruby",
        "php" => "php",
        _ => "unknown",
    }
    .to_string()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer::graph::DependencyGraph;
    use crate::observer::parser::AstNodeFeature;

    fn mock_feature(name: &str, kind: &str, exported: bool, complexity: u32) -> AstNodeFeature {
        AstNodeFeature {
            name: name.to_string(),
            kind: kind.to_string(),
            body: format!("fn {}() {{}}", name),
            start_byte: 0,
            end_byte: 20,
            language: "rust".to_string(),
            cyclomatic_complexity: complexity,
            pattern_tags: vec![],
            is_exported: exported,
            calls: vec!["helper".to_string()],
            line_count: Some(10),
        }
    }

    #[test]
    fn test_file_skeleton_id() {
        assert_eq!(file_skeleton_id("src/main.rs"), "fsi::src/main.rs");
        assert_eq!(
            file_skeleton_id("./src/main.rs"),
            "fsi::src/main.rs"
        );
        assert_eq!(
            file_skeleton_id("src\\main.rs"),
            "fsi::src/main.rs"
        );
    }

    #[test]
    fn test_symbol_entry_id() {
        assert_eq!(
            symbol_entry_id("src/main.rs", "main", "function"),
            "fusi::src/main.rs::main::function"
        );
    }

    #[test]
    fn test_symbol_entry_id_sanitization() {
        // :: in symbol names should be escaped
        assert_eq!(
            symbol_entry_id("src/main.rs", "Foo::bar", "method"),
            "fusi::src/main.rs::Foo_bar::method"
        );
    }

    #[test]
    fn test_file_skeleton_build_and_render() {
        let features = vec![
            mock_feature("main", "function", true, 3),
            mock_feature("helper", "function", false, 1),
            mock_feature("Config", "class", true, 0),
        ];
        let graph = DependencyGraph::new();

        let skeleton = FileSkeleton::build("src/main.rs", &features, &graph, "use std::io;\nfn main() {}");

        assert_eq!(skeleton.path, "src/main.rs");
        assert_eq!(skeleton.language, "rust");
        assert_eq!(skeleton.functions.len(), 2);
        assert_eq!(skeleton.types.len(), 1);
        assert_eq!(skeleton.symbol_count, 3);
        assert!((skeleton.avg_complexity - 2.0).abs() < 0.01);

        let rendered = skeleton.render();
        assert!(rendered.contains("[skeleton:file]"));
        assert!(rendered.contains("path: src/main.rs"));
        assert!(rendered.contains("pub fn main"));
        assert!(rendered.contains("fn helper"));
        assert!(rendered.contains("types: Config"));
    }

    #[test]
    fn test_file_skeleton_to_memory_entry() {
        let features = vec![mock_feature("main", "function", true, 5)];
        let graph = DependencyGraph::new();
        let skeleton = FileSkeleton::build("src/main.rs", &features, &graph, "");
        let entry = skeleton.to_memory_entry("test-project");

        assert_eq!(entry.id, "fsi::src/main.rs");
        assert_eq!(entry.project_id, "test-project");
        assert!(entry.tags.contains(&"skeleton".to_string()));
        assert!(entry.tags.contains(&"fsi".to_string()));
    }

    #[test]
    fn test_file_skeleton_to_symbol_entries() {
        let features = vec![
            mock_feature("main", "function", true, 5),
            mock_feature("helper", "function", false, 1),
        ];
        let graph = DependencyGraph::new();
        let skeleton = FileSkeleton::build("src/main.rs", &features, &graph, "");

        let mut cg = CallGraph::new();
        cg.update_file("src/main.rs", vec![
            ("main".to_string(), vec!["helper".to_string()]),
        ]);

        let entries = skeleton.to_symbol_entries("test-project", &cg);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].tags.contains(&"fusi".to_string()));
        assert!(entries[0].content.contains("calls: [helper]"));
    }
}
