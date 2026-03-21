//! OXC-powered semantic analysis for TypeScript/JavaScript files.
//!
//! This pass complements tree-sitter with compiler-grade import resolution and
//! precise call-target enrichment for the most important static cases:
//! - direct calls to imported symbols
//! - direct calls to locally declared functions
//! - namespace/member calls against imported modules

use std::collections::HashMap;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_resolver::{ResolveOptions, Resolver};
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType};
use serde::{Deserialize, Serialize};
use tracing::trace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedImport {
    pub specifier: String,
    pub resolved_path: Option<String>,
    pub is_type_only: bool,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedCall {
    pub caller_fn: String,
    pub callee_expr: String,
    pub line: u32,
    pub is_method: bool,
    pub callee_file: Option<String>,
    pub callee_symbol: Option<String>,
    pub callee_line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OxcAnalysis {
    pub file_path: String,
    pub resolved_imports: Vec<ResolvedImport>,
    pub calls: Vec<ResolvedCall>,
    pub scope_count: usize,
    pub reference_count: usize,
    pub declaration_count: usize,
}

#[derive(Debug, Clone)]
enum ImportBindingKind {
    Named { imported_name: String },
    Default,
    Namespace,
}

#[derive(Debug, Clone)]
struct ImportBinding {
    resolved_path: String,
    kind: ImportBindingKind,
}

pub fn is_oxc_supported(ext: &str) -> bool {
    matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
}

pub fn analyze_file(
    file_path: &Path,
    source_code: &str,
    workspace_root: Option<&Path>,
) -> Option<OxcAnalysis> {
    let ext = file_path.extension()?.to_str()?;
    if !is_oxc_supported(ext) {
        return None;
    }

    let source_type = SourceType::from_path(file_path).ok()?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source_code, source_type).parse();

    if parsed.panicked || !parsed.errors.is_empty() {
        trace!("OXC parse errors in {:?}, falling back to tree-sitter-only semantics", file_path);
    }

    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let stats = semantic.stats();

    let resolver = build_resolver();
    let module_index = build_module_index(&parsed.program, file_path, workspace_root, &resolver);
    let local_symbols = collect_local_symbols(&parsed.program, source_code);
    let calls = extract_calls_from_ast(
        &parsed.program,
        file_path,
        source_code,
        &module_index.bindings,
        &local_symbols,
    );

    Some(OxcAnalysis {
        file_path: file_path.to_string_lossy().to_string(),
        resolved_imports: module_index.imports,
        calls,
        scope_count: semantic.scoping().scopes_len(),
        reference_count: stats.references as usize,
        declaration_count: semantic.scoping().symbols_len(),
    })
}

fn build_resolver() -> Resolver {
    Resolver::new(ResolveOptions {
        extensions: vec![
            ".ts".to_string(),
            ".tsx".to_string(),
            ".js".to_string(),
            ".jsx".to_string(),
            ".mjs".to_string(),
            ".json".to_string(),
            ".d.ts".to_string(),
        ],
        main_fields: vec!["module".to_string(), "main".to_string()],
        condition_names: vec![
            "import".to_string(),
            "require".to_string(),
            "default".to_string(),
        ],
        ..Default::default()
    })
}

#[derive(Default)]
struct ModuleIndex {
    imports: Vec<ResolvedImport>,
    bindings: HashMap<String, ImportBinding>,
}

fn build_module_index(
    program: &oxc_ast::ast::Program<'_>,
    file_path: &Path,
    workspace_root: Option<&Path>,
    resolver: &Resolver,
) -> ModuleIndex {
    let mut index = ModuleIndex::default();
    let allow_node_modules =
        std::env::var("MEMIX_OXC_RESOLVE_NODE_MODULES").unwrap_or_else(|_| "false".to_string()) == "true";

    for stmt in &program.body {
        use oxc_ast::ast::{ImportDeclarationSpecifier, ImportOrExportKind, Statement};

        let Statement::ImportDeclaration(import_decl) = stmt else {
            continue;
        };

        let specifier = import_decl.source.value.to_string();
        let resolved_path = resolve_import_path(file_path, workspace_root, resolver, &specifier)
            .filter(|path| allow_node_modules || !path.contains("node_modules"));
        let is_type_only = import_decl.import_kind == ImportOrExportKind::Type;
        let mut symbols = Vec::new();

        if let Some(specifiers) = &import_decl.specifiers {
            for spec in specifiers {
                match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(named) => {
                        let imported_name = named.imported.name().to_string();
                        let local_name = named.local.name.to_string();
                        symbols.push(local_name.clone());
                        if let Some(path) = &resolved_path {
                            index.bindings.insert(
                                local_name.clone(),
                                ImportBinding {
                                    resolved_path: path.clone(),
                                    kind: ImportBindingKind::Named { imported_name },
                                },
                            );
                        }
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(default_spec) => {
                        let local_name = default_spec.local.name.to_string();
                        symbols.push(local_name.clone());
                        if let Some(path) = &resolved_path {
                            index.bindings.insert(
                                local_name.clone(),
                                ImportBinding {
                                    resolved_path: path.clone(),
                                    kind: ImportBindingKind::Default,
                                },
                            );
                        }
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace_spec) => {
                        let local_name = namespace_spec.local.name.to_string();
                        symbols.push(format!("* as {}", local_name));
                        if let Some(path) = &resolved_path {
                            index.bindings.insert(
                                local_name.clone(),
                                ImportBinding {
                                    resolved_path: path.clone(),
                                    kind: ImportBindingKind::Namespace,
                                },
                            );
                        }
                    }
                }
            }
        }

        index.imports.push(ResolvedImport {
            specifier,
            resolved_path,
            is_type_only,
            symbols,
        });
    }

    index
}

fn resolve_import_path(
    file_path: &Path,
    workspace_root: Option<&Path>,
    resolver: &Resolver,
    specifier: &str,
) -> Option<String> {
    resolver
        .resolve_file(file_path, specifier)
        .or_else(|_| {
            let base = workspace_root
                .or_else(|| file_path.parent())
                .unwrap_or_else(|| Path::new("."));
            resolver.resolve(base, specifier)
        })
        .ok()
        .map(|resolution| resolution.path().to_string_lossy().to_string())
}

fn collect_local_symbols(
    program: &oxc_ast::ast::Program<'_>,
    source_code: &str,
) -> HashMap<String, u32> {
    let mut symbols = HashMap::new();

    for stmt in &program.body {
        collect_symbols_from_statement(stmt, source_code, &mut symbols);
    }

    symbols
}

fn collect_symbols_from_statement(
    stmt: &oxc_ast::ast::Statement<'_>,
    source_code: &str,
    symbols: &mut HashMap<String, u32>,
) {
    use oxc_ast::ast::Statement;

    match stmt {
        Statement::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                symbols.insert(id.name.to_string(), line_from_span(source_code, function.span));
            }
            if let Some(body) = &function.body {
                for stmt in &body.statements {
                    collect_symbols_from_statement(stmt, source_code, symbols);
                }
            }
        }
        Statement::BlockStatement(block) => {
            for stmt in &block.body {
                collect_symbols_from_statement(stmt, source_code, symbols);
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_symbols_from_statement(&if_stmt.consequent, source_code, symbols);
            if let Some(alternate) = &if_stmt.alternate {
                collect_symbols_from_statement(alternate, source_code, symbols);
            }
        }
        _ => {}
    }
}

