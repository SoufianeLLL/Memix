pub mod compiler;
pub mod hierarchy;

pub use compiler::{BudgetCompiledContext, BudgetContextCompiler, CompilerConfig};
pub use hierarchy::{ContextLayer, HierarchyResolver, ResolvedContext};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::brain::schema::MemoryEntry;
use crate::observer::graph::DependencyGraph;
use crate::constants::GENERATED_DIRS;
use crate::observer::parser::{AstNodeFeature, AstParser};
use crate::recorder::flight::{FlightRecord, SessionEvent};
use crate::token::engine::TokenEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileRequest {
    pub project_id: String,
    pub active_file: String,
    pub token_budget: usize,
    pub task_type: Option<String>,
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledSection {
    pub id: String,
    pub kind: String,
    pub priority: u8,
    pub tokens: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilePassMetrics {
    pub relevant_files: usize,
    pub skeletons_built: usize,
    pub skeleton_index_sections: usize,
    pub deduplicated_files: usize,
    pub history_sections: usize,
    pub rules_sections: usize,
    pub ranked_sections: usize,
    pub fitted_sections: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledContext {
    pub budget: usize,
    pub total_tokens: usize,
    pub naive_token_estimate: u64,
    pub explainability_summary: String,
    pub selected_sections: Vec<CompiledSection>,
    pub omitted_section_ids: Vec<String>,
    pub metrics: CompilePassMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskType {
    NewFeature,
    BugFix,
    Refactor,
    CodeReview,
    Unknown,
}

#[derive(Debug, Clone)]
struct RelevantFile {
    path: String,
    depth: usize,
}

#[derive(Debug, Clone)]
struct CodeSkeleton {
    path: String,
    signatures: Vec<String>,
    types: Vec<String>,
    exports: Vec<String>,
    full_functions: Vec<String>,
}

pub struct ContextCompiler {
    workspace_root: Option<PathBuf>,
}

impl ContextCompiler {
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        Self { workspace_root }
    }

    pub fn compile(
        &self,
        request: CompileRequest,
        graph: &DependencyGraph,
        history: &[FlightRecord],
        brain_entries: &[MemoryEntry],
        skeleton_entries: &[MemoryEntry],
        causal_context: Option<String>,
    ) -> Result<CompiledContext> {
        if request.token_budget == 0 {
            return Ok(CompiledContext {
                budget: 0,
                total_tokens: 0,
                naive_token_estimate: 0,
                explainability_summary: "Zero token budget requested".to_string(),
                selected_sections: vec![],
                omitted_section_ids: vec![],
                metrics: CompilePassMetrics {
                    relevant_files: 0,
                    skeletons_built: 0,
                    skeleton_index_sections: 0,
                    deduplicated_files: 0,
                    history_sections: 0,
                    rules_sections: 0,
                    ranked_sections: 0,
                    fitted_sections: 0,
                },
            });
        }

        let task_type = parse_task_type(request.task_type.as_deref());
        let relevant_files = self.pass_dead_context_elimination(
            &request.active_file,
            graph,
            request.max_depth.unwrap_or(2),
        );

        let naive_estimate = if relevant_files.is_empty() {
            // Fallback: estimate from skeleton entry sizes when no relevant files found
            skeleton_entries.iter()
                .filter_map(|e| {
                    // Estimate ~25% of content length as token count
                    let content_len = e.content.len() as f64;
                    Some((content_len * 0.25) as u64)
                })
                .sum()
        } else {
            relevant_files.iter()
                .filter_map(|rf| std::fs::metadata(&rf.path).ok())
                .map(|meta| {
                    (meta.len() as f64 * 0.25) as u64
                })
                .sum::<u64>()
        };

        let skeletons = self.pass_skeleton_extraction(&request.active_file, &relevant_files)?;
        let (deduplicated_skeletons, deduplicated_files) =
            self.pass_brain_dedup(skeletons, brain_entries);
        let history_sections = self.pass_history_compaction(history);
        let rules_sections = self.pass_rules_pruning(&task_type, request.query.as_deref());
        let ranked_sections = self.pass_priority_ranking(
            &request.active_file,
            graph,
            &task_type,
            deduplicated_skeletons,
            history_sections,
            rules_sections,
            skeleton_entries,
            causal_context,
            request.query.as_deref(),
        )?;
        let ranked_sections_count = ranked_sections.len();
        let (selected_sections, omitted_section_ids, total_tokens) =
            self.pass_budget_fitting(ranked_sections, request.token_budget)?;
        let metrics = CompilePassMetrics {
            relevant_files: relevant_files.len(),
            skeletons_built: selected_sections
                .iter()
                .filter(|s| s.kind == "Skeleton")
                .count(),
            skeleton_index_sections: selected_sections
                .iter()
                .filter(|s| s.kind == "SkeletonIndex")
                .count(),
            deduplicated_files,
            history_sections: selected_sections
                .iter()
                .filter(|s| s.kind == "history")
                .count(),
            rules_sections: selected_sections
                .iter()
                .filter(|s| s.kind == "rules")
                .count(),
            ranked_sections: ranked_sections_count,
            fitted_sections: selected_sections.len(),
        };
        Ok(CompiledContext {
            budget: request.token_budget,
            total_tokens,
            naive_token_estimate: naive_estimate,
            explainability_summary: format!(
                "Compiled {} context sections from {} relevant files for {:?} under a {} token budget",
                selected_sections.len(),
                metrics.relevant_files,
                task_type,
                request.token_budget
            ),
            selected_sections,
            omitted_section_ids,
            metrics,
        })
    }

    fn pass_dead_context_elimination(
        &self,
        active_file: &str,
        graph: &DependencyGraph,
        max_depth: usize,
    ) -> Vec<RelevantFile> {
        let mut queue = VecDeque::from([(active_file.to_string(), 0usize)]);
        let mut seen = HashSet::from([active_file.to_string()]);
        let mut out = vec![RelevantFile {
            path: active_file.to_string(),
            depth: 0,
        }];

        while let Some((file, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            let neighbors = graph
                .edges_out
                .get(&file)
                .into_iter()
                .flat_map(|deps| deps.iter())
                .chain(graph.edges_in.get(&file).into_iter().flat_map(|deps| deps.iter()));
            for neighbor in neighbors {
                if seen.insert(neighbor.clone()) {
                    queue.push_back((neighbor.clone(), depth + 1));
                    out.push(RelevantFile {
                        path: neighbor.clone(),
                        depth: depth + 1,
                    });
                }
            }
        }

        out.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));
        out
    }

    fn pass_skeleton_extraction(
        &self,
        active_file: &str,
        files: &[RelevantFile],
    ) -> Result<Vec<CodeSkeleton>> {
        let mut parser = AstParser::new()?;
        let mut skeletons = Vec::new();
        for file in files {
            let path = Path::new(&file.path);
            if !path.exists() || !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|v| v.to_str()) else {
                continue;
            };
            if !AstParser::is_supported(ext) {
                continue;
            }
            let Ok(source) = fs::read(path) else {
                continue;
            };
            let Ok(Some((tree, language))) = parser.parse_file(path) else {
                continue;
            };
            let features = parser.extract_features(&tree, language, &source, ext);
            if features.is_empty() {
                continue;
            }
            skeletons.push(build_code_skeleton(active_file, &file.path, &features));
        }
        Ok(skeletons)
    }

    fn pass_brain_dedup(
        &self,
        skeletons: Vec<CodeSkeleton>,
        brain_entries: &[MemoryEntry],
    ) -> (Vec<CodeSkeleton>, usize) {
        let mut deduplicated_files = 0usize;
        let out = skeletons
            .into_iter()
            .filter_map(|mut skeleton| {
                let mut coverage_hits = 0usize;
                for entry in brain_entries {
                    if entry.content.contains(&skeleton.path) {
                        coverage_hits += 1;
                    }
                    coverage_hits += skeleton
                        .exports
                        .iter()
                        .filter(|export_name| entry.content.contains(export_name.as_str()))
                        .count();
                }
                if coverage_hits >= 3 {
                    deduplicated_files += 1;
                    skeleton.full_functions.clear();
                    if skeleton.signatures.len() > 3 {
                        skeleton.signatures.truncate(3);
                    }
                }
                if skeleton.signatures.is_empty() && skeleton.types.is_empty() && skeleton.exports.is_empty() {
                    return None;
                }
                Some(skeleton)
            })
            .collect::<Vec<_>>();
        (out, deduplicated_files)
    }

    fn pass_history_compaction(&self, history: &[FlightRecord]) -> Vec<(String, String, u8)> {
        if history.is_empty() {
            return Vec::new();
        }
        let mut sections = Vec::new();
        let split = history.len().saturating_sub(4);
        let older = &history[..split];
        let recent = &history[split..];
        if !older.is_empty() {
            let mut mutation_count = 0usize;
            let mut intent_types = HashSet::new();
            let mut files = HashSet::new();
            for record in older {
                match &record.event {
                    SessionEvent::AstMutation { file, .. } => {
                        mutation_count += 1;
                        files.insert(file.clone());
                    }
                    SessionEvent::IntentDetected { intent_type } => {
                        intent_types.insert(intent_type.clone());
                    }
                    SessionEvent::MemoryAccessed { .. } | SessionEvent::ScorePenalty { .. } => {}
                }
            }
            let mut touched_files = files.into_iter().collect::<Vec<_>>();
            touched_files.sort();
            let mut intents = intent_types.into_iter().collect::<Vec<_>>();
            intents.sort();
            sections.push((
                "history:summary".to_string(),
                format!(
                    "Earlier session summary:\n- AST mutations: {}\n- Intent types: {}\n- Files touched: {}",
                    mutation_count,
                    if intents.is_empty() { "none".to_string() } else { intents.join(", ") },
                    if touched_files.is_empty() { "none".to_string() } else { touched_files.join(", ") }
                ),
                55,
            ));
        }
        if !recent.is_empty() {
            let recent_lines = recent
                .iter()
                .map(|record| format!("- {} :: {}", record.timestamp.to_rfc3339(), summarize_event(&record.event)))
                .collect::<Vec<_>>()
                .join("\n");
            sections.push((
                "history:recent".to_string(),
                format!("Recent daemon history:\n{}", recent_lines),
                80,
            ));
        }
        sections
    }

    fn pass_rules_pruning(&self, task_type: &TaskType, query: Option<&str>) -> Vec<(String, String, u8)> {
        let mut sections = Vec::new();
        for path in self.rule_file_candidates() {
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let pruned = prune_rules_content(&content, task_type, query);
            if pruned.trim().is_empty() {
                continue;
            }
            sections.push((
                format!("rules:{}", path.file_name().and_then(|v| v.to_str()).unwrap_or("rules")),
                pruned,
                70,
            ));
        }
        sections
    }

    fn pass_priority_ranking(
        &self,
        active_file: &str,
        graph: &DependencyGraph,
        task_type: &TaskType,
        skeletons: Vec<CodeSkeleton>,
        history_sections: Vec<(String, String, u8)>,
        rules_sections: Vec<(String, String, u8)>,
        skeleton_entries: &[MemoryEntry],
        causal_context: Option<String>,
        query: Option<&str>,
    ) -> Result<Vec<CompiledSection>> {
        let mut sections = Vec::new();

        // Query-aware priority boost. Parse the developer's prompt into
        // meaningful terms and reward sections whose content contains them.
        // Max boost is +10 priority points — enough to lift a relevant
        // skeleton-fsi (base 85) above an irrelevant code-skeleton (base 95)
        // without making the boost dominate structural importance.
        let query_terms: Vec<String> = query
            .map(|q| {
                q.split_whitespace()
                    .filter(|w| w.len() >= 3)
                    .map(|w| {
                        w.to_lowercase()
                            .trim_matches(|c: char| !c.is_alphanumeric())
                            .to_string()
                    })
                    .filter(|w| !w.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // Returns 0–10 additional priority points based on term overlap.
        let query_boost = |content: &str| -> u8 {
            if query_terms.is_empty() {
                return 0;
            }
            let lower = content.to_lowercase();
            let matched = query_terms.iter().filter(|t| lower.contains(t.as_str())).count();
            let ratio = matched as f64 / query_terms.len() as f64;
            (ratio * 10.0).round().min(10.0) as u8
        };

        let importance = graph.importance_scores(0);
        sections.push(compiled_section(
            "active:file".to_string(),
            "active-context".to_string(),
            100,
            format!("Active file: {}\nTask type: {:?}", active_file, task_type),
        )?);
        if let Some(causal_context) = causal_context {
            sections.push(compiled_section(
                format!("causal:{}", active_file),
                "causal-chain".to_string(),
                95,
                causal_context,
            )?);
        }
        for skeleton in skeletons {
            let base_priority: u8 = if skeleton.path == active_file { 95 } else { 72 };
            let betweenness = importance.betweenness.get(&skeleton.path).copied().unwrap_or(0.0);
            let pagerank = importance.pagerank.get(&skeleton.path).copied().unwrap_or(0.0);
            let combined = (betweenness * 0.7) + (pagerank * 0.3);
            let boost = (combined * 15.0).round() as i32;
            let priority = if boost > 0 {
                base_priority.saturating_add(boost as u8).min(100)
            } else {
                base_priority
            };
            let rendered = render_skeleton(&skeleton);
            let qboost = query_boost(&rendered);
            sections.push(compiled_section(
                format!("code:{}", skeleton.path),
                "code-skeleton".to_string(),
                priority.saturating_add(qboost),
                rendered,
            )?);
        }

        // Inject skeleton index sections (FSI = priority 85, FuSI = priority 78)
        for entry in skeleton_entries {
            let path = entry.tags.iter()
                .find_map(|t| t.strip_prefix("path:"))
                .or_else(|| entry.content.lines()
                    .find_map(|l| l.strip_prefix("path: ")));
            
            // Skip entries whose path contains a generated directory component
            if let Some(p) = path {
                if GENERATED_DIRS.iter().any(|dir| p.contains(dir)) {
                    continue;
                }
            }
            
            let is_fsi = entry.tags.contains(&"fsi".to_string());
            let is_fusi = entry.tags.contains(&"fusi".to_string());
            if is_fsi {
                let qboost = query_boost(&entry.content);
                sections.push(compiled_section(
                    format!("skel:{}", entry.id),
                    "skeleton-fsi".to_string(),
                    (85u8).saturating_add(qboost),
                    entry.content.clone(),
                )?);
            } else if is_fusi {
                let qboost = query_boost(&entry.content);
                sections.push(compiled_section(
                    format!("skel:{}", entry.id),
                    "skeleton-fusi".to_string(),
                    (78u8).saturating_add(qboost),
                    entry.content.clone(),
                )?);
            }
        }

        for (id, content, priority) in history_sections {
            sections.push(compiled_section(id, "history".to_string(), priority, content)?);
        }
        for (id, content, priority) in rules_sections {
            sections.push(compiled_section(id, "rules".to_string(), priority, content)?);
        }
        sections.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.tokens.cmp(&b.tokens)));
        Ok(sections)
    }

    fn pass_budget_fitting(
        &self,
        ranked_sections: Vec<CompiledSection>,
        budget: usize,
    ) -> Result<(Vec<CompiledSection>, Vec<String>, usize)> {
        let n = ranked_sections.len();
        let mut dp = vec![vec![0usize; budget + 1]; n + 1];
        for i in 1..=n {
            let tokens = ranked_sections[i - 1].tokens;
            let value = usize::from(ranked_sections[i - 1].priority).saturating_mul(100) + 1;
            for cap in 0..=budget {
                dp[i][cap] = dp[i - 1][cap];
                if tokens <= cap {
                    let with = dp[i - 1][cap - tokens].saturating_add(value);
                    if with > dp[i][cap] {
                        dp[i][cap] = with;
                    }
                }
            }
        }

        let mut selected = Vec::new();
        let mut omitted = Vec::new();
        let mut total_tokens = 0usize;
        let mut cap = budget;
        let mut chosen = HashSet::new();
        for i in (1..=n).rev() {
            if dp[i][cap] != dp[i - 1][cap] {
                let section = ranked_sections[i - 1].clone();
                cap = cap.saturating_sub(section.tokens);
                total_tokens = total_tokens.saturating_add(section.tokens);
                chosen.insert(section.id.clone());
                selected.push(section);
            }
        }
        selected.reverse();
        for section in ranked_sections {
            if !chosen.contains(&section.id) {
                omitted.push(section.id);
            }
        }
        Ok((selected, omitted, total_tokens))
    }

    fn rule_file_candidates(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        
        let Some(root) = &self.workspace_root else {
            return out;
        };
        
        // =========================================================================
        // IDE-SPECIFIC RULES (Project-level conventions)
        // =========================================================================
        
        // Cursor IDE
        out.push(root.join(".cursorrules"));
        out.push(root.join(".cursor/rules"));
        out.push(root.join(".cursor/rules/global.mdc"));
        out.push(root.join(".cursor/rules/project.mdc"));
        
        // Windsurf IDE
        out.push(root.join(".windsurfrules"));
        out.push(root.join(".windsurfrules-guard"));
        
        // VS Code / GitHub Copilot
        out.push(root.join(".github/copilot-instructions.md"));
        out.push(root.join(".vscode/settings.json"));
        out.push(root.join(".vscode/extensions.json"));
        out.push(root.join(".vscode/launch.json"));
        out.push(root.join(".vscode/tasks.json"));
        out.push(root.join(".vscode/cspell.json"));
        
        // Cline (AI assistant)
        out.push(root.join(".cline/rules"));
        out.push(root.join(".cline/config.json"));
        
        // Continue.dev
        out.push(root.join(".continue/config.json"));
        out.push(root.join(".continue/rules.md"));
        
        // Zed Editor
        out.push(root.join(".zed/settings.json"));
        out.push(root.join(".zed/keymap.json"));
        out.push(root.join(".zed/tasks.json"));
        
        // IntelliJ / JetBrains IDEs
        out.push(root.join(".idea/codeStyles/Project.xml"));
        out.push(root.join(".idea/codeStyles/codeStyleConfig.xml"));
        out.push(root.join(".idea/inspectionProfiles/Project_Default.xml"));
        out.push(root.join(".idea/runConfigurations"));
        out.push(root.join(".idea/jsLinters/eslint.xml"));
        
        // Eclipse
        out.push(root.join(".settings/org.eclipse.core.resources.prefs"));
        out.push(root.join(".settings/org.eclipse.jdt.core.prefs"));
        out.push(root.join(".settings/org.eclipse.jdt.ui.prefs"));
        
        // =========================================================================
        // AI ASSISTANT & AGENT CONFIGURATIONS (Project-level, not generic prompts)
        // =========================================================================
        
        // Claude Code (Anthropic)
        out.push(root.join("CLAUDE.md"));
        out.push(root.join(".claude/instructions.md"));
        out.push(root.join(".claude/rules.md"));
        out.push(root.join(".claude/settings.json"));
        
        // Cursor AI Rules (MDC format)
        out.push(root.join(".cursorrules"));
        out.push(root.join(".cursor/rules/global.mdc"));
        out.push(root.join(".cursor/rules/code-style.mdc"));
        out.push(root.join(".cursor/rules/architecture.mdc"));
        out.push(root.join(".cursor/rules/testing.mdc"));
        out.push(root.join(".cursor/rules/documentation.mdc"));
        
        // Windsurf Rules
        out.push(root.join(".windsurfrules"));
        out.push(root.join(".windsurf/config.json"));
        out.push(root.join(".windsurf/rules.md"));
        
        // Antigravity (Google)
        out.push(root.join(".antigravity/config.json"));
        out.push(root.join(".antigravity/rules.md"));
        
        // Memix (self)
        out.push(root.join(".memix/rules.md"));
        out.push(root.join(".memix/config.toml"));
        
        // Generic AI Rules (adopted by many tools)
        out.push(root.join(".ai/rules.md"));
        out.push(root.join(".ai/config.json"));
        out.push(root.join(".aider.conf.yml"));
        out.push(root.join(".aider.conf.yaml"));
        
        // =========================================================================
        // PROJECT CONVENTIONS & STYLE GUIDES
        // =========================================================================
        
        // Coding standards
        out.push(root.join("CODING_STANDARDS.md"));
        out.push(root.join("STYLE_GUIDE.md"));
        out.push(root.join("CONTRIBUTING.md"));
        out.push(root.join(".github/CONTRIBUTING.md"));
        out.push(root.join("ARCHITECTURE.md"));
        out.push(root.join("ARCHITECTURE_DECISIONS.md"));
        out.push(root.join("ADR.md"));
        out.push(root.join("docs/architecture.md"));
        out.push(root.join("docs/decisions.md"));
        out.push(root.join("docs/ADR"));
        out.push(root.join("docs/adr"));
        
        // =========================================================================
        // CODE QUALITY & LINTING CONFIGURATIONS
        // =========================================================================
        
        // ESLint
        out.push(root.join(".eslintrc"));
        out.push(root.join(".eslintrc.json"));
        out.push(root.join(".eslintrc.js"));
        out.push(root.join(".eslintrc.cjs"));
        out.push(root.join(".eslintrc.yaml"));
        out.push(root.join(".eslintrc.yml"));
        out.push(root.join("eslint.config.js"));
        out.push(root.join("eslint.config.mjs"));
        out.push(root.join("eslint.config.ts"));
        
        // Prettier
        out.push(root.join(".prettierrc"));
        out.push(root.join(".prettierrc.json"));
        out.push(root.join(".prettierrc.js"));
        out.push(root.join(".prettierrc.yaml"));
        out.push(root.join(".prettierrc.yml"));
        out.push(root.join("prettier.config.js"));
        out.push(root.join(".prettierignore"));
        
        // Biome
        out.push(root.join("biome.json"));
        out.push(root.join("biome.jsonc"));
        out.push(root.join(".biome.json"));
        
        // Ruff (Python)
        out.push(root.join("ruff.toml"));
        out.push(root.join(".ruff.toml"));
        out.push(root.join("pyproject.toml")); // contains ruff config
        
        // Black (Python)
        out.push(root.join("pyproject.toml")); // contains black config
        out.push(root.join(".black"));
        
        // Go
        out.push(root.join(".golangci.yml"));
        out.push(root.join(".golangci.yaml"));
        out.push(root.join("golangci.yml"));
        
        // Rust
        out.push(root.join("rustfmt.toml"));
        out.push(root.join(".rustfmt.toml"));
        out.push(root.join("clippy.toml"));
        out.push(root.join(".clippy.toml"));
        
        // =========================================================================
        // FRAMEWORK-SPECIFIC CONFIGURATIONS
        // =========================================================================
        
        // Next.js
        out.push(root.join("next.config.js"));
        out.push(root.join("next.config.ts"));
        out.push(root.join("next.config.mjs"));
        
        // Nuxt
        out.push(root.join("nuxt.config.ts"));
        out.push(root.join("nuxt.config.js"));
        
        // SvelteKit
        out.push(root.join("svelte.config.js"));
        
        // Astro
        out.push(root.join("astro.config.mjs"));
        out.push(root.join("astro.config.ts"));
        
        // Vite
        out.push(root.join("vite.config.js"));
        out.push(root.join("vite.config.ts"));
        out.push(root.join("vite.config.mjs"));
        
        // Webpack
        out.push(root.join("webpack.config.js"));
        out.push(root.join("webpack.config.ts"));
        
        // Tailwind CSS
        out.push(root.join("tailwind.config.js"));
        out.push(root.join("tailwind.config.ts"));
        out.push(root.join("tailwind.config.mjs"));
        
        // TypeScript
        out.push(root.join("tsconfig.json"));
        out.push(root.join("tsconfig.build.json"));
        out.push(root.join("tsconfig.node.json"));
        out.push(root.join("tsconfig.web.json"));
        
        // =========================================================================
        // PACKAGE MANAGER CONFIGURATIONS
        // =========================================================================
        
        out.push(root.join("package.json"));
        out.push(root.join("pnpm-workspace.yaml"));
        out.push(root.join(".npmrc"));
        out.push(root.join(".yarnrc.yml"));
        out.push(root.join(".yarnrc"));
        out.push(root.join("bun.lockb"));
        
        // =========================================================================
        // BUILD & DEPLOYMENT CONFIGURATIONS
        // =========================================================================
        
        out.push(root.join("Dockerfile"));
        out.push(root.join("docker-compose.yml"));
        out.push(root.join("docker-compose.override.yml"));
        out.push(root.join(".dockerignore"));
        
        out.push(root.join("Makefile"));
        out.push(root.join("makefile"));
        out.push(root.join("justfile"));
        out.push(root.join("Taskfile.yml"));
        
        // =========================================================================
        // CI/CD CONFIGURATIONS (Project intelligence)
        // =========================================================================
        
        // GitHub Actions
        out.push(root.join(".github/workflows/*.yml"));
        out.push(root.join(".github/workflows/*.yaml"));
        out.push(root.join(".github/CODEOWNERS"));
        out.push(root.join(".github/PULL_REQUEST_TEMPLATE.md"));
        
        // GitLab CI
        out.push(root.join(".gitlab-ci.yml"));
        out.push(root.join(".gitlab/merge_request_templates/*.md"));
        
        // CircleCI
        out.push(root.join(".circleci/config.yml"));
        
        // Bitbucket Pipelines
        out.push(root.join("bitbucket-pipelines.yml"));
        
        // Azure DevOps
        out.push(root.join("azure-pipelines.yml"));
        
        // Jenkins
        out.push(root.join("Jenkinsfile"));
        
        // =========================================================================
        // ENVIRONMENT CONFIGURATIONS (Templates, not secrets)
        // =========================================================================
        
        out.push(root.join(".env.example"));
        out.push(root.join(".env.template"));
        out.push(root.join(".env.sample"));
        out.push(root.join("env.example"));
        out.push(root.join("env.sample"));
        
        // =========================================================================
        // TESTING CONFIGURATIONS
        // =========================================================================
        
        out.push(root.join("vitest.config.ts"));
        out.push(root.join("vitest.config.js"));
        out.push(root.join("jest.config.js"));
        out.push(root.join("jest.config.ts"));
        out.push(root.join("playwright.config.ts"));
        out.push(root.join("cypress.config.ts"));
        
        // =========================================================================
        // DOCUMENTATION
        // =========================================================================
        
        out.push(root.join("README.md"));
        out.push(root.join("README.rst"));
        out.push(root.join(".github/README.md"));
        out.push(root.join("docs/README.md"));
        out.push(root.join("CHANGELOG.md"));
        out.push(root.join("LICENSE"));
        out.push(root.join("LICENSE.md"));
        
        // =========================================================================
        // LEGACY / WILDCARD SUPPORT
        // =========================================================================
        
        // Generic .rules directory (many tools)
        out.push(root.join(".rules"));
        out.push(root.join(".rules/*.md"));
        out.push(root.join(".rules/*.mdc"));
        out.push(root.join(".rules/*.toml"));
        
        // Note: AGENTS.md and similar generic AI instruction files are intentionally
        // excluded from context compilation. They are AI agent prompts, not project-
        // specific coding conventions or architectural decisions. Including them
        // pollutes the context with generic instructions that are rarely relevant
        // to specific coding queries. Users can still manually include them via
        // .rules/ or .cursor/rules/ if they want them in context.
        
        // Deduplicate (preserve order but remove duplicates)
        let mut seen = std::collections::HashSet::new();
        out.retain(|path| {
            let path_str = path.to_string_lossy().to_string();
            if seen.contains(&path_str) {
                false
            } else {
                seen.insert(path_str);
                true
            }
        });
        
        out
    }
}

fn parse_task_type(raw: Option<&str>) -> TaskType {
    match raw.unwrap_or("").trim().to_lowercase().as_str() {
        "newfeature" | "new_feature" | "feature" => TaskType::NewFeature,
        "bugfix" | "bug_fix" | "bug" => TaskType::BugFix,
        "refactor" => TaskType::Refactor,
        "codereview" | "code_review" | "review" => TaskType::CodeReview,
        _ => TaskType::Unknown,
    }
}

fn build_code_skeleton(active_file: &str, path: &str, features: &[AstNodeFeature]) -> CodeSkeleton {
    let mut signatures = Vec::new();
    let mut types = Vec::new();
    let mut exports = Vec::new();
    for feature in features {
        if matches!(feature.kind.as_str(), "class" | "interface" | "type") {
            types.push(signature_head(&feature.body));
        } else {
            signatures.push(signature_head(&feature.body));
        }
        if feature.is_exported {
            exports.push(feature.name.clone());
        }
    }
    let mut full_functions = Vec::new();
    if path == active_file {
        let mut sorted = features.to_vec();
        sorted.sort_by(|a, b| b.cyclomatic_complexity.cmp(&a.cyclomatic_complexity));
        full_functions = sorted
            .into_iter()
            .take(2)
            .map(|feature| feature.body)
            .collect();
    }
    signatures.sort();
    signatures.dedup();
    types.sort();
    types.dedup();
    exports.sort();
    exports.dedup();
    CodeSkeleton {
        path: path.to_string(),
        signatures,
        types,
        exports,
        full_functions,
    }
}

fn signature_head(body: &str) -> String {
    let first = body.lines().next().unwrap_or("").trim();
    first.split('{').next().unwrap_or(first).trim().to_string()
}

fn render_skeleton(skeleton: &CodeSkeleton) -> String {
    let mut parts = vec![format!("File: {}", skeleton.path)];
    if !skeleton.exports.is_empty() {
        parts.push(format!("Exports: {}", skeleton.exports.join(", ")));
    }
    if !skeleton.signatures.is_empty() {
        parts.push(format!("Signatures:\n{}", skeleton.signatures.join("\n")));
    }
    if !skeleton.types.is_empty() {
        parts.push(format!("Types:\n{}", skeleton.types.join("\n")));
    }
    if !skeleton.full_functions.is_empty() {
        parts.push(format!("Included bodies:\n{}", skeleton.full_functions.join("\n\n")));
    }
    parts.join("\n\n")
}

fn compiled_section(id: String, kind: String, priority: u8, content: String) -> Result<CompiledSection> {
    let tokens = TokenEngine::count_tokens(&content)?;
    Ok(CompiledSection {
        id,
        kind,
        priority,
        tokens,
        content,
    })
}

fn summarize_event(event: &SessionEvent) -> String {
    match event {
        SessionEvent::AstMutation { file, nodes_changed } => {
            format!("AST mutation in {} ({} nodes changed)", file, nodes_changed)
        }
        SessionEvent::MemoryAccessed { memory_id } => {
            format!("Memory accessed: {}", memory_id)
        }
        SessionEvent::IntentDetected { intent_type } => {
            format!("Intent detected: {}", intent_type)
        }
        SessionEvent::ScorePenalty { reason, severity } => {
            format!("Score penalty [{}]: {}", severity, reason)
        }
    }
}

fn prune_rules_content(content: &str, task_type: &TaskType, query: Option<&str>) -> String {
    // Task-type keywords (static)
    let task_keywords: &[&str] = match task_type {
        TaskType::NewFeature => &["pattern", "architecture", "memory", "auto-save", "agent"],
        TaskType::BugFix => &["warning", "issue", "debug", "safety", "agent"],
        TaskType::Refactor => &["dependency", "decision", "pattern", "architecture", "agent"],
        TaskType::CodeReview => &["pattern", "convention", "warning", "safety", "agent"],
        TaskType::Unknown => &["memory", "pattern", "agent"],
    };
    
    // Extract query terms (owned strings for lifetime safety)
    let query_terms: Vec<String> = query
        .map(|q| {
            q.split_whitespace()
                .filter(|w| w.len() >= 3)
                .map(|w| w.to_lowercase()
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string())
                .filter(|w| !w.is_empty())
                .collect()
        })
        .unwrap_or_default();
    
    let mut selected = Vec::new();
    for line in content.lines() {
        let lowered = line.to_lowercase();
        // Check task keywords
        let matches_task = task_keywords.iter().any(|keyword| lowered.contains(keyword));
        // Check query terms
        let matches_query = query_terms.iter().any(|term| lowered.contains(term.as_str()));
        
        if matches_task || matches_query {
            selected.push(line);
        }
    }
    if selected.is_empty() {
        // No matches - only include if query is empty, otherwise skip entirely
        if query.is_none() || query.map(|q| q.trim().is_empty()).unwrap_or(true) {
            content.lines().take(30).collect::<Vec<_>>().join("\n")
        } else {
            String::new() // Return empty to signal "skip this file"
        }
    } else {
        selected.into_iter().take(80).collect::<Vec<_>>().join("\n")
    }
}
