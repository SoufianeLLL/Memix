use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::warn;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AstNodeFeature {
    pub name: String,
    pub kind: String,
    pub body: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub language: String,
    pub cyclomatic_complexity: u32,
    pub pattern_tags: Vec<String>,
    pub is_exported: bool,
}

pub struct AstParser {
    parser: Parser,
}

impl AstParser {
    pub fn new() -> Result<Self> {
        let parser = Parser::new();
        Ok(Self { parser })
    }

    pub fn parse_file(&mut self, file_path: &Path) -> Result<Option<(Tree, Language)>> {
        let content =
            fs::read_to_string(file_path).context("Failed to read file for AST parsing")?;
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let language = match ext {
            "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
            "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
            "rs" => tree_sitter_rust::LANGUAGE.into(),
            "py" => tree_sitter_python::LANGUAGE.into(),
            "go" => tree_sitter_go::LANGUAGE.into(),
            "java" => tree_sitter_java::LANGUAGE.into(),
            "cpp" | "cc" | "cxx" | "c" | "h" | "hpp" => tree_sitter_cpp::LANGUAGE.into(),
            "cs" => tree_sitter_c_sharp::LANGUAGE.into(),
            "rb" => tree_sitter_ruby::LANGUAGE.into(),
            "swift" => tree_sitter_swift::LANGUAGE.into(),
            "kt" | "kts" => tree_sitter_kotlin_ng::LANGUAGE.into(),
            "php" => tree_sitter_php::LANGUAGE_PHP.into(),
            _ => {
                warn!("Unsupported file extension for AST: {}", ext);
                return Ok(None);
            }
        };

        self.parser
            .set_language(&language)
            .context("Failed to load tree-sitter language bindings")?;

        let tree = self.parser.parse(&content, None);
        Ok(tree.map(|t| (t, language)))
    }

    pub fn extract_features(
        &self,
        tree: &Tree,
        language: Language,
        source_code: &[u8],
        extension: &str,
    ) -> Vec<AstNodeFeature> {
        let mut features = Vec::new();
        let Some(query_string) = entity_query_for_extension(extension) else {
            return features;
        };

        let query = match Query::new(&language, query_string) {
            Ok(q) => q,
            Err(e) => {
                warn!("Failed to compile tree-sitter entity query: {}", e);
                return features;
            }
        };

        let mut query_cursor = QueryCursor::new();
        let name_idx = query.capture_index_for_name("name").unwrap_or(0);
        let entity_idx = query.capture_index_for_name("entity").unwrap_or(0);

        for match_ in query_cursor.matches(&query, tree.root_node(), source_code) {
            let mut name = String::new();
            let mut entity_node = None;

            for capture in match_.captures {
                if capture.index == name_idx {
                    name = slice_text(source_code, capture.node.start_byte(), capture.node.end_byte())
                        .trim()
                        .to_string();
                } else if capture.index == entity_idx {
                    entity_node = Some(capture.node);
                }
            }

            let Some(entity_node) = entity_node else {
                continue;
            };

            let body = slice_text(source_code, entity_node.start_byte(), entity_node.end_byte());
            if body.trim().is_empty() {
                continue;
            }

            let kind = canonical_entity_kind(entity_node.kind());
            if name.is_empty() {
                name = infer_entity_name(&kind, entity_node.start_byte());
            }

            let cyclomatic_complexity = compute_cyclomatic_complexity(entity_node, extension, source_code);
            let is_exported = detect_is_exported(entity_node, extension, source_code);
            let mut pattern_tags = detect_pattern_tags(&name, &kind, extension, is_exported, source_code);
            pattern_tags.sort();
            pattern_tags.dedup();

            features.push(AstNodeFeature {
                name,
                kind,
                body,
                start_byte: entity_node.start_byte(),
                end_byte: entity_node.end_byte(),
                language: language_key(extension).to_string(),
                cyclomatic_complexity,
                pattern_tags,
                is_exported,
            });
        }

        features.sort_by(|a, b| a.start_byte.cmp(&b.start_byte).then_with(|| a.name.cmp(&b.name)));
        features.dedup_by(|a, b| a.start_byte == b.start_byte && a.end_byte == b.end_byte && a.name == b.name);
        features
    }

