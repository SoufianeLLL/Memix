// Unified pattern discovery engine — Known + Framework + Emergent

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use walkdir::WalkDir;

// ═══════════════════════════════════════════════════════════════
// PUBLIC TYPES — These are returned to the extension via HTTP
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    pub id: String,
    pub label: String,
    pub category: String,
    pub tier: String,           // "known" | "framework" | "emergent"
    pub confidence: f32,
    pub occurrences: usize,
    pub evidence: Vec<PatternEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternEvidence {
    pub file: String,
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternReport {
    pub patterns: Vec<DetectedPattern>,
    pub total_files_scanned: usize,
    pub total_functions_analyzed: usize,
    pub scan_duration_ms: u64,
}

// ═══════════════════════════════════════════════════════════════
// INTERNAL TYPES — Used during analysis
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct AnalyzedFile {
    relative_path: String,
    functions: Vec<FunctionInfo>,
    imports: Vec<ImportInfo>,
    export_names: Vec<String>,
    has_use_client: bool,
    has_use_server: bool,
    has_default_export: bool,
    line_count: usize,
    has_class: bool,
    class_names: Vec<String>,
    has_jsx: bool,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    name: String,
    file: String,
    line: usize,
    is_async: bool,
    is_exported: bool,
    param_count: usize,
    param_names: Vec<String>,
    has_destructure_param: bool,
    starts_with_guard: bool,
    has_try_catch: bool,
    has_await: bool,
    has_validation: bool,
    has_logging: bool,
    has_throw: bool,
    returns_object_literal: bool,
    body_prefix: Vec<String>,
    called_functions: Vec<String>,
    is_generator: bool,
    body_line_count: usize,
}

#[derive(Debug, Clone)]
struct ImportInfo {
    module: String,
    specifiers: Vec<String>,
    is_default: bool,
    is_type_only: bool,
}

#[derive(Debug, Clone)]
struct ShapeCluster {
    signature: String,
    functions: Vec<FunctionInfo>,
}

// ═══════════════════════════════════════════════════════════════
// PATTERN ENGINE — Main public API
// ═══════════════════════════════════════════════════════════════

pub struct PatternEngine {
    min_occurrences: usize,
    max_body_prefix: usize,
}

impl PatternEngine {
    pub fn new(min_occurrences: usize) -> Self {
        Self {
            min_occurrences: min_occurrences.max(2),
            max_body_prefix: 4,
        }
    }

    pub fn analyze(&self, project_root: &Path) -> PatternReport {
        let start = std::time::Instant::now();

        // Phase 1: Scan and parse all source files
        let files = self.scan_and_parse(project_root);
        let total_files = files.len();
        let total_functions: usize = files.iter().map(|f| f.functions.len()).sum();

        // Phase 2: Read package.json for framework detection
        let dependencies = read_dependencies(project_root);

        // Phase 3: Run all detection strategies
        let mut all_patterns: Vec<DetectedPattern> = Vec::new();

        // Strategy A: Known pattern detection (Tier 1)
        all_patterns.extend(self.detect_known_patterns(&files));

        // Strategy B: Framework-specific patterns (Tier 2)
        all_patterns.extend(self.detect_framework_patterns(&dependencies, &files));

        // Strategy C: Emergent function shape clustering (Tier 3)
        all_patterns.extend(self.discover_function_shapes(&files));

        // Strategy D: Emergent import constellation (Tier 3)
        all_patterns.extend(self.discover_import_constellations(&files));

        // Strategy E: Emergent export shape (Tier 3)
        all_patterns.extend(self.discover_export_shapes(&files));

        // Strategy F: Emergent error handling fingerprint (Tier 3)
        all_patterns.extend(self.discover_error_patterns(&files));

        // Strategy G: Emergent statement sequence patterns (Tier 3)
        all_patterns.extend(self.discover_sequence_patterns(&files));

        // Phase 4: Deduplicate and sort
        all_patterns = self.deduplicate(all_patterns);
        all_patterns.sort_by(|a, b| {
            a.tier.cmp(&b.tier)
                .then(b.occurrences.cmp(&a.occurrences))
                .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
        });

        PatternReport {
            patterns: all_patterns,
            total_files_scanned: total_files,
            total_functions_analyzed: total_functions,
            scan_duration_ms: start.elapsed().as_millis() as u64,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// FILE SCANNING + AST PARSING
// ═══════════════════════════════════════════════════════════════

const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "dist", "build", ".next", ".nuxt",
    "coverage", ".turbo", ".cache", "target", "__pycache__",
    ".svelte-kit", ".output", "vendor", "tmp", ".temp",
];

impl PatternEngine {
    fn scan_and_parse(&self, root: &Path) -> Vec<AnalyzedFile> {
        let mut files = Vec::new();

        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !SKIP_DIRS.iter().any(|skip| name == *skip)
                    && !name.starts_with('.')
            })
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            let is_ts = matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "mts");
            if !is_ts {
                continue;
            }

            // Skip test files for pattern analysis (they skew results)
            let path_str = path.to_string_lossy();
            if path_str.contains("__tests__")
                || path_str.contains(".test.")
                || path_str.contains(".spec.")
                || path_str.contains("__mocks__")
            {
                continue;
            }

            let relative = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if source.len() > 500_000 {
                // Skip files over 500KB (likely generated)
                continue;
            }

            let analyzed = self.analyze_file(&relative, &source, ext);
            files.push(analyzed);
        }

        files
    }

    fn analyze_file(&self, relative_path: &str, source: &str, ext: &str) -> AnalyzedFile {
        let is_tsx = matches!(ext, "tsx" | "jsx");

        let mut parser = tree_sitter::Parser::new();

        let language = if is_tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else if matches!(ext, "js" | "mjs") {
            tree_sitter_javascript::LANGUAGE.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };

        // If set_language fails, return empty analysis
        if parser.set_language(&language).is_err() {
            return empty_analyzed_file(relative_path, source);
        }

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return empty_analyzed_file(relative_path, source),
        };

        let root = tree.root_node();

        let mut functions = Vec::new();
        let mut imports = Vec::new();
        let mut export_names = Vec::new();
        let mut has_default_export = false;
        let mut has_class = false;
        let mut class_names = Vec::new();
        let mut has_jsx = false;

        // Check for directives in first 5 lines
        let first_lines: String = source.lines().take(5).collect::<Vec<_>>().join("\n");
        let has_use_client = first_lines.contains("'use client'") || first_lines.contains("\"use client\"");
        let has_use_server = first_lines.contains("'use server'") || first_lines.contains("\"use server\"");

        // Walk entire AST
        self.walk_node(
            root,
            source,
            relative_path,
            &mut functions,
            &mut imports,
            &mut export_names,
            &mut has_default_export,
            &mut has_class,
            &mut class_names,
            &mut has_jsx,
        );

        AnalyzedFile {
            relative_path: relative_path.to_string(),
            functions,
            imports,
            export_names,
            has_use_client,
            has_use_server,
            has_default_export,
            line_count: source.lines().count(),
            has_class,
            class_names,
            has_jsx,
        }
    }
}

