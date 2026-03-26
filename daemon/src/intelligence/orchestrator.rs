/**
 *
 * Context Orchestrator — the layer that turns raw developer questions into
 * structurally-enriched prompts before they reach any AI model.
 *
 * Problem it solves
 *
 * When a developer asks an AI "why is my license validation failing?",
 * the AI has no structural knowledge of the codebase. It discovers
 * relevant files through expensive tool calls — each one is a full
 * API round-trip that carries the entire conversation history. A
 * 10-step discovery costs ~55 context-window-loads of tokens.
 *
 * The Orchestrator eliminates this by front-loading the discovery work
 * locally, before the question reaches the model. The AI receives a
 * single rich prompt containing all structural context it needs,
 * enabling a one-shot answer with zero tool calls.
 *
 *
 */

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::brain::schema::MemoryEntry;
use crate::context::{CompileRequest, CompiledContext, CompiledSection, ContextCompiler};
use crate::observer::graph::DependencyGraph;
use crate::recorder::flight::FlightRecord;

// ─── Public API types ─────────────────────────────────────────────────────────

/// Everything the orchestrator needs from the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrateRequest {
    /// The raw question or instruction the developer typed.
    pub prompt: String,

    /// Which project to compile context for.
    pub project_id: String,

    /// The file currently open in the editor.
    /// This is the dependency-graph anchor — all relevant files are discovered
    /// by traversing imports/exports from this starting point.
    pub active_file: String,

    /// Maximum tokens the compiled context block may consume.
    /// Defaults to 3000, leaving headroom for the developer's question and
    /// the AI's response within a typical 8k input window.
    #[serde(default)]
    pub context_budget: Option<usize>,

    /// Task type hint — tunes which context sections rank highest.
    /// Valid values: "bugfix", "new_feature", "refactor", "code_review"
    #[serde(default)]
    pub task_type: Option<String>,

    /// Maximum dependency graph traversal depth.
    /// Orchestration defaults to 3 (vs 2 for panel display) to find more
    /// indirectly relevant files for the specific question.
    #[serde(default)]
    pub max_depth: Option<usize>,
}

/// Everything the caller receives back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrateResponse {
    /// The fully enhanced prompt, ready to paste into any AI chat.
    /// Contains a labelled structural context block followed by the original question.
    pub enhanced_prompt: String,

    /// How many context sections were included.
    pub sections_used: usize,

    /// How many tokens the structural context block consumed.
    pub compiled_tokens: u64,

    /// What a naive full-file-dump approach would have cost.
    /// Ratio (naive / compiled) is the compression efficiency.
    pub naive_estimate: u64,

    /// compression ratio = naive_estimate / compiled_tokens.
    /// A value of 8.0 means this prompt is 8× more token-efficient than
    /// pasting the raw files into the chat.
    pub compression_ratio: f64,

    /// Files identified as structurally relevant to the question.
    /// Useful for debugging — the developer can see exactly what Memix found.
    pub relevant_files: Vec<String>,

    /// Summary from the context compiler explaining what was selected and why.
    pub compilation_summary: String,
}

// ─── Orchestrator ─────────────────────────────────────────────────────────────

pub struct Orchestrator {
    workspace_root: Option<PathBuf>,
}

impl Orchestrator {
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        Self { workspace_root }
    }

    /// Core enhancement pipeline. Three steps:
    ///
    /// 1. Run the context compiler with the developer's prompt as a ranking
    ///    hint, so sections whose content is semantically relevant to the
    ///    specific question rank above structurally-important-but-unrelated ones.
    ///
    /// 2. Render a labelled context block from the selected sections, grouping
    ///    them by kind (call graph, code structure, history, rules) so the AI
    ///    can orient itself immediately.
    ///
    /// 3. Prepend the context block to the original question, along with a
    ///    header that tells the AI what it is receiving and why — which prevents
    ///    the AI from hallucinating context it doesn't have.
    pub fn enhance(
        &self,
        req: OrchestrateRequest,
        graph: &DependencyGraph,
        history: &[FlightRecord],
        brain_entries: &[MemoryEntry],
        skeleton_entries: &[MemoryEntry],
        causal_context: Option<String>,
    ) -> Result<OrchestrateResponse> {
        let budget = req.context_budget.unwrap_or(3000);

        // The query field carries the developer's prompt into the compiler's
        // priority-ranking pass, which boosts sections whose content contains
        // terms from the prompt. The orchestrator therefore uses a deeper
        // graph traversal (max_depth 3 vs 2) to cast a wider net, trusting
        // the query-boost to filter down to what is actually relevant.
        let compile_req = CompileRequest {
            project_id:   req.project_id.clone(),
            active_file:  req.active_file.clone(),
            token_budget: budget,
            task_type:    req.task_type.clone(),
            max_depth:    Some(req.max_depth.unwrap_or(3)),
            query:        Some(req.prompt.clone()),
        };

        let compiled = ContextCompiler::new(self.workspace_root.clone()).compile(
            compile_req,
            graph,
            history,
            brain_entries,
            skeleton_entries,
            causal_context,
        )?;

        let compiled_tokens  = compiled.total_tokens as u64;
        let naive_estimate   = compiled.naive_token_estimate;
        let compilation_summary = compiled.explainability_summary.clone();

        let compression_ratio = if compiled_tokens > 0 {
            naive_estimate as f64 / compiled_tokens as f64
        } else {
            1.0
        };

        // Extract the files we actually included from the selected sections.
        // code-skeleton section IDs are formatted as "code:{absolute_path}".
        let relevant_files: Vec<String> = compiled.selected_sections
            .iter()
            .filter_map(|s| {
                if s.kind == "code-skeleton" {
                    s.id.strip_prefix("code:").map(|p| p.to_string())
                } else {
                    None
                }
            })
            .collect();

        let enhanced_prompt = build_enhanced_prompt(&req.prompt, &compiled);

        Ok(OrchestrateResponse {
            enhanced_prompt,
            sections_used: compiled.selected_sections.len(),
            compiled_tokens,
            naive_estimate,
            compression_ratio,
            relevant_files,
            compilation_summary,
        })
    }
}