    /// Get all supported file extensions
    pub fn supported_extensions() -> Vec<&'static str> {
        vec![
            "ts", "tsx", "js", "jsx", "mjs", "cjs",
            "rs",
            "py",
            "go",
            "java",
            "cpp", "cc", "cxx", "c", "h", "hpp",
            "cs",
            "rb",
            "swift",
            "kt", "kts",
            "php",
        ]
    }

    /// Check if a file extension is supported
    pub fn is_supported(extension: &str) -> bool {
        Self::supported_extensions().contains(&extension)
    }
}

fn entity_query_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "ts" | "tsx" => Some(
            "(function_declaration name: (identifier) @name) @entity
             (method_definition name: (property_identifier) @name) @entity
             (class_declaration name: (type_identifier) @name) @entity
             (lexical_declaration (variable_declarator name: (identifier) @name value: [(arrow_function) (function_expression)] @entity))
             (variable_declaration (variable_declarator name: (identifier) @name value: [(arrow_function) (function_expression)] @entity))",
        ),
        "js" | "jsx" | "mjs" | "cjs" => Some(
            "(function_declaration name: (identifier) @name) @entity
             (method_definition name: (property_identifier) @name) @entity
             (class_declaration name: (identifier) @name) @entity
             (lexical_declaration (variable_declarator name: (identifier) @name value: [(arrow_function) (function_expression)] @entity))
             (variable_declaration (variable_declarator name: (identifier) @name value: [(arrow_function) (function_expression)] @entity))",
        ),
        "rs" => Some(
            "(function_item name: (identifier) @name) @entity
             (struct_item name: (type_identifier) @name) @entity
             (enum_item name: (type_identifier) @name) @entity
             (trait_item name: (type_identifier) @name) @entity
             (impl_item type: (type_identifier) @name) @entity",
        ),
        "py" => Some(
            "(function_definition name: (identifier) @name) @entity
             (class_definition name: (identifier) @name) @entity",
        ),
        "go" => Some(
            "(function_declaration name: (identifier) @name) @entity
             (method_declaration name: (field_identifier) @name) @entity
             (method_declaration name: (identifier) @name) @entity
             (type_spec name: (type_identifier) @name) @entity",
        ),
        "java" => Some(
            "(method_declaration name: (identifier) @name) @entity
             (class_declaration name: (identifier) @name) @entity
             (interface_declaration name: (identifier) @name) @entity",
        ),
        "cpp" | "cc" | "cxx" | "c" | "h" | "hpp" => Some(
            "(function_definition declarator: [(function_declarator declarator: (identifier) @name) (identifier) @name]) @entity
             (class_specifier name: (type_identifier) @name) @entity
             (struct_specifier name: (type_identifier) @name) @entity",
        ),
        "cs" => Some(
            "(method_declaration name: (identifier) @name) @entity
             (class_declaration name: (identifier) @name) @entity
             (interface_declaration name: (identifier) @name) @entity
             (struct_declaration name: (identifier) @name) @entity
             (enum_declaration name: (identifier) @name) @entity",
        ),
        "rb" => Some(
            "(method name: (identifier) @name) @entity
             (class name: [(constant) (scope_resolution)] @name) @entity
             (module name: [(constant) (scope_resolution)] @name) @entity",
        ),
        "swift" => Some(
            "(function_declaration (simple_identifier) @name) @entity
             (class_declaration (type_identifier) @name) @entity
             (protocol_declaration (type_identifier) @name) @entity
             (struct_declaration (type_identifier) @name) @entity
             (enum_declaration (type_identifier) @name) @entity",
        ),
        "kt" | "kts" => Some(
            "(function_declaration (simple_identifier) @name) @entity
             (class_declaration (type_identifier) @name) @entity
             (object_declaration (type_identifier) @name) @entity",
        ),
        "php" => Some(
            "(function_definition name: (name) @name) @entity
             (method_declaration name: (name) @name) @entity
             (class_declaration name: (name) @name) @entity
             (interface_declaration name: (name) @name) @entity
             (trait_declaration name: (name) @name) @entity",
        ),
        _ => None,
    }
}