fn empty_analyzed_file(path: &str, source: &str) -> AnalyzedFile {
    AnalyzedFile {
        relative_path: path.to_string(),
        functions: vec![],
        imports: vec![],
        export_names: vec![],
        has_use_client: false,
        has_use_server: false,
        has_default_export: false,
        line_count: source.lines().count(),
        has_class: false,
        class_names: vec![],
        has_jsx: false,
    }
}


// ═══════════════════════════════════════════════════════════════
// AST WALKING — Extract structured info from tree-sitter nodes
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn walk_node(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &str,
        functions: &mut Vec<FunctionInfo>,
        imports: &mut Vec<ImportInfo>,
        export_names: &mut Vec<String>,
        has_default_export: &mut bool,
        has_class: &mut bool,
        class_names: &mut Vec<String>,
        has_jsx: &mut bool,
    ) {
        match node.kind() {
            "function_declaration" => {
                if let Some(info) = self.extract_function(node, source, file_path) {
                    functions.push(info);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                // const foo = () => {} or const foo = function() {}
                for i in 0..node.named_child_count() {
                    if let Some(declarator) = node.named_child(i) {
                        if declarator.kind() == "variable_declarator" {
                            if let Some(value) = declarator.child_by_field_name("value") {
                                if value.kind() == "arrow_function"
                                    || value.kind() == "function"
                                    || value.kind() == "function_expression"
                                {
                                    let name = declarator
                                        .child_by_field_name("name")
                                        .map(|n| node_text(n, source))
                                        .unwrap_or_default();

                                    let is_exported = node
                                        .parent()
                                        .map(|p| p.kind() == "export_statement")
                                        .unwrap_or(false);

                                    if let Some(mut info) =
                                        self.extract_function(value, source, file_path)
                                    {
                                        info.name = name;
                                        info.is_exported = is_exported;
                                        functions.push(info);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "class_declaration" => {
                *has_class = true;
                let cname = node
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source))
                    .unwrap_or_default();
                if !cname.is_empty() {
                    class_names.push(cname.clone());
                }
                // Extract methods
                if let Some(body) = node.child_by_field_name("body") {
                    for i in 0..body.named_child_count() {
                        if let Some(method) = body.named_child(i) {
                            if method.kind() == "method_definition" {
                                let method_name = method
                                    .child_by_field_name("name")
                                    .map(|n| node_text(n, source))
                                    .unwrap_or_default();

                                if let Some(mut info) =
                                    self.extract_function(method, source, file_path)
                                {
                                    info.name = format!("{}.{}", cname, method_name);
                                    functions.push(info);
                                }
                            }
                        }
                    }
                }
            }
            "import_statement" => {
                if let Some(info) = extract_import(node, source) {
                    imports.push(info);
                }
            }
            "export_statement" => {
                // Collect export names
                let text = node_text(node, source);
                if text.contains("default") {
                    *has_default_export = true;
                }
                // Check for exported declaration
                for i in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(i) {
                        match child.kind() {
                            "function_declaration" => {
                                let name = child
                                    .child_by_field_name("name")
                                    .map(|n| node_text(n, source))
                                    .unwrap_or_default();
                                if !name.is_empty() {
                                    export_names.push(name);
                                }
                            }
                            "lexical_declaration" | "variable_declaration" => {
                                for j in 0..child.named_child_count() {
                                    if let Some(decl) = child.named_child(j) {
                                        if decl.kind() == "variable_declarator" {
                                            let name = decl
                                                .child_by_field_name("name")
                                                .map(|n| node_text(n, source))
                                                .unwrap_or_default();
                                            if !name.is_empty() {
                                                export_names.push(name);
                                            }
                                        }
                                    }
                                }
                            }
                            "class_declaration" => {
                                let name = child
                                    .child_by_field_name("name")
                                    .map(|n| node_text(n, source))
                                    .unwrap_or_default();
                                if !name.is_empty() {
                                    export_names.push(name);
                                }
                            }
                            "export_clause" => {
                                // export { foo, bar }
                                for k in 0..child.named_child_count() {
                                    if let Some(spec) = child.named_child(k) {
                                        if spec.kind() == "export_specifier" {
                                            let name = spec
                                                .child_by_field_name("name")
                                                .or_else(|| spec.named_child(0))
                                                .map(|n| node_text(n, source))
                                                .unwrap_or_default();
                                            if !name.is_empty() {
                                                export_names.push(name);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => {
                *has_jsx = true;
            }
            _ => {}
        }

        // Recurse into all children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                // Skip recursing into function bodies we already processed at declaration level
                // This avoids double-counting nested arrow functions as top-level patterns
                let dominated = matches!(
                    child.kind(),
                    "function_declaration" | "class_declaration"
                ) && matches!(node.kind(), "export_statement");

                if !dominated {
                    self.walk_node(
                        child,
                        source,
                        file_path,
                        functions,
                        imports,
                        export_names,
                        has_default_export,
                        has_class,
                        class_names,
                        has_jsx,
                    );
                }
            }
        }
    }

    fn extract_function(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &str,
    ) -> Option<FunctionInfo> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        let params = node.child_by_field_name("parameters");
        let param_count = params.map(|p| p.named_child_count()).unwrap_or(0);

        let mut param_names = Vec::new();
        let mut has_destructure = false;
        if let Some(p) = params {
            for i in 0..p.named_child_count() {
                if let Some(param) = p.named_child(i) {
                    let pname = node_text(param, source);
                    param_names.push(pname);
                    if param.kind() == "object_pattern" || param.kind() == "array_pattern" {
                        has_destructure = true;
                    }
                }
            }
        }

        let body = node.child_by_field_name("body");
        let func_text = node_text(node, source);
        let is_async = func_text.trim_start().starts_with("async");

        let is_exported = node
            .parent()
            .map(|p| p.kind() == "export_statement")
            .unwrap_or(false);

        let is_generator = func_text.contains("function*") || func_text.contains("function *");

        let (
            starts_with_guard,
            has_try_catch,
            has_await,
            has_throw,
            returns_object,
            body_prefix,
            called_functions,
            body_lines,
        ) = if let Some(b) = body {
            (
                check_guard_clause(b, source),
                has_descendant_kind(b, "try_statement"),
                has_descendant_kind(b, "await_expression"),
                has_descendant_kind(b, "throw_statement"),
                check_returns_object(b, source),
                self.extract_body_prefix(b, source),
                extract_calls(b, source),
                b.end_position().row.saturating_sub(b.start_position().row),
            )
        } else {
            (false, false, false, false, false, vec![], vec![], 0)
        };

        let has_validation = check_has_validation(&func_text);
        let has_logging = check_has_logging(&func_text);

        Some(FunctionInfo {
            name,
            file: file_path.to_string(),
            line: node.start_position().row + 1,
            is_async,
            is_exported,
            param_count,
            param_names,
            has_destructure_param: has_destructure,
            starts_with_guard,
            has_try_catch,
            has_await,
            has_validation,
            has_logging,
            has_throw,
            returns_object_literal: returns_object,
            body_prefix,
            called_functions,
            is_generator,
            body_line_count: body_lines,
        })
    }

    fn extract_body_prefix(&self, body: tree_sitter::Node, source: &str) -> Vec<String> {
        let mut prefix = Vec::new();
        for i in 0..body.named_child_count().min(self.max_body_prefix) {
            if let Some(stmt) = body.named_child(i) {
                prefix.push(classify_statement(stmt, source));
            }
        }
        prefix
    }
}


// ═══════════════════════════════════════════════════════════════
// HELPER FUNCTIONS — Node text extraction, descendant checks
// ═══════════════════════════════════════════════════════════════

fn node_text(node: tree_sitter::Node, source: &str) -> String {
    source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or("")
        .to_string()
}

fn has_descendant_kind(node: tree_sitter::Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if has_descendant_kind(child, kind) {
                return true;
            }
        }
    }
    false
}

fn check_guard_clause(body: tree_sitter::Node, _source: &str) -> bool {
    if let Some(first) = body.named_child(0) {
        if first.kind() == "if_statement" {
            if let Some(consequence) = first.child_by_field_name("consequence") {
                return has_descendant_kind(consequence, "return_statement")
                    || has_descendant_kind(consequence, "throw_statement");
            }
        }
    }
    false
}

fn check_returns_object(body: tree_sitter::Node, source: &str) -> bool {
    for i in 0..body.named_child_count() {
        if let Some(stmt) = body.named_child(i) {
            if stmt.kind() == "return_statement" {
                let text = node_text(stmt, source);
                if text.contains("{") && (text.contains("success") || text.contains("data") || text.contains("error")) {
                    return true;
                }
            }
        }
    }
    false
}

fn check_has_validation(text: &str) -> bool {
    text.contains(".parse(")
        || text.contains(".safeParse(")
        || text.contains(".validate(")
        || text.contains("z.object")
        || text.contains("z.string")
        || text.contains("z.number")
        || text.contains("z.array")
        || text.contains("z.enum")
        || text.contains("Joi.")
        || text.contains("yup.")
        || text.contains(".validateSync(")
}

fn check_has_logging(text: &str) -> bool {
    text.contains("console.log")
        || text.contains("console.error")
        || text.contains("console.warn")
        || text.contains("console.info")
        || text.contains("logger.")
        || text.contains("log.info")
        || text.contains("log.error")
        || text.contains("log.warn")
        || text.contains("log.debug")
}

fn classify_statement(node: tree_sitter::Node, source: &str) -> String {
    match node.kind() {
        "if_statement" => {
            if let Some(consequence) = node.child_by_field_name("consequence") {
                if has_descendant_kind(consequence, "return_statement")
                    || has_descendant_kind(consequence, "throw_statement")
                {
                    return "guard".into();
                }
            }
            "if".into()
        }
        "return_statement" => "return".into(),
        "throw_statement" => "throw".into(),
        "try_statement" => "try".into(),
        "for_statement" | "for_in_statement" | "for_of_statement" => "loop".into(),
        "while_statement" | "do_statement" => "loop".into(),
        "switch_statement" => "switch".into(),
        "expression_statement" => {
            if let Some(expr) = node.named_child(0) {
                match expr.kind() {
                    "await_expression" => "await".into(),
                    "call_expression" => {
                        let text = node_text(expr, source);
                        if check_has_validation(&text) {
                            "validate".into()
                        } else if check_has_logging(&text) {
                            "log".into()
                        } else {
                            "call".into()
                        }
                    }
                    "assignment_expression" => "assign".into(),
                    "yield_expression" => "yield".into(),
                    _ => "expr".into(),
                }
            } else {
                "expr".into()
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            let text = node_text(node, source);
            if text.contains("await ") {
                "declare-await".into()
            } else if text.contains("= {") || text.contains("= [") {
                "destructure".into()
            } else {
                "declare".into()
            }
        }
        _ => node.kind().to_string(),
    }
}

fn extract_import(node: tree_sitter::Node, source: &str) -> Option<ImportInfo> {
    let source_node = node.child_by_field_name("source")?;
    let module_raw = node_text(source_node, source);
    let module = module_raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`');

    let full_text = node_text(node, source);
    let is_type_only = full_text.contains("import type");
    let is_default = !full_text.contains('{');

    let mut specifiers = Vec::new();
    // Walk children to find import specifiers
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "import_clause" {
                for j in 0..child.named_child_count() {
                    if let Some(spec_container) = child.named_child(j) {
                        if spec_container.kind() == "named_imports" {
                            for k in 0..spec_container.named_child_count() {
                                if let Some(spec) = spec_container.named_child(k) {
                                    if spec.kind() == "import_specifier" {
                                        let name = spec
                                            .child_by_field_name("name")
                                            .or_else(|| spec.named_child(0))
                                            .map(|n| node_text(n, source))
                                            .unwrap_or_default();
                                        if !name.is_empty() {
                                            specifiers.push(name);
                                        }
                                    }
                                }
                            }
                        } else if spec_container.kind() == "identifier" {
                            specifiers.push(node_text(spec_container, source));
                        }
                    }
                }
            }
        }
    }

    Some(ImportInfo {
        module: module.to_string(),
        specifiers,
        is_default,
        is_type_only,
    })
}

fn extract_calls(node: tree_sitter::Node, source: &str) -> Vec<String> {
    let mut calls = Vec::new();
    collect_calls_recursive(node, source, &mut calls);
    calls
}

fn collect_calls_recursive(node: tree_sitter::Node, source: &str, calls: &mut Vec<String>) {
    if node.kind() == "call_expression" {
        if let Some(func_node) = node.child_by_field_name("function") {
            let name = node_text(func_node, source);
            // Clean up: take only the function/method name, not full chain
            let clean = name
                .rsplit('.')
                .next()
                .unwrap_or(&name)
                .trim()
                .to_string();
            if !clean.is_empty() && clean.len() < 60 {
                calls.push(clean);
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_calls_recursive(child, source, calls);
        }
    }
}

fn read_dependencies(root: &Path) -> Vec<String> {
    let pkg_path = root.join("package.json");
    let content = match std::fs::read_to_string(pkg_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let mut deps = Vec::new();
    if let Some(d) = pkg.get("dependencies").and_then(|v| v.as_object()) {
        deps.extend(d.keys().cloned());
    }
    if let Some(d) = pkg.get("devDependencies").and_then(|v| v.as_object()) {
        deps.extend(d.keys().cloned());
    }
    deps
}



// ═══════════════════════════════════════════════════════════════
// STRATEGY A: KNOWN PATTERN DETECTION (Tier 1)
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn detect_known_patterns(&self, files: &[AnalyzedFile]) -> Vec<DetectedPattern> {
        let mut results = Vec::new();
        let all_functions: Vec<&FunctionInfo> = files.iter().flat_map(|f| &f.functions).collect();

        // ── Singleton ──
        let singletons: Vec<_> = files
            .iter()
            .filter(|f| {
                let has_get_instance = f.functions.iter().any(|func| {
                    func.name.to_lowercase().contains("getinstance")
                        || func.name.to_lowercase().contains("get_instance")
                });
                let singleton_export_names = [
                    "db", "prisma", "client", "redis", "supabase", "firebase",
                    "connection", "pool", "cache", "instance", "app",
                ];
                let is_single_export = f.export_names.len() == 1
                    && f.functions.is_empty()
                    && !f.has_class;
                let has_singleton_name = f.export_names.iter().any(|name| {
                    singleton_export_names
                        .iter()
                        .any(|s| name.to_lowercase() == *s)
                });
                has_get_instance || is_single_export || has_singleton_name
            })
            .collect();
        if !singletons.is_empty() {
            results.push(DetectedPattern {
                id: "singleton".into(),
                label: "Singleton".into(),
                category: "Creational".into(),
                tier: "known".into(),
                confidence: 0.88,
                occurrences: singletons.len(),
                evidence: singletons
                    .iter()
                    .map(|f| PatternEvidence {
                        file: f.relative_path.clone(),
                        name: f.export_names.first().cloned().unwrap_or_default(),
                        line: 1,
                    })
                    .collect(),
            });
        }

        // ── Repository / CRUD Module ──
        let crud_keywords = ["create", "find", "get", "update", "delete", "remove", "list", "upsert", "save", "destroy"];
        let repositories: Vec<_> = files
            .iter()
            .filter(|f| {
                if f.export_names.len() < 2 {
                    return false;
                }
                let crud_count = f.export_names.iter().filter(|name| {
                    let lower = name.to_lowercase();
                    crud_keywords.iter().any(|kw| lower.contains(kw))
                }).count();
                crud_count >= 2
            })
            .collect();
        if !repositories.is_empty() {
            results.push(DetectedPattern {
                id: "repository".into(),
                label: "Repository".into(),
                category: "Data Access".into(),
                tier: "known".into(),
                confidence: 0.90,
                occurrences: repositories.len(),
                evidence: repositories
                    .iter()
                    .map(|f| PatternEvidence {
                        file: f.relative_path.clone(),
                        name: "CRUD module".into(),
                        line: 1,
                    })
                    .collect(),
            });
        }

        // ── Factory ──
        let factory_prefixes = ["create", "build", "make", "new", "generate", "produce"];
        let factories: Vec<_> = all_functions
            .iter()
            .filter(|f| {
                let lower = f.name.to_lowercase();
                factory_prefixes.iter().any(|p| lower.starts_with(p))
                    && f.is_exported
                    && f.returns_object_literal
            })
            .collect();
        if factories.len() >= 2 {
            results.push(DetectedPattern {
                id: "factory".into(),
                label: "Factory".into(),
                category: "Creational".into(),
                tier: "known".into(),
                confidence: 0.82,
                occurrences: factories.len(),
                evidence: factories
                    .iter()
                    .take(10)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        // ── Guard Clauses ──
        let guards: Vec<_> = all_functions
            .iter()
            .filter(|f| f.starts_with_guard)
            .collect();
        if guards.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "guard-clause".into(),
                label: "Guard Clauses".into(),
                category: "Error Handling".into(),
                tier: "known".into(),
                confidence: 0.95,
                occurrences: guards.len(),
                evidence: guards
                    .iter()
                    .take(10)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        // ── Async/Await ──
        let async_fns: Vec<_> = all_functions.iter().filter(|f| f.is_async).collect();
        if async_fns.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "async-await".into(),
                label: "Async/Await".into(),
                category: "Concurrency".into(),
                tier: "known".into(),
                confidence: 0.98,
                occurrences: async_fns.len(),
                evidence: async_fns
                    .iter()
                    .take(5)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        // ── Result Type ──
        let result_fns: Vec<_> = all_functions
            .iter()
            .filter(|f| f.returns_object_literal)
            .collect();
        if result_fns.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "result-type".into(),
                label: "Result Type".into(),
                category: "Error Handling".into(),
                tier: "known".into(),
                confidence: 0.75,
                occurrences: result_fns.len(),
                evidence: result_fns
                    .iter()
                    .take(10)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        // ── Observer / Event Emitter ──
        let observer_methods = ["on", "off", "emit", "addEventListener", "removeEventListener", "subscribe", "unsubscribe"];
        let observers: Vec<_> = files
            .iter()
            .filter(|f| {
                let all_calls: Vec<&str> = f.functions.iter()
                    .flat_map(|func| func.called_functions.iter().map(|s| s.as_str()))
                    .collect();
                observer_methods.iter().filter(|m| all_calls.contains(m)).count() >= 2
            })
            .collect();
        if !observers.is_empty() {
            results.push(DetectedPattern {
                id: "observer".into(),
                label: "Observer / Pub-Sub".into(),
                category: "Behavioral".into(),
                tier: "known".into(),
                confidence: 0.85,
                occurrences: observers.len(),
                evidence: observers
                    .iter()
                    .map(|f| PatternEvidence {
                        file: f.relative_path.clone(),
                        name: "event emitter".into(),
                        line: 1,
                    })
                    .collect(),
            });
        }

        // ── Middleware / Chain of Responsibility ──
        let middleware_fns: Vec<_> = all_functions
            .iter()
            .filter(|f| {
                let has_next = f.param_names.iter().any(|p| p.contains("next"));
                let has_req_res = f.param_names.iter().any(|p| p.contains("req"))
                    && f.param_names.iter().any(|p| p.contains("res"));
                (has_next && f.param_count >= 2) || (has_req_res && f.param_count >= 2)
            })
            .collect();
        if middleware_fns.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "middleware".into(),
                label: "Middleware".into(),
                category: "Behavioral".into(),
                tier: "known".into(),
                confidence: 0.87,
                occurrences: middleware_fns.len(),
                evidence: middleware_fns
                    .iter()
                    .take(10)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        // ── Decorator / HOC ──
        let hoc_prefixes = ["with", "enhance", "wrap", "connect"];
        let decorators: Vec<_> = all_functions
            .iter()
            .filter(|f| {
                let lower = f.name.to_lowercase();
                hoc_prefixes.iter().any(|p| lower.starts_with(p)) && f.is_exported
            })
            .collect();
        if decorators.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "decorator-hoc".into(),
                label: "Decorator / HOC".into(),
                category: "Structural".into(),
                tier: "known".into(),
                confidence: 0.78,
                occurrences: decorators.len(),
                evidence: decorators
                    .iter()
                    .take(10)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        // ── Strategy Pattern ──
        let strategy_files: Vec<_> = files
            .iter()
            .filter(|f| {
                // Files that export a map/record of string → function
                let has_handler_map = f.functions.iter().any(|func| {
                    func.name.to_lowercase().contains("handler")
                        || func.name.to_lowercase().contains("strategy")
                });
                let exports_multiple_same_shape = f.export_names.len() >= 3
                    && f.functions.len() >= 3;
                has_handler_map || exports_multiple_same_shape
            })
            .collect();
        if strategy_files.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "strategy".into(),
                label: "Strategy".into(),
                category: "Behavioral".into(),
                tier: "known".into(),
                confidence: 0.70,
                occurrences: strategy_files.len(),
                evidence: strategy_files
                    .iter()
                    .map(|f| PatternEvidence {
                        file: f.relative_path.clone(),
                        name: "strategy module".into(),
                        line: 1,
                    })
                    .collect(),
            });
        }

        // ── Validation / Schema ──
        let validation_fns: Vec<_> = all_functions
            .iter()
            .filter(|f| f.has_validation)
            .collect();
        if validation_fns.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "input-validation".into(),
                label: "Input Validation".into(),
                category: "Security".into(),
                tier: "known".into(),
                confidence: 0.92,
                occurrences: validation_fns.len(),
                evidence: validation_fns
                    .iter()
                    .take(10)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        // ── Barrel Exports ──
        let barrels: Vec<_> = files
            .iter()
            .filter(|f| {
                let is_index = f.relative_path.ends_with("index.ts")
                    || f.relative_path.ends_with("index.js")
                    || f.relative_path.ends_with("index.tsx");
                is_index && f.functions.is_empty() && !f.export_names.is_empty()
            })
            .collect();
        if barrels.len() >= self.min_occurrences {
            results.push(DetectedPattern {
                id: "barrel-exports".into(),
                label: "Barrel Exports".into(),
                category: "Structural".into(),
                tier: "known".into(),
                confidence: 0.95,
                occurrences: barrels.len(),
                evidence: barrels
                    .iter()
                    .map(|f| PatternEvidence {
                        file: f.relative_path.clone(),
                        name: "barrel index".into(),
                        line: 1,
                    })
                    .collect(),
            });
        }

        results
    }
}



// ═══════════════════════════════════════════════════════════════
// STRATEGY B: FRAMEWORK DETECTION (Tier 2)
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn detect_framework_patterns(
        &self,
        deps: &[String],
        files: &[AnalyzedFile],
    ) -> Vec<DetectedPattern> {
        let mut results = Vec::new();
        let has = |name: &str| deps.iter().any(|d| d == name);
        let has_prefix = |prefix: &str| deps.iter().any(|d| d.starts_with(prefix));

        // ── Next.js ──
        if has("next") {
            let app_files: Vec<_> = files
                .iter()
                .filter(|f| f.relative_path.contains("app/") || f.relative_path.contains("src/app/"))
                .collect();
            if !app_files.is_empty() {
                results.push(make_framework("nextjs-app-router", "Next.js App Router", 1, 0.95));
            }
            let page_files: Vec<_> = files
                .iter()
                .filter(|f| f.relative_path.contains("pages/"))
                .collect();
            if !page_files.is_empty() && app_files.is_empty() {
                results.push(make_framework("nextjs-pages-router", "Next.js Pages Router", 1, 0.95));
            }
            let server_components: Vec<_> = files
                .iter()
                .filter(|f| {
                    (f.relative_path.ends_with(".tsx") || f.relative_path.ends_with(".jsx"))
                        && !f.has_use_client
                        && f.has_jsx
                        && (f.relative_path.contains("app/") || f.relative_path.contains("src/app/"))
                })
                .collect();
            if !server_components.is_empty() {
                results.push(DetectedPattern {
                    id: "server-components".into(),
                    label: "Server Components".into(),
                    category: "Framework".into(),
                    tier: "framework".into(),
                    confidence: 0.90,
                    occurrences: server_components.len(),
                    evidence: server_components.iter().take(5).map(|f| PatternEvidence {
                        file: f.relative_path.clone(), name: "RSC".into(), line: 1,
                    }).collect(),
                });
            }
            let server_actions: Vec<_> = files.iter().filter(|f| f.has_use_server).collect();
            if !server_actions.is_empty() {
                results.push(DetectedPattern {
                    id: "server-actions".into(),
                    label: "Server Actions".into(),
                    category: "Framework".into(),
                    tier: "framework".into(),
                    confidence: 0.95,
                    occurrences: server_actions.len(),
                    evidence: server_actions.iter().take(5).map(|f| PatternEvidence {
                        file: f.relative_path.clone(), name: "'use server'".into(), line: 1,
                    }).collect(),
                });
            }
            let client_components: Vec<_> = files.iter().filter(|f| f.has_use_client).collect();
            if !client_components.is_empty() {
                results.push(DetectedPattern {
                    id: "client-components".into(),
                    label: "Client Components".into(),
                    category: "Framework".into(),
                    tier: "framework".into(),
                    confidence: 0.95,
                    occurrences: client_components.len(),
                    evidence: client_components.iter().take(5).map(|f| PatternEvidence {
                        file: f.relative_path.clone(), name: "'use client'".into(), line: 1,
                    }).collect(),
                });
            }
            let api_routes: Vec<_> = files
                .iter()
                .filter(|f| {
                    f.relative_path.contains("api/")
                        && (f.relative_path.contains("route.") || f.relative_path.contains("/api/"))
                })
                .collect();
            if !api_routes.is_empty() {
                results.push(make_framework_with_evidence(
                    "api-routes", "API Routes", &api_routes,
                ));
            }
        }

        // ── React ──
        if has("react") {
            let all_fns: Vec<&FunctionInfo> = files.iter().flat_map(|f| &f.functions).collect();
            let hooks: Vec<_> = all_fns
                .iter()
                .filter(|f| {
                    f.name.starts_with("use")
                        && f.name.len() > 3
                        && f.name.chars().nth(3).map(|c| c.is_uppercase()).unwrap_or(false)
                })
                .collect();
            if !hooks.is_empty() {
                results.push(DetectedPattern {
                    id: "custom-hooks".into(),
                    label: "Custom Hooks".into(),
                    category: "Framework".into(),
                    tier: "framework".into(),
                    confidence: 0.93,
                    occurrences: hooks.len(),
                    evidence: hooks.iter().take(10).map(|f| PatternEvidence {
                        file: f.file.clone(), name: f.name.clone(), line: f.line,
                    }).collect(),
                });
            }
            let component_files: Vec<_> = files.iter().filter(|f| f.has_jsx).collect();
            if !component_files.is_empty() {
                results.push(make_framework("ui-components", "UI Components", component_files.len(), 0.90));
            }
        }

        // ── Prisma ──
        if has("@prisma/client") || has("prisma") {
            let prisma_calls: Vec<_> = files
                .iter()
                .flat_map(|f| &f.functions)
                .filter(|func| {
                    func.called_functions.iter().any(|c| {
                        c.contains("prisma") || c.contains("findMany") || c.contains("findUnique")
                            || c.contains("$transaction") || c.contains("$queryRaw")
                    })
                })
                .collect();
            if !prisma_calls.is_empty() {
                results.push(DetectedPattern {
                    id: "prisma-orm".into(),
                    label: "Prisma ORM".into(),
                    category: "Framework".into(),
                    tier: "framework".into(),
                    confidence: 0.92,
                    occurrences: prisma_calls.len(),
                    evidence: prisma_calls.iter().take(5).map(|f| PatternEvidence {
                        file: f.file.clone(), name: f.name.clone(), line: f.line,
                    }).collect(),
                });
            }
        }

        // ── tRPC ──
        if has_prefix("@trpc/") {
            results.push(make_framework("trpc", "tRPC", 1, 0.90));
        }

        // ── Express / Fastify / Hono ──
        for (dep, label) in &[("express", "Express"), ("fastify", "Fastify"), ("hono", "Hono")] {
            if has(dep) {
                results.push(make_framework(
                    &format!("{}-framework", dep),
                    &format!("{} Framework", label),
                    1, 0.90,
                ));
            }
        }

        // ── Tailwind ──
        if has("tailwindcss") {
            let cn_calls: usize = files.iter().flat_map(|f| &f.functions)
                .flat_map(|func| &func.called_functions)
                .filter(|c| *c == "cn" || *c == "clsx" || *c == "twMerge" || *c == "cva")
                .count();
            if cn_calls > 0 {
                results.push(make_framework("tailwind-cn", "Class Merging (cn/clsx)", cn_calls, 0.88));
            }
            results.push(make_framework("tailwind", "Tailwind CSS", 1, 0.95));
        }

        // ── Testing frameworks ──
        for (dep, label) in &[("vitest", "Vitest"), ("jest", "Jest"), ("@playwright/test", "Playwright"), ("cypress", "Cypress")] {
            if has(dep) {
                results.push(make_framework(
                    &format!("{}-testing", dep.replace('@', "").replace('/', "-")),
                    &format!("{} Testing", label),
                    1, 0.95,
                ));
            }
        }

        // ── State Management ──
        for (dep, label) in &[("zustand", "Zustand"), ("@reduxjs/toolkit", "Redux Toolkit"), ("jotai", "Jotai"), ("recoil", "Recoil"), ("mobx", "MobX")] {
            if has(dep) {
                results.push(make_framework(
                    &format!("{}-state", dep.replace('@', "").replace('/', "-")),
                    &format!("{} State Management", label),
                    1, 0.90,
                ));
            }
        }

        results
    }
}

fn make_framework(id: &str, label: &str, occurrences: usize, confidence: f32) -> DetectedPattern {
    DetectedPattern {
        id: id.to_string(),
        label: label.to_string(),
        category: "Framework".into(),
        tier: "framework".into(),
        confidence,
        occurrences,
        evidence: vec![],
    }
}

fn make_framework_with_evidence(
    id: &str,
    label: &str,
    files: &[&AnalyzedFile],
) -> DetectedPattern {
    DetectedPattern {
        id: id.to_string(),
        label: label.to_string(),
        category: "Framework".into(),
        tier: "framework".into(),
        confidence: 0.90,
        occurrences: files.len(),
        evidence: files
            .iter()
            .take(5)
            .map(|f| PatternEvidence {
                file: f.relative_path.clone(),
                name: String::new(),
                line: 1,
            })
            .collect(),
    }
}



// ═══════════════════════════════════════════════════════════════
// STRATEGY C: EMERGENT FUNCTION SHAPE CLUSTERING (Tier 3)
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn discover_function_shapes(&self, files: &[AnalyzedFile]) -> Vec<DetectedPattern> {
        let all_functions: Vec<&FunctionInfo> =
            files.iter().flat_map(|f| &f.functions).collect();

        if all_functions.len() < self.min_occurrences {
            return vec![];
        }

        // Compute shape signature for each function
        let mut clusters: HashMap<String, Vec<&FunctionInfo>> = HashMap::new();
        for func in &all_functions {
            let sig = self.compute_shape_signature(func);
            if sig.len() >= 3 {
                // Only cluster functions with meaningful shapes
                clusters.entry(sig).or_default().push(func);
            }
        }

        let mut results = Vec::new();

        for (signature, functions) in &clusters {
            if functions.len() < self.min_occurrences {
                continue;
            }

            // Skip trivially simple shapes (just "async" or just "p1")
            let complexity: usize = signature.matches('|').count();
            if complexity < 2 {
                continue;
            }

            let label = self.generate_shape_label(signature, functions);
            let id = format!("shape-{}", &blake3::hash(signature.as_bytes()).to_hex()[..12]);

            let confidence = compute_cluster_confidence(functions.len(), all_functions.len(), complexity);

            results.push(DetectedPattern {
                id,
                label,
                category: "Emergent".into(),
                tier: "emergent".into(),
                confidence,
                occurrences: functions.len(),
                evidence: functions
                    .iter()
                    .take(10)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        results
    }

    fn compute_shape_signature(&self, func: &FunctionInfo) -> String {
        let mut parts: Vec<&str> = Vec::new();

        if func.is_async {
            parts.push("A");
        }
        if func.starts_with_guard {
            parts.push("G");
        }
        if func.has_destructure_param {
            parts.push("D");
        }
        if func.has_try_catch {
            parts.push("T");
        }
        if func.has_validation {
            parts.push("V");
        }
        if func.has_logging {
            parts.push("L");
        }
        if func.has_throw {
            parts.push("E");
        }
        if func.returns_object_literal {
            parts.push("R");
        }

        let param_bucket = match func.param_count {
            0 => "p0",
            1 => "p1",
            2 => "p2",
            3 => "p3",
            _ => "p4+",
        };
        parts.push(param_bucket);

        // Add body prefix (first N statement types)
        let prefix_str: Vec<&str> = func
            .body_prefix
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect();

        let mut sig = parts.join("|");
        if !prefix_str.is_empty() {
            sig.push('|');
            sig.push_str(&prefix_str.join("|"));
        }

        sig
    }

    fn generate_shape_label(&self, signature: &str, functions: &[&FunctionInfo]) -> String {
        let mut descriptors: Vec<&str> = Vec::new();

        if signature.contains("|G|") || signature.starts_with("G|") {
            descriptors.push("Guard");
        }
        if signature.contains("|V|") || signature.contains("|validate") {
            descriptors.push("Validated");
        }
        if signature.contains("|A|") || signature.starts_with("A|") {
            descriptors.push("Async");
        }
        if signature.contains("|T|") {
            descriptors.push("Try-Catch");
        }
        if signature.contains("|D|") {
            descriptors.push("Destructured");
        }
        if signature.contains("|L|") || signature.contains("|log") {
            descriptors.push("Logged");
        }
        if signature.contains("|R") {
            descriptors.push("Result");
        }
        if signature.contains("|declare-await") {
            descriptors.push("Fetch");
        }

        if !descriptors.is_empty() {
            return format!("{} Pattern (×{})", descriptors.join("-"), functions.len());
        }

        // Try to infer from function names
        let names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();

        if names.iter().all(|n| n.starts_with("handle")) {
            return format!("Handler Pattern (×{})", functions.len());
        }
        if names.iter().all(|n| n.starts_with("use")) {
            return format!("Hook Pattern (×{})", functions.len());
        }
        if names.iter().all(|n| n.starts_with("on")) {
            return format!("Event Handler Pattern (×{})", functions.len());
        }
        if names.iter().all(|n| n.starts_with("get") || n.starts_with("fetch")) {
            return format!("Data Fetcher Pattern (×{})", functions.len());
        }
        if names.iter().all(|n| n.starts_with("render") || n.starts_with("draw")) {
            return format!("Render Pattern (×{})", functions.len());
        }
        if names.iter().all(|n| n.starts_with("transform") || n.starts_with("map") || n.starts_with("convert")) {
            return format!("Transformer Pattern (×{})", functions.len());
        }

        format!("Structural Pattern (×{})", functions.len())
    }
}

fn compute_cluster_confidence(cluster_size: usize, total_functions: usize, complexity: usize) -> f32 {
    let frequency = cluster_size as f32 / total_functions as f32;
    let size_score = (cluster_size as f32).ln() / 10.0; // logarithmic, max around 0.7
    let complexity_score = (complexity as f32).min(5.0) / 5.0; // 0-1

    let raw = (frequency * 0.3 + size_score * 0.4 + complexity_score * 0.3).min(0.95);
    (raw * 100.0).round() / 100.0 // round to 2 decimal places
}



// ═══════════════════════════════════════════════════════════════
// STRATEGY D: IMPORT CONSTELLATION (Tier 3)
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn discover_import_constellations(&self, files: &[AnalyzedFile]) -> Vec<DetectedPattern> {
        let mut results = Vec::new();

        // Group files by their set of local (relative) imports
        let mut import_groups: HashMap<String, Vec<&AnalyzedFile>> = HashMap::new();

        for file in files {
            let local_imports: Vec<&str> = file
                .imports
                .iter()
                .filter(|i| i.module.starts_with('.') || i.module.starts_with("@/"))
                .map(|i| i.module.as_str())
                .collect();

            if local_imports.len() < 2 {
                continue;
            }

            let mut sorted = local_imports.clone();
            sorted.sort();
            let key = sorted.join(",");

            import_groups.entry(key).or_default().push(file);
        }

        for (import_key, group_files) in &import_groups {
            if group_files.len() < self.min_occurrences {
                continue;
            }

            let modules: Vec<&str> = import_key.split(',').collect();
            let short_modules: Vec<String> = modules
                .iter()
                .take(3)
                .map(|m| {
                    m.rsplit('/')
                        .next()
                        .unwrap_or(m)
                        .to_string()
                })
                .collect();

            let label = format!(
                "Files sharing imports: {} (×{})",
                short_modules.join(", "),
                group_files.len()
            );
            let id = format!("imports-{}", &blake3::hash(import_key.as_bytes()).to_hex()[..12]);

            results.push(DetectedPattern {
                id,
                label,
                category: "Emergent".into(),
                tier: "emergent".into(),
                confidence: 0.72,
                occurrences: group_files.len(),
                evidence: group_files
                    .iter()
                    .take(8)
                    .map(|f| PatternEvidence {
                        file: f.relative_path.clone(),
                        name: format!("{} shared imports", modules.len()),
                        line: 1,
                    })
                    .collect(),
            });
        }

        results
    }
}

// ═══════════════════════════════════════════════════════════════
// STRATEGY E: EXPORT SHAPE (Tier 3)
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn discover_export_shapes(&self, files: &[AnalyzedFile]) -> Vec<DetectedPattern> {
        let mut results = Vec::new();

        // Compute "export shape" = sorted set of export name prefixes
        let mut shape_groups: HashMap<String, Vec<&AnalyzedFile>> = HashMap::new();

        for file in files {
            if file.export_names.len() < 2 {
                continue;
            }

            let prefixes: Vec<String> = file
                .export_names
                .iter()
                .map(|name| extract_verb_prefix(name))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            if prefixes.len() < 2 {
                continue;
            }

            let mut sorted = prefixes;
            sorted.sort();
            let key = sorted.join(",");

            shape_groups.entry(key).or_default().push(file);
        }

        for (shape_key, group_files) in &shape_groups {
            if group_files.len() < self.min_occurrences {
                continue;
            }

            let verbs: Vec<&str> = shape_key.split(',').collect();
            let label = format!(
                "Shared API Shape: {} (×{})",
                verbs.join("/"),
                group_files.len()
            );
            let id = format!("export-{}", &blake3::hash(shape_key.as_bytes()).to_hex()[..12]);

            results.push(DetectedPattern {
                id,
                label,
                category: "Emergent".into(),
                tier: "emergent".into(),
                confidence: 0.68,
                occurrences: group_files.len(),
                evidence: group_files
                    .iter()
                    .take(8)
                    .map(|f| PatternEvidence {
                        file: f.relative_path.clone(),
                        name: f.export_names.join(", "),
                        line: 1,
                    })
                    .collect(),
            });
        }

        results
    }
}

fn extract_verb_prefix(name: &str) -> String {
    let prefixes = [
        "create", "build", "make", "get", "fetch", "find", "list",
        "update", "set", "patch", "delete", "remove", "destroy",
        "handle", "on", "use", "with", "is", "has", "can",
        "validate", "check", "parse", "transform", "map", "convert",
        "render", "draw", "show", "hide", "toggle",
        "init", "setup", "configure", "register", "subscribe",
        "send", "emit", "dispatch", "trigger", "notify",
    ];

    let lower = name.to_lowercase();
    for prefix in &prefixes {
        if lower.starts_with(prefix) {
            return prefix.to_string();
        }
    }

    // Return first word (split at uppercase boundary)
    let mut end = 1;
    for (i, c) in name.char_indices().skip(1) {
        if c.is_uppercase() {
            end = i;
            break;
        }
        end = i + c.len_utf8();
    }
    name[..end].to_lowercase()
}

// ═══════════════════════════════════════════════════════════════
// STRATEGY F: ERROR HANDLING FINGERPRINT (Tier 3)
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn discover_error_patterns(&self, files: &[AnalyzedFile]) -> Vec<DetectedPattern> {
        let all_fns: Vec<&FunctionInfo> = files.iter().flat_map(|f| &f.functions).collect();
        let total = all_fns.len();
        if total == 0 {
            return vec![];
        }

        let try_catch_count = all_fns.iter().filter(|f| f.has_try_catch).count();
        let guard_count = all_fns.iter().filter(|f| f.starts_with_guard).count();
        let _throw_count = all_fns.iter().filter(|f| f.has_throw).count();
        let result_count = all_fns.iter().filter(|f| f.returns_object_literal).count();

        let try_catch_pct = try_catch_count as f32 / total as f32;
        let _guard_pct = guard_count as f32 / total as f32;
        let result_pct = result_count as f32 / total as f32;

        let mut results = Vec::new();

        // Determine dominant error handling strategy
        if try_catch_pct > 0.3 && try_catch_pct > result_pct {
            results.push(DetectedPattern {
                id: "error-strategy-trycatch".into(),
                label: format!(
                    "Dominant Error Strategy: Try/Catch ({:.0}% of functions)",
                    try_catch_pct * 100.0
                ),
                category: "Emergent".into(),
                tier: "emergent".into(),
                confidence: try_catch_pct.min(0.95),
                occurrences: try_catch_count,
                evidence: vec![],
            });
        } else if result_pct > 0.15 {
            results.push(DetectedPattern {
                id: "error-strategy-result".into(),
                label: format!(
                    "Dominant Error Strategy: Result Type ({:.0}% of functions)",
                    result_pct * 100.0
                ),
                category: "Emergent".into(),
                tier: "emergent".into(),
                confidence: (result_pct * 2.0).min(0.90),
                occurrences: result_count,
                evidence: vec![],
            });
        }

        // Mixed error handling (potential inconsistency)
        if try_catch_pct > 0.15 && result_pct > 0.10 {
            results.push(DetectedPattern {
                id: "error-mixed".into(),
                label: format!(
                    "Mixed Error Handling: {:.0}% try/catch + {:.0}% result type",
                    try_catch_pct * 100.0,
                    result_pct * 100.0
                ),
                category: "Emergent".into(),
                tier: "emergent".into(),
                confidence: 0.80,
                occurrences: try_catch_count + result_count,
                evidence: vec![],
            });
        }

        results
    }
}

// ═══════════════════════════════════════════════════════════════
// STRATEGY G: STATEMENT SEQUENCE PATTERNS (Tier 3)
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn discover_sequence_patterns(&self, files: &[AnalyzedFile]) -> Vec<DetectedPattern> {
        let all_fns: Vec<&FunctionInfo> = files
            .iter()
            .flat_map(|f| &f.functions)
            .filter(|f| f.body_prefix.len() >= 2)
            .collect();

        if all_fns.len() < self.min_occurrences {
            return vec![];
        }

        // Group by body prefix sequence
        let mut sequence_groups: HashMap<String, Vec<&FunctionInfo>> = HashMap::new();
        for func in &all_fns {
            let key = func.body_prefix.iter().take(3).cloned().collect::<Vec<_>>().join("→");
            if key.len() >= 3 {
                sequence_groups.entry(key).or_default().push(func);
            }
        }

        let mut results = Vec::new();

        for (sequence, functions) in &sequence_groups {
            if functions.len() < self.min_occurrences {
                continue;
            }

            // Skip trivially common sequences
            if sequence == "declare" || sequence == "return" || sequence == "call" {
                continue;
            }

            let label = format!(
                "{} Sequence (×{})",
                sequence
                    .split('→')
                    .map(|s| capitalize(s))
                    .collect::<Vec<_>>()
                    .join(" → "),
                functions.len()
            );
            let id = format!("seq-{}", &blake3::hash(sequence.as_bytes()).to_hex()[..12]);

            results.push(DetectedPattern {
                id,
                label,
                category: "Emergent".into(),
                tier: "emergent".into(),
                confidence: compute_cluster_confidence(
                    functions.len(),
                    all_fns.len(),
                    sequence.matches('→').count() + 1,
                ),
                occurrences: functions.len(),
                evidence: functions
                    .iter()
                    .take(8)
                    .map(|f| PatternEvidence {
                        file: f.file.clone(),
                        name: f.name.clone(),
                        line: f.line,
                    })
                    .collect(),
            });
        }

        results
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}



// ═══════════════════════════════════════════════════════════════
// DEDUPLICATION — Remove overlapping/redundant patterns
// ═══════════════════════════════════════════════════════════════

impl PatternEngine {
    fn deduplicate(&self, patterns: Vec<DetectedPattern>) -> Vec<DetectedPattern> {
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut result = Vec::new();

        // Known and Framework patterns take priority over Emergent
        let tier_priority = |t: &str| -> u8 {
            match t {
                "known" => 0,
                "framework" => 1,
                "emergent" => 2,
                _ => 3,
            }
        };

        let mut sorted = patterns;
        sorted.sort_by(|a, b| tier_priority(&a.tier).cmp(&tier_priority(&b.tier)));

        for pattern in sorted {
            if seen_ids.contains(&pattern.id) {
                continue;
            }

            // Check if an existing pattern covers this one
            let dominated = result.iter().any(|existing: &DetectedPattern| {
                // If existing pattern has same evidence files and higher confidence
                if existing.confidence > pattern.confidence {
                    let existing_files: HashSet<&str> =
                        existing.evidence.iter().map(|e| e.file.as_str()).collect();
                    let new_files: HashSet<&str> =
                        pattern.evidence.iter().map(|e| e.file.as_str()).collect();
                    let overlap = existing_files.intersection(&new_files).count();
                    let overlap_ratio =
                        overlap as f32 / new_files.len().max(1) as f32;
                    overlap_ratio > 0.7
                } else {
                    false
                }
            });

            if !dominated {
                seen_ids.insert(pattern.id.clone());
                result.push(pattern);
            }
        }

        result
    }
}