fn extract_calls_from_ast(
    program: &oxc_ast::ast::Program<'_>,
    file_path: &Path,
    source_code: &str,
    import_bindings: &HashMap<String, ImportBinding>,
    local_symbols: &HashMap<String, u32>,
) -> Vec<ResolvedCall> {
    let mut calls = Vec::new();
    for stmt in &program.body {
        collect_calls_from_statement(
            stmt,
            file_path,
            source_code,
            import_bindings,
            local_symbols,
            &mut calls,
            None,
        );
    }
    calls
}

fn collect_calls_from_statement(
    stmt: &oxc_ast::ast::Statement<'_>,
    file_path: &Path,
    source_code: &str,
    import_bindings: &HashMap<String, ImportBinding>,
    local_symbols: &HashMap<String, u32>,
    calls: &mut Vec<ResolvedCall>,
    current_fn: Option<&str>,
) {
    use oxc_ast::ast::Statement;

    match stmt {
        Statement::FunctionDeclaration(function) => {
            let fn_name = function
                .id
                .as_ref()
                .map(|id| id.name.to_string())
                .unwrap_or_else(|| "<anonymous>".to_string());
            if let Some(body) = &function.body {
                for stmt in &body.statements {
                    collect_calls_from_statement(
                        stmt,
                        file_path,
                        source_code,
                        import_bindings,
                        local_symbols,
                        calls,
                        Some(&fn_name),
                    );
                }
            }
        }
        Statement::BlockStatement(block) => {
            for stmt in &block.body {
                collect_calls_from_statement(
                    stmt,
                    file_path,
                    source_code,
                    import_bindings,
                    local_symbols,
                    calls,
                    current_fn,
                );
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            collect_calls_from_expr(
                &expr_stmt.expression,
                file_path,
                source_code,
                import_bindings,
                local_symbols,
                calls,
                current_fn,
            );
        }
        Statement::ReturnStatement(ret) => {
            if let Some(argument) = &ret.argument {
                collect_calls_from_expr(
                    argument,
                    file_path,
                    source_code,
                    import_bindings,
                    local_symbols,
                    calls,
                    current_fn,
                );
            }
        }
        Statement::VariableDeclaration(var_decl) => {
            for decl in &var_decl.declarations {
                if let Some(init) = &decl.init {
                    collect_calls_from_expr(
                        init,
                        file_path,
                        source_code,
                        import_bindings,
                        local_symbols,
                        calls,
                        current_fn,
                    );
                }
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_calls_from_expr(
                &if_stmt.test,
                file_path,
                source_code,
                import_bindings,
                local_symbols,
                calls,
                current_fn,
            );
            collect_calls_from_statement(
                &if_stmt.consequent,
                file_path,
                source_code,
                import_bindings,
                local_symbols,
                calls,
                current_fn,
            );
            if let Some(alternate) = &if_stmt.alternate {
                collect_calls_from_statement(
                    alternate,
                    file_path,
                    source_code,
                    import_bindings,
                    local_symbols,
                    calls,
                    current_fn,
                );
            }
        }
        _ => {}
    }
}

fn collect_calls_from_expr(
    expr: &oxc_ast::ast::Expression<'_>,
    file_path: &Path,
    source_code: &str,
    import_bindings: &HashMap<String, ImportBinding>,
    local_symbols: &HashMap<String, u32>,
    calls: &mut Vec<ResolvedCall>,
    current_fn: Option<&str>,
) {
    use oxc_ast::ast::Expression;

    match expr {
        Expression::CallExpression(call) => {
            let (callee_expr, is_method, callee_file, callee_symbol, callee_line) =
                resolve_call_target(
                    &call.callee,
                    file_path,
                    source_code,
                    import_bindings,
                    local_symbols,
                );

            calls.push(ResolvedCall {
                caller_fn: current_fn.unwrap_or("<top-level>").to_string(),
                callee_expr,
                line: line_from_span(source_code, call.span),
                is_method,
                callee_file,
                callee_symbol,
                callee_line,
            });

            collect_calls_from_expr(
                &call.callee,
                file_path,
                source_code,
                import_bindings,
                local_symbols,
                calls,
                current_fn,
            );

            for arg in &call.arguments {
                match arg {
                    oxc_ast::ast::Argument::SpreadElement(spread) => collect_calls_from_expr(
                        &spread.argument,
                        file_path,
                        source_code,
                        import_bindings,
                        local_symbols,
                        calls,
                        current_fn,
                    ),
                    _ if arg.as_expression().is_some() => {
                        let expression = arg.as_expression().expect("checked above");
                        collect_calls_from_expr(
                            expression,
                            file_path,
                            source_code,
                            import_bindings,
                            local_symbols,
                            calls,
                            current_fn,
                        )
                    }
                    _ => {}
                }
            }
        }
        Expression::ArrowFunctionExpression(arrow) => {
            for stmt in &arrow.body.statements {
                collect_calls_from_statement(
                    stmt,
                    file_path,
                    source_code,
                    import_bindings,
                    local_symbols,
                    calls,
                    current_fn,
                );
            }
        }
        Expression::ConditionalExpression(cond) => {
            collect_calls_from_expr(
                &cond.test,
                file_path,
                source_code,
                import_bindings,
                local_symbols,
                calls,
                current_fn,
            );
            collect_calls_from_expr(
                &cond.consequent,
                file_path,
                source_code,
                import_bindings,
                local_symbols,
                calls,
                current_fn,
            );
            collect_calls_from_expr(
                &cond.alternate,
                file_path,
                source_code,
                import_bindings,
                local_symbols,
                calls,
                current_fn,
            );
        }
        _ => {}
    }
}

fn resolve_call_target(
    callee: &oxc_ast::ast::Expression<'_>,
    file_path: &Path,
    source_code: &str,
    import_bindings: &HashMap<String, ImportBinding>,
    local_symbols: &HashMap<String, u32>,
) -> (String, bool, Option<String>, Option<String>, Option<u32>) {
    use oxc_ast::ast::Expression;

    match callee {
        Expression::Identifier(ident) => {
            let name = ident.name.to_string();
            if let Some(binding) = import_bindings.get(&name) {
                let imported_symbol = match &binding.kind {
                    ImportBindingKind::Named { imported_name } => imported_name.clone(),
                    ImportBindingKind::Default => "default".to_string(),
                    ImportBindingKind::Namespace => name.clone(),
                };
                (
                    name.clone(),
                    false,
                    Some(binding.resolved_path.clone()),
                    Some(imported_symbol.clone()),
                    resolve_symbol_line(&binding.resolved_path, &imported_symbol),
                )
            } else if let Some(line) = local_symbols.get(&name) {
                (
                    name.clone(),
                    false,
                    Some(file_path.to_string_lossy().to_string()),
                    Some(name),
                    Some(*line),
                )
            } else {
                (name, false, None, None, None)
            }
        }
        Expression::StaticMemberExpression(member) => {
            let object_name = extract_expr_name(&member.object, source_code);
            let property_name = member.property.name.to_string();
            if let Some(binding) = import_bindings.get(&object_name) {
                (
                    format!("{}.{}", object_name, property_name),
                    true,
                    Some(binding.resolved_path.clone()),
                    Some(property_name.clone()),
                    resolve_symbol_line(&binding.resolved_path, &property_name),
                )
            } else {
                (
                    format!("{}.{}", object_name, property_name),
                    true,
                    None,
                    None,
                    None,
                )
            }
        }
        _ => {
            let start = callee.span().start as usize;
            let end = callee.span().end as usize;
            let text = source_code[start..end.min(source_code.len())]
                .chars()
                .take(64)
                .collect::<String>();
            (text, false, None, None, None)
        }
    }
}

fn resolve_symbol_line(file_path: &str, symbol: &str) -> Option<u32> {
    if symbol == "default" || symbol == "*" {
        return None;
    }

    let path = Path::new(file_path);
    let ext = path.extension().and_then(|ext| ext.to_str())?;
    if !is_oxc_supported(ext) {
        return None;
    }

    let source = std::fs::read_to_string(path).ok()?;
    let source_type = SourceType::from_path(path).ok()?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, source_type).parse();

    for stmt in &parsed.program.body {
        use oxc_ast::ast::Statement;
        if let Statement::FunctionDeclaration(function) = stmt {
            if function
                .id
                .as_ref()
                .map(|id| id.name.as_str() == symbol)
                .unwrap_or(false)
            {
                return Some(line_from_span(&source, function.span));
            }
        }
    }

    None
}

fn line_from_span(source_code: &str, span: oxc_span::Span) -> u32 {
    source_code[..span.start as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count() as u32
        + 1
}

fn extract_expr_name(expr: &oxc_ast::ast::Expression<'_>, source_code: &str) -> String {
    use oxc_ast::ast::Expression;

    match expr {
        Expression::Identifier(ident) => ident.name.to_string(),
        Expression::ThisExpression(_) => "this".to_string(),
        _ => {
            let start = expr.span().start as usize;
            let end = expr.span().end as usize;
            source_code[start..end.min(source_code.len())]
                .chars()
                .take(32)
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_is_oxc_supported() {
        assert!(is_oxc_supported("ts"));
        assert!(is_oxc_supported("tsx"));
        assert!(is_oxc_supported("js"));
        assert!(is_oxc_supported("jsx"));
        assert!(is_oxc_supported("mjs"));
        assert!(!is_oxc_supported("rs"));
        assert!(!is_oxc_supported("py"));
    }

    #[test]
    fn test_basic_analysis_extracts_imports_and_calls() {
        let source = r#"
import { validate } from './validator';

function App() {
    validate(42);
    return 42;
}
"#;
        let path = PathBuf::from("test.ts");
        let result = analyze_file(&path, source, None).expect("analysis");
        assert_eq!(result.resolved_imports.len(), 1);
        assert_eq!(result.resolved_imports[0].specifier, "./validator");
        assert_eq!(result.resolved_imports[0].symbols, vec!["validate"]);
        assert_eq!(result.calls.len(), 1);
        assert_eq!(result.calls[0].caller_fn, "App");
        assert_eq!(result.calls[0].callee_expr, "validate");
    }

    #[test]
    fn test_unsupported_extension_returns_none() {
        let path = PathBuf::from("test.rs");
        assert!(analyze_file(&path, "fn main() {}", None).is_none());
    }
}
