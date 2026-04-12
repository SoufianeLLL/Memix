//! Context compiler for budget-fit context packet generation.
//!
//! Compiles minimal, high-signal context for LLM prompts by:
//! - Relevant-file elimination
//! - AST skeleton extraction
//! - Brain deduplication
//! - History compaction
//! - Rules pruning
//! - Ranking and budget fitting

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Budget-fit compiled context packet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetCompiledContext {
    /// Total tokens in compiled context
    pub total_tokens: usize,
    /// Included sections
    pub sections: Vec<ContextSection>,
    /// Files included
    pub included_files: Vec<String>,
    /// Files excluded (with reasons)
    pub excluded_files: Vec<ExcludedFile>,
    /// Compilation metadata
    pub metadata: CompilationMetadata,
}

/// A section of compiled context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSection {
    /// Section name
    pub name: String,
    /// Section content
    pub content: String,
    /// Token count
    pub tokens: usize,
    /// Priority (higher = more important)
    pub priority: u32,
    /// Source type
    pub source: ContextSource,
}

/// Source of context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextSource {
    /// From brain memory
    Brain,
    /// From file content
    File,
    /// From AST skeleton
    AstSkeleton,
    /// From git history
    GitHistory,
    /// From rules/patterns
    Rules,
    /// From known issues
    KnownIssues,
}

/// Excluded file with reason
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludedFile {
    /// File path
    pub path: String,
    /// Reason for exclusion
    pub reason: ExclusionReason,
}

/// Reason for excluding a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExclusionReason {
    /// Not relevant to task
    NotRelevant,
    /// Exceeds budget
    BudgetExceeded,
    /// Duplicate of brain content
    DuplicateBrain,
    /// Low priority
    LowPriority,
    /// Binary/generated file
    BinaryGenerated,
}

/// Compilation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationMetadata {
    /// Budget used
    pub budget_used: usize,
    /// Budget total
    pub budget_total: usize,
    /// Files considered
    pub files_considered: usize,
    /// Files included
    pub files_included: usize,
    /// Brain entries deduplicated
    pub brain_deduplicated: usize,
    /// Compilation time (ms)
    pub compile_time_ms: u64,
}

/// Context compiler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerConfig {
    /// Maximum token budget
    pub max_tokens: usize,
    /// Include brain content
    pub include_brain: bool,
    /// Include AST skeletons
    pub include_ast_skeletons: bool,
    /// Include git history
    pub include_git: bool,
    /// Include rules
    pub include_rules: bool,
    /// Maximum file size to include (bytes)
    pub max_file_size: usize,
    /// Priority weights
    pub priority_weights: PriorityWeights,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            include_brain: true,
            include_ast_skeletons: true,
            include_git: false,
            include_rules: true,
            max_file_size: 100_000,
            priority_weights: PriorityWeights::default(),
        }
    }
}

/// Priority weights for different context types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityWeights {
    pub brain: u32,
    pub current_file: u32,
    pub related_files: u32,
    pub ast_skeleton: u32,
    pub git_history: u32,
    pub rules: u32,
    pub known_issues: u32,
}

impl Default for PriorityWeights {
    fn default() -> Self {
        Self {
            brain: 100,
            current_file: 90,
            related_files: 70,
            ast_skeleton: 50,
            git_history: 30,
            rules: 40,
            known_issues: 60,
        }
    }
}

/// Budget-fit context compiler
pub struct BudgetContextCompiler {
    config: CompilerConfig,
    /// Brain content already included (for deduplication)
    brain_content: HashSet<String>,
    /// File relevance scores
    file_scores: HashMap<String, f64>,
}

impl BudgetContextCompiler {
    pub fn new(config: CompilerConfig) -> Self {
        Self {
            config,
            brain_content: HashSet::new(),
            file_scores: HashMap::new(),
        }
    }
    