// ─── Prompt assembly ──────────────────────────────────────────────────────────

/// Assembles the final enhanced prompt.
///
/// Layout:
///   MEMIX STRUCTURAL CONTEXT — {n} sections
///
///   ### Active Context
///   {active context section}
///
///   ### Call Graph (Causal Chain)
///   {causal chain sections}
///
///   ### Code Structure
///   // {label} — {tokens} tokens
///   {skeleton/index content}
///
///   ### Recent Development Activity
///   {history sections}
///
///   ### Project Rules & Conventions
///   {rules sections}
///
///   ---
///
///   QUESTION:
///   {original developer prompt}
///
/// The AI receives this as a single user message. Because all structural
/// discovery has already been done by Memix, the AI can answer in one shot.
fn build_enhanced_prompt(original_prompt: &str, compiled: &CompiledContext) -> String {
    let mut parts: Vec<String> = Vec::new();

    let compression_display = if compiled.naive_token_estimate > 0 && compiled.total_tokens > 0 {
        format!(
            "{:.1}×",
            compiled.naive_token_estimate as f64 / compiled.total_tokens as f64
        )
    } else {
        "N/A".to_string()
    };

    parts.push(format!(
        // "MEMIX STRUCTURAL CONTEXT — {} sections · {} tokens · {} more efficient than raw file paste",
        "MEMIX STRUCTURAL CONTEXT — {} sections",
        compiled.selected_sections.len(),
        // compiled.total_tokens,
        // compression_display,
    ));

    // Partition sections by kind for coherent grouping.
    let active:   Vec<&CompiledSection> = sections_of_kind(compiled, "active-context");
    let causal:   Vec<&CompiledSection> = sections_of_kind(compiled, "causal-chain");
    let code:     Vec<&CompiledSection> = {
        let mut v = sections_of_kind(compiled, "code-skeleton");
        v.extend(sections_of_kind(compiled, "skeleton-fsi"));
        v.extend(sections_of_kind(compiled, "skeleton-fusi"));
        v
    };
    let history:  Vec<&CompiledSection> = sections_of_kind(compiled, "history");
    let rules:    Vec<&CompiledSection> = sections_of_kind(compiled, "rules");

    // Active context — always present, always first.
    if !active.is_empty() {
        let active_content: String = active.iter().map(|s| s.content.as_str()).collect();
        // Skip placeholder active context when no real file is open
        if !active_content.contains("Active file: Untitled") && !active_content.contains("Active file: ") {
            parts.push("### Active Context".to_string());
            for s in active {
                parts.push(s.content.clone());
            }
        }
    }

    // Causal chain — who calls what. Critical for bug-fixing questions.
    if !causal.is_empty() {
        parts.push("### Call Graph (Causal Chain)".to_string());
        for s in causal {
            parts.push(s.content.clone());
        }
    }

    // Code structure — the heart of what makes this prompt useful.
    // Each block is labelled so the AI knows what it is looking at.
    if !code.is_empty() {
        parts.push("### Code Structure".to_string());
        for s in &code {
            let label = match s.kind.as_str() {
                "code-skeleton"  => "Full skeleton",
                "skeleton-fsi"   => "File index entry",
                "skeleton-fusi"  => "Function index entry",
                _                => "Structure",
            };
            parts.push(format!(
                "// {}\n[skeleton:{}]\n{}",
                label, s.id, s.content
            ));
        }
    }

    // Session history — recent changes provide important "what was I just doing" context.
    if !history.is_empty() {
        parts.push("### Recent Development Activity".to_string());
        for s in history {
            parts.push(s.content.clone());
        }
    }

    // Rules — project conventions the AI must respect.
    if !rules.is_empty() {
        parts.push("### Project Rules & Conventions".to_string());
        for s in rules {
            parts.push(s.content.clone());
        }
    }

    // Clear separator between context and question.
    parts.push("---".to_string());
    parts.push(format!("QUESTION:\n{}", original_prompt));

    parts.join("\n\n")
}

/// Collects references to compiled sections of a specific kind.
fn sections_of_kind<'a>(compiled: &'a CompiledContext, kind: &str) -> Vec<&'a CompiledSection> {
    compiled.selected_sections
        .iter()
        .filter(|s| s.kind == kind)
        .collect()
}