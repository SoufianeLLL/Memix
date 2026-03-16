use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionScore {
    pub active_file: String,
    pub lines_written: u32,
    pub architectural_compliance: u8,
    pub hallucination_penalty: u8,
    pub timestamp: String,
}

pub struct SessionScorer;

impl SessionScorer {
    pub const MAX_ARCHITECTURAL_COMPLIANCE: u8 = 100;

    /// Extremely strict, enterprise-grade scoring evaluation.
    /// Memix calculates session health natively from codebase regressions and AST differentials.
    pub fn calculate_score(active_file: &str, lines_written: u32, contradiction_hits: u32, edits_reverted: u32) -> SessionScore {
        let mut compliance = 100u8;
        let mut penalty = 0u32;

        // Severe penalty for hitting explicit negative memory boundaries during active generation
        if contradiction_hits > 0 {
            compliance = compliance.saturating_sub((contradiction_hits * 15).try_into().unwrap_or(255));
            penalty += contradiction_hits * 20;
        }

        // Penalty for high code churn reverting recently generated chunks (indicates poor AI context loading)
        if edits_reverted > 0 {
            compliance = compliance.saturating_sub((edits_reverted * 5).try_into().unwrap_or(255));
            penalty += edits_reverted * 10;
        }

        SessionScore {
            active_file: active_file.to_string(),
            lines_written,
            architectural_compliance: compliance,
            hallucination_penalty: penalty.min(255) as u8,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}
