use anyhow::{Result, Context};
use once_cell::sync::Lazy;
use tiktoken_rs::{cl100k_base, CoreBPE};

/// Process-wide cached tokenizer — initialized exactly once, reused forever.
static BPE: Lazy<CoreBPE> = Lazy::new(|| {
    cl100k_base().expect("Failed to initialize cl100k_base tokenizer")
});

pub struct TokenEngine;

impl TokenEngine {
    /// Get the exact token count for a piece of text using OpenAI's cl100k_base tokenizer.
    /// Uses a process-wide cached BPE instance for zero-allocation reuse.
    pub fn count_tokens(text: &str) -> Result<usize> {
        let tokens = BPE.encode_ordinary(text);
        Ok(tokens.len())
    }

    /// Safely truncate text to fit exactly within a specific context window limit.
    pub fn truncate_to_budget(text: &str, max_tokens: usize) -> Result<String> {
        let mut tokens = BPE.encode_ordinary(text);
        
        if tokens.len() <= max_tokens {
            return Ok(text.to_string());
        }

        tokens.truncate(max_tokens);
        let truncated_text = BPE.decode(tokens).context("Failed to decode token array back into string")?;
        Ok(truncated_text)
    }
}