    /// Compile context for a task
    pub fn compile(
        &mut self,
        _task_description: &str,
        current_file: Option<&str>,
        related_files: &[String],
        brain_entries: &[String],
        rules: &[String],
        known_issues: &[String],
    ) -> Result<BudgetCompiledContext> {
        let start = std::time::Instant::now();
        let mut sections = Vec::new();
        let mut included_files = Vec::new();
        let mut excluded_files = Vec::new();
        let mut total_tokens = 0;
        let mut files_considered = 0;
        let mut brain_deduplicated = 0;
        
        // Phase 1: Add brain content (highest priority)
        if self.config.include_brain {
            for entry in brain_entries {
                let tokens = estimate_tokens(entry);
                if total_tokens + tokens <= self.config.max_tokens {
                    // Check for duplicates
                    let content_hash = simple_hash(entry);
                    if self.brain_content.insert(content_hash) {
                        sections.push(ContextSection {
                            name: "Brain".to_string(),
                            content: entry.clone(),
                            tokens,
                            priority: self.config.priority_weights.brain,
                            source: ContextSource::Brain,
                        });
                        total_tokens += tokens;
                    } else {
                        brain_deduplicated += 1;
                    }
                }
            }
        }
        
        // Phase 2: Add current file (high priority)
        if let Some(file) = current_file {
            files_considered += 1;
            if let Ok(content) = std::fs::read_to_string(file) {
                let content = truncate_content(&content, self.config.max_file_size);
                let tokens = estimate_tokens(&content);
                
                if total_tokens + tokens <= self.config.max_tokens {
                    sections.push(ContextSection {
                        name: format!("Current File: {}", file),
                        content,
                        tokens,
                        priority: self.config.priority_weights.current_file,
                        source: ContextSource::File,
                    });
                    included_files.push(file.to_string());
                    total_tokens += tokens;
                } else {
                    excluded_files.push(ExcludedFile {
                        path: file.to_string(),
                        reason: ExclusionReason::BudgetExceeded,
                    });
                }
            }
        }
        
        // Phase 3: Add related files (medium priority)
        for file in related_files {
            files_considered += 1;
            if included_files.contains(file) {
                continue;
            }
            
            if let Ok(content) = std::fs::read_to_string(file) {
                let content = truncate_content(&content, self.config.max_file_size);
                let tokens = estimate_tokens(&content);
                
                if total_tokens + tokens <= self.config.max_tokens {
                    sections.push(ContextSection {
                        name: format!("Related: {}", file),
                        content,
                        tokens,
                        priority: self.config.priority_weights.related_files,
                        source: ContextSource::File,
                    });
                    included_files.push(file.clone());
                    total_tokens += tokens;
                } else {
                    excluded_files.push(ExcludedFile {
                        path: file.clone(),
                        reason: ExclusionReason::BudgetExceeded,
                    });
                }
            }
        }
        
        // Phase 4: Add rules (if budget allows)
        if self.config.include_rules {
            for rule in rules {
                let tokens = estimate_tokens(rule);
                if total_tokens + tokens <= self.config.max_tokens {
                    sections.push(ContextSection {
                        name: "Rule".to_string(),
                        content: rule.clone(),
                        tokens,
                        priority: self.config.priority_weights.rules,
                        source: ContextSource::Rules,
                    });
                    total_tokens += tokens;
                }
            }
        }
        
        // Phase 5: Add known issues (if budget allows)
        for issue in known_issues {
            let tokens = estimate_tokens(issue);
            if total_tokens + tokens <= self.config.max_tokens {
                sections.push(ContextSection {
                    name: "Known Issue".to_string(),
                    content: issue.clone(),
                    tokens,
                    priority: self.config.priority_weights.known_issues,
                    source: ContextSource::KnownIssues,
                });
                total_tokens += tokens;
            }
        }
        
        // Sort sections by priority (descending)
        sections.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        let compile_time_ms = start.elapsed().as_millis() as u64;
        let files_included_count = included_files.len();
        
        Ok(BudgetCompiledContext {
            total_tokens,
            sections,
            included_files,
            excluded_files,
            metadata: CompilationMetadata {
                budget_used: total_tokens,
                budget_total: self.config.max_tokens,
                files_considered,
                files_included: files_included_count,
                brain_deduplicated,
                compile_time_ms,
            },
        })
    }
    
    /// Set file relevance scores
    pub fn set_file_scores(&mut self, scores: HashMap<String, f64>) {
        self.file_scores = scores;
    }
    
    /// Clear brain content cache
    pub fn clear_brain_cache(&mut self) {
        self.brain_content.clear();
    }
}

/// Estimate token count for content
fn estimate_tokens(content: &str) -> usize {
    // Rough estimate: ~4 characters per token
    content.len() / 4
}

/// Truncate content to max size
fn truncate_content(content: &str, max_size: usize) -> String {
    if content.len() <= max_size {
        content.to_string()
    } else {
        format!("{}... [truncated]", &content[..max_size])
    }
}

/// Simple hash for deduplication
fn simple_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
