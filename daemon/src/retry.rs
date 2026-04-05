//! Retry logic with exponential backoff.
//! Based on patterns from claude-code-rust for resilient operations.

use std::time::Duration;
use std::future::Future;
use crate::error::{MemixError, Result};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Initial backoff delay
    pub initial_backoff: Duration,
    /// Maximum backoff delay
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
        }
    }
}

impl RetryConfig {
    /// Create a new retry config
    pub fn new(max_retries: u32, initial_backoff: Duration, max_backoff: Duration) -> Self {
        Self {
            max_retries,
            initial_backoff,
            max_backoff,
        }
    }

    /// Quick retry config (3 retries, 100ms-10s)
    pub fn quick() -> Self {
        Self::default()
    }

    /// Standard retry config (5 retries, 200ms-30s)
    pub fn standard() -> Self {
        Self {
            max_retries: 5,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(30),
        }
    }

    /// Aggressive retry config (10 retries, 50ms-60s)
    pub fn aggressive() -> Self {
        Self {
            max_retries: 10,
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(60),
        }
    }
}

/// Execute an operation with retry logic and exponential backoff.
/// 
/// # Example
/// ```ignore
/// use memix_daemon::retry::{with_retry, RetryConfig};
/// 
/// let result = with_retry(RetryConfig::quick(), || async {
///     some_fallible_operation().await.map_err(|e| MemixError::from(e))
/// }).await;
/// ```
pub async fn with_retry<T, F, Fut>(config: RetryConfig, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempts = 0;
    let mut last_error: Option<MemixError> = None;

    loop {
        attempts += 1;
        
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempts <= config.max_retries => {
                last_error = Some(e);
                let delay = backoff_for_attempt(attempts, &config)?;
                tracing::debug!(
                    "Retry attempt {}/{} after {:?}",
                    attempts,
                    config.max_retries,
                    delay
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }

    // This should never be reached, but just in case
    Err(MemixError::RetriesExhausted {
        attempts,
        last_error: Box::new(last_error.unwrap_or_else(|| MemixError::Other("Unknown error".to_string()))),
    })
}

/// Execute an operation with retry logic, passing the attempt number.
/// 
/// This is useful when the operation needs to know which attempt it is.
pub async fn with_retry_counted<T, F, Fut>(config: RetryConfig, operation: F) -> Result<T>
where
    F: Fn(u32) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempts = 0;
    let mut last_error: Option<MemixError> = None;

    loop {
        attempts += 1;
        
        match operation(attempts).await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempts <= config.max_retries => {
                last_error = Some(e);
                let delay = backoff_for_attempt(attempts, &config)?;
                tracing::debug!(
                    "Retry attempt {}/{} after {:?}",
                    attempts,
                    config.max_retries,
                    delay
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }

    Err(MemixError::RetriesExhausted {
        attempts,
        last_error: Box::new(last_error.unwrap_or_else(|| MemixError::Other("Unknown error".to_string()))),
    })
}

/// Calculate backoff delay for a given attempt number.
/// Uses exponential backoff: initial_backoff * 2^(attempt-1)
fn backoff_for_attempt(attempt: u32, config: &RetryConfig) -> Result<Duration> {
    // Calculate multiplier: 2^(attempt-1)
    let Some(multiplier) = 1u32.checked_shl(attempt.saturating_sub(1)) else {
        return Err(MemixError::BackoffOverflow {
            attempt,
            base_delay: config.initial_backoff,
        });
    };

    // Calculate delay: initial_backoff * multiplier
    let delay = config
        .initial_backoff
        .checked_mul(multiplier)
        .map_or(config.max_backoff, |d| d.min(config.max_backoff));

    Ok(delay)
}

/// Execute an operation with retry, but only for specific error types.
/// 
/// # Example
/// ```ignore
/// let result = with_retry_if(config, || async {
///     some_operation().await
/// }, |e| matches!(e, MemixError::LockTimeout { .. })).await;
/// ```
pub async fn with_retry_if<T, F, Fut, P>(config: RetryConfig, operation: F, should_retry: P) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
    P: Fn(&MemixError) -> bool,
{
    let mut attempts = 0;
    let mut last_error: Option<MemixError> = None;

    loop {
        attempts += 1;
        
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if should_retry(&e) && attempts <= config.max_retries => {
                last_error = Some(e);
                let delay = backoff_for_attempt(attempts, &config)?;
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }

    Err(MemixError::RetriesExhausted {
        attempts,
        last_error: Box::new(last_error.unwrap_or_else(|| MemixError::Other("Unknown error".to_string()))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        let config = RetryConfig::quick();
        
        // Attempt 1: 100ms
        let d1 = backoff_for_attempt(1, &config).unwrap();
        assert_eq!(d1, Duration::from_millis(100));
        
        // Attempt 2: 200ms
        let d2 = backoff_for_attempt(2, &config).unwrap();
        assert_eq!(d2, Duration::from_millis(200));
        
        // Attempt 3: 400ms
        let d3 = backoff_for_attempt(3, &config).unwrap();
        assert_eq!(d3, Duration::from_millis(400));
    }

    #[test]
    fn test_backoff_max_cap() {
        let config = RetryConfig::new(10, Duration::from_millis(100), Duration::from_millis(500));
        
        // Attempt 4: would be 800ms, but capped at 500ms
        let d4 = backoff_for_attempt(4, &config).unwrap();
        assert_eq!(d4, Duration::from_millis(500));
    }

    #[tokio::test]
    async fn test_retry_success_on_second_attempt() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();
        
        let result = with_retry(RetryConfig::quick(), move || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    // First attempt fails
                    Err(MemixError::database("lock", true))
                } else {
                    // Second attempt succeeds
                    Ok(42)
                }
            }
        }).await;
        
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let result = with_retry(RetryConfig::new(2, Duration::from_millis(10), Duration::from_millis(100)), || async {
            Err::<i32, _>(MemixError::database("always fails", true))
        }).await;
        
        match result {
            Err(MemixError::RetriesExhausted { attempts, .. }) => {
                assert_eq!(attempts, 3); // 1 initial + 2 retries
            }
            _ => panic!("Expected RetriesExhausted"),
        }
    }
}
