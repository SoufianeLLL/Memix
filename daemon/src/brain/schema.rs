use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Decision,
    Warning,
    Pattern,
    Context,
    /// Explicit instruction on what NOT to do, guarding against regression hallucinatory behavior.
    Negative,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    UserManual,
    AgentExtracted,
    FileWatcher,
    GitArchaeology,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryEntry {
    pub id: String,
    pub project_id: String,
    pub kind: MemoryKind,
    pub content: String,
    pub tags: Vec<String>,
    pub source: MemorySource,
    
    /// True memory causality: if this decision supersedes legacy logic, we reference the old ID.
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Any memories that functionally contradict this logic, acting as active guardrails.
    #[serde(default)]
    pub contradicts: Vec<String>,

	/// Optional direct parent in the memory graph for hierarchical context.
	#[serde(default)]
	pub parent_id: Option<String>,
	/// Causal predecessors that explain why this memory exists.
	#[serde(default)]
	pub caused_by: Vec<String>,
	/// Downstream memories or tasks this memory enables.
	#[serde(default)]
	pub enables: Vec<String>,

    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "chrono::Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default)]
    pub last_accessed_at: Option<DateTime<Utc>>,
}