fn language_key(extension: &str) -> &'static str {
    match extension {
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "cpp" | "cc" | "cxx" | "c" | "h" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "php" => "php",
        _ => "unknown",
    }
}

fn canonical_entity_kind(kind: &str) -> String {
    match kind {
        "function_declaration" | "method_definition" | "function_item" | "function_definition" | "function_definition_item"
        | "method_declaration" | "arrow_function" | "function_expression" => "function".to_string(),
        "class_declaration" | "class_definition" | "class_specifier" | "struct_item" | "struct_specifier" => "class".to_string(),
        "interface_declaration" | "trait_item" => "interface".to_string(),
        "enum_item" | "type_spec" | "impl_item" => "type".to_string(),
        _ => kind.to_string(),
    }
}

fn infer_entity_name(kind: &str, start_byte: usize) -> String {
    format!("{}_at_{}", kind, start_byte)
}

fn slice_text(source_code: &[u8], start_byte: usize, end_byte: usize) -> String {
    std::str::from_utf8(&source_code[start_byte..end_byte])
        .unwrap_or("")
        .to_string()
}

fn detect_is_exported(entity_node: Node<'_>, extension: &str, source_code: &[u8]) -> bool {
    match extension {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            let mut current = Some(entity_node);
            while let Some(node) = current {
                if matches!(node.kind(), "export_statement" | "export_clause") {
                    return true;
                }
                current = node.parent();
            }
            let body = slice_text(source_code, entity_node.start_byte(), entity_node.end_byte());
            body.trim_start().starts_with("export ")
        }
        "rs" => slice_text(source_code, entity_node.start_byte(), entity_node.end_byte())
            .trim_start()
            .starts_with("pub "),
        "java" | "cs" | "kt" | "kts" => slice_text(source_code, entity_node.start_byte(), entity_node.end_byte())
            .trim_start()
            .starts_with("public "),
        "swift" => slice_text(source_code, entity_node.start_byte(), entity_node.end_byte())
            .trim_start()
            .starts_with("public "),
        "php" => {
            let body = slice_text(source_code, entity_node.start_byte(), entity_node.end_byte());
            let trimmed = body.trim_start();
            trimmed.starts_with("public ") || trimmed.starts_with("function ")
        }
        _ => false,
    }
}

fn detect_pattern_tags(
    name: &str,
    kind: &str,
    extension: &str,
    is_exported: bool,
    source_code: &[u8],
) -> Vec<String> {
    let mut tags = Vec::new();
    let lower_name = name.to_lowercase();

    let shared_patterns = [
        ("repository", "repository"),
        ("service", "service"),
        ("controller", "controller"),
        ("middleware", "edge-guards"),
        ("guard", "edge-guards"),
        ("interceptor", "edge-guards"),
        ("webhook", "edge-guards"),
        ("adapter", "adapter"),
        ("facade", "facade"),
        ("builder", "builder"),
    ];
    for (needle, tag) in shared_patterns {
        if lower_name.contains(needle) {
            tags.push(tag.to_string());
        }
    }

    if matches!(extension, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") {
        if kind == "function" && name.starts_with("use") && name.chars().nth(3).map(|c| c.is_uppercase()).unwrap_or(false) {
            tags.push("react-hook".to_string());
        }
        if kind == "class" || (kind == "function" && name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)) {
            tags.push("component-driven".to_string());
        }
        let file_head = std::str::from_utf8(&source_code[..source_code.len().min(256)]).unwrap_or("");
        if is_exported && file_head.contains("use server") {
            tags.push("server-actions".to_string());
        }
    }

    if matches!(extension, "py") && kind == "function" && lower_name.starts_with("test_") {
        tags.push("tests".to_string());
    }

    if matches!(extension, "rs") && (lower_name == "new" || lower_name.starts_with("build")) {
        tags.push("builder".to_string());
    }

    tags
}

