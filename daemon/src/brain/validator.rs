use crate::brain::schema::MemoryEntry;
use anyhow::{anyhow, Result};
use regex::Regex;

pub struct BrainValidator {
    secret_patterns: Vec<Regex>,
    max_content_size: usize,
    max_tags: usize,
}

impl BrainValidator {
    pub fn new() -> Self {
        static SECRET_PATTERNS: std::sync::OnceLock<Vec<Regex>> = std::sync::OnceLock::new();
        let secret_patterns = SECRET_PATTERNS.get_or_init(|| vec![
            Regex::new(r#"(?i)(api[_-]?key|secret[_-]?key|access[_-]?token)["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}"#).unwrap(),
            Regex::new(r#"(?i)password\s*[:=]\s*["'][^"']+"#).unwrap(),
            Regex::new(r#"(?i)(private[_-]?key|PRIVATE[_-]?KEY)["']?\s*[:=]\s*"#).unwrap(),
            Regex::new(r"-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----").unwrap(),
            Regex::new(r"ghp_[a-zA-Z0-9]{36}").unwrap(),
            Regex::new(r"sk-[a-zA-Z0-9]{48}").unwrap(),
            Regex::new(r"sq0csp-[a-zA-Z0-9\-]{43}").unwrap(),
            Regex::new(r"sk_live_[a-zA-Z0-9]{24,}").unwrap(),
        ]).clone();

        Self {
            secret_patterns,
            max_content_size: 51200,
            max_tags: 20,
        }
    }

    pub fn validate_entry(&self, entry: &MemoryEntry) -> Result<()> {
        if entry.content.is_empty() {
            return Err(anyhow!("Entry content cannot be empty"));
        }

        if entry.content.len() > self.max_content_size {
            return Err(anyhow!(
                "Entry content exceeds maximum size of {} bytes",
                self.max_content_size
            ));
        }

        if entry.tags.len() > self.max_tags {
            return Err(anyhow!(
                "Entry cannot have more than {} tags",
                self.max_tags
            ));
        }

        if self.contains_secret(&entry.content) {
            return Err(anyhow!("Entry contains potential secrets - blocked for security"));
        }

        Ok(())
    }

    pub fn validate_project_id(&self, project_id: &str) -> Result<()> {
        if project_id.is_empty() {
            return Err(anyhow!("Project ID cannot be empty"));
        }

        if project_id.len() > 100 {
            return Err(anyhow!("Project ID cannot exceed 100 characters"));
        }

        static VALID_CHARS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let valid_chars = VALID_CHARS.get_or_init(|| Regex::new(r"^[a-zA-Z0-9_\-\.]+$").unwrap());
        if !valid_chars.is_match(project_id) {
            return Err(anyhow!("Project ID contains invalid characters"));
        }

        Ok(())
    }

    pub fn contains_secret(&self, content: &str) -> bool {
        self.secret_patterns
            .iter()
            .any(|pattern| pattern.is_match(content))
    }

    pub fn sanitize_content(&self, content: &str) -> String {
        let mut sanitized = content.to_string();

        for pattern in &self.secret_patterns {
            sanitized = pattern.replace_all(&sanitized, "[REDACTED]").to_string();
        }

        sanitized
    }
}

impl Default for BrainValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_detection() {
        let validator = BrainValidator::new();
        
        assert!(validator.contains_secret("api_key=ghp_abcdefghijklmnopqrstuvwxyz1234567890"));
        assert!(validator.contains_secret("password: \"mysecretpassword123\""));
        assert!(!validator.contains_secret("This is normal code content"));
    }

    #[test]
    fn test_validation() {
        let validator = BrainValidator::new();
        
        let valid_entry = MemoryEntry {
            id: "test".to_string(),
            project_id: "myproject".to_string(),
            kind: crate::brain::schema::MemoryKind::Fact,
            content: "This is a test fact".to_string(),
            tags: vec!["test".to_string()],
            source: crate::brain::schema::MemorySource::UserManual,
            superseded_by: None,
            contradicts: vec![],
			parent_id: None,
			caused_by: vec![],
			enables: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            access_count: 0,
            last_accessed_at: None,
        };

        assert!(validator.validate_entry(&valid_entry).is_ok());
    }
}