fn compute_cyclomatic_complexity(entity_node: Node<'_>, extension: &str, source_code: &[u8]) -> u32 {
    let mut complexity = 1u32;
    let mut stack = vec![entity_node];

    while let Some(node) = stack.pop() {
        complexity = complexity.saturating_add(cyclomatic_weight(node, extension, source_code));
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    complexity.max(1)
}

fn cyclomatic_weight(node: Node<'_>, extension: &str, source_code: &[u8]) -> u32 {
    let kind = node.kind();
    let is_decision = match extension {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => matches!(
            kind,
            "if_statement" | "for_statement" | "for_in_statement" | "while_statement" | "do_statement"
                | "switch_case" | "catch_clause" | "ternary_expression" | "conditional_expression"
        ),
        "rs" => matches!(
            kind,
            "if_expression" | "if_let_expression" | "for_expression" | "while_expression" | "loop_expression"
                | "match_expression"
        ),
        "py" => matches!(
            kind,
            "if_statement" | "elif_clause" | "for_statement" | "while_statement" | "except_clause"
                | "conditional_expression"
        ),
        "go" => matches!(
            kind,
            "if_statement" | "for_statement" | "expression_switch_statement" | "type_switch_statement"
                | "case_clause" | "select_statement" | "communication_case"
        ),
        "java" => matches!(
            kind,
            "if_statement" | "for_statement" | "enhanced_for_statement" | "while_statement" | "do_statement"
                | "catch_clause" | "switch_block_statement_group" | "conditional_expression"
        ),
        "cpp" | "cc" | "cxx" | "c" | "h" | "hpp" => matches!(
            kind,
            "if_statement" | "for_statement" | "while_statement" | "do_statement" | "case_statement"
                | "catch_clause" | "conditional_expression"
        ),
        "cs" => matches!(
            kind,
            "if_statement" | "for_statement" | "for_each_statement" | "while_statement" | "do_statement"
                | "catch_clause" | "switch_section" | "conditional_expression"
        ),
        "rb" => matches!(
            kind,
            "if" | "unless" | "while" | "until" | "for" | "case" | "when"
                | "rescue" | "conditional"
        ),
        "swift" => matches!(
            kind,
            "if_statement" | "guard_statement" | "for_statement" | "while_statement"
                | "switch_entry" | "catch_clause" | "ternary_expression"
        ),
        "kt" | "kts" => matches!(
            kind,
            "if_expression" | "when_expression" | "for_statement" | "while_statement"
                | "do_while_statement" | "catch_block" | "when_entry"
        ),
        "php" => matches!(
            kind,
            "if_statement" | "for_statement" | "foreach_statement" | "while_statement"
                | "do_statement" | "switch_case" | "catch_clause" | "conditional_expression"
        ),
        _ => false,
    };
    if is_decision {
        return 1;
    }

    if matches!(kind, "binary_expression" | "logical_expression") {
        let body = slice_text(source_code, node.start_byte(), node.end_byte());
        return body.matches("&&").count().saturating_add(body.matches("||").count()) as u32;
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_ast_metrics_and_patterns_from_typescript() {
        let mut parser = AstParser::new().unwrap();
        let language: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.parser.set_language(&language).unwrap();

        let source = br#"'use server';
 export function useInventory(items: number[]) {
     if (items.length > 0 && items[0] > 1) {
         return items[0] > 10 ? items[0] : 10;
     }
     return 0;
 }"#;
        let tree = parser.parser.parse(source.as_slice(), None).unwrap();

        let features = parser.extract_features(&tree, language, source, "ts");
        assert_eq!(features.len(), 1);
        let feature = &features[0];
        assert_eq!(feature.name, "useInventory");
        assert!(feature.cyclomatic_complexity >= 4);
        assert!(feature.is_exported);
        assert!(feature.pattern_tags.iter().any(|tag| tag == "react-hook"));
        assert!(feature.pattern_tags.iter().any(|tag| tag == "server-actions"));
    }
}
