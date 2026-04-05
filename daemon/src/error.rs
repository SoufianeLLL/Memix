//! Typed error handling with retry information.
//! Based on patterns from claude-code-rust for better error recovery.

use std::env::VarError;
use std::fmt::{Display, Formatter};
use std::time::Duration;

/// Typed error enum that carries retry information.
/// This allows automatic retry for transient failures.
#[derive(Debug)]
pub enum MemixError {
    /// Database-related errors (SQLite)
    Database {
        message: String,
        retryable: bool,
    },
    /// HTTP/API errors
    Http {
        status: u16,
        message: String,
        retryable: bool,
    },
    /// IO errors (file system, network)
    Io(std::io::Error),
    /// JSON serialization/deserialization errors
    Json(serde_json::Error),
    /// Resource not found
    NotFound {
        resource: String,
    },
    /// Invalid input from user
    InvalidInput {
        field: String,
        reason: String,
    },
    /// Missing credentials
    MissingCredentials {
        provider: &'static str,
        env_vars: &'static [&'static str],
    },
    /// Configuration error
    Config {
        message: String,
    },
    /// Lock timeout (database busy)
    LockTimeout {
        resource: String,
        timeout_ms: u64,
    },
    /// Retries exhausted
    RetriesExhausted {
        attempts: u32,
        last_error: Box<MemixError>,
    },
    /// Backoff overflow (too many retries)
    BackoffOverflow {
        attempt: u32,
        base_delay: Duration,
    },
    /// Generic error with message
    Other(String),
}

impl MemixError {
    /// Create a database error
    pub fn database(message: impl Into<String>, retryable: bool) -> Self {
        Self::Database {
            message: message.into(),
            retryable,
        }
    }

    /// Create an HTTP error
    pub fn http(status: u16, message: impl Into<String>, retryable: bool) -> Self {
        Self::Http {
            status,
            message: message.into(),
            retryable,
        }
    }

    /// Create a not found error
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
        }
    }

    /// Create an invalid input error
    pub fn invalid_input(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidInput {
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Create a config error
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
        }
    }

    /// Create a lock timeout error
    pub fn lock_timeout(resource: impl Into<String>, timeout_ms: u64) -> Self {
        Self::LockTimeout {
            resource: resource.into(),
            timeout_ms,
        }
    }

    /// Check if this error is retryable
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Database { retryable, .. } => *retryable,
            Self::Http { retryable, .. } => *retryable,
            Self::Io(e) => {
                // Retry on connection reset, would block, timed out, or not connected
                matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::NotConnected
                )
            }
            Self::LockTimeout { .. } => true,
            Self::RetriesExhausted { last_error, .. } => last_error.is_retryable(),
            Self::MissingCredentials { .. }
            | Self::NotFound { .. }
            | Self::InvalidInput { .. }
            | Self::Config { .. }
            | Self::Json(_)
            | Self::BackoffOverflow { .. }
            | Self::Other(_) => false,
        }
    }

    /// Get a user-friendly error message
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::Database { message, .. } => format!("Database error: {}", message),
            Self::Http { status, message, .. } => format!("HTTP {}: {}", status, message),
            Self::Io(e) => format!("IO error: {}", e),
            Self::Json(e) => format!("JSON error: {}", e),
            Self::NotFound { resource } => format!("Not found: {}", resource),
            Self::InvalidInput { field, reason } => format!("Invalid {}: {}", field, reason),
            Self::MissingCredentials { provider, env_vars } => {
                format!(
                    "Missing {} credentials. Set {}",
                    provider,
                    env_vars.join(" or ")
                )
            }
            Self::Config { message } => format!("Configuration error: {}", message),
            Self::LockTimeout { resource, timeout_ms } => {
                format!("Lock timeout on {} after {}ms", resource, timeout_ms)
            }
            Self::RetriesExhausted { attempts, last_error } => {
                format!("Failed after {} attempts: {}", attempts, last_error)
            }
            Self::BackoffOverflow { attempt, base_delay } => {
                format!("Retry backoff overflow on attempt {} with base delay {:?}", attempt, base_delay)
            }
            Self::Other(message) => message.clone(),
        }
    }
}

impl Display for MemixError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for MemixError {}

// --- From implementations for common error types ---

impl From<std::io::Error> for MemixError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for MemixError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<VarError> for MemixError {
    fn from(e: VarError) -> Self {
        Self::Config {
            message: format!("Environment variable error: {}", e),
        }
    }
}

impl From<String> for MemixError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for MemixError {
    fn from(message: &str) -> Self {
        Self::Other(message.to_string())
    }
}

impl From<anyhow::Error> for MemixError {
    fn from(e: anyhow::Error) -> Self {
        // Convert anyhow error to string - simpler and more reliable
        Self::Other(e.to_string())
    }
}

/// Result type alias using MemixError
pub type Result<T> = std::result::Result<T, MemixError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_database() {
        let retryable = MemixError::database("lock", true);
        assert!(retryable.is_retryable());

        let not_retryable = MemixError::database("corruption", false);
        assert!(!not_retryable.is_retryable());
    }

    #[test]
    fn test_is_retryable_io() {
        let retryable = MemixError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "reset",
        ));
        assert!(retryable.is_retryable());

        let not_retryable = MemixError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        assert!(!not_retryable.is_retryable());
    }

    #[test]
    fn test_user_message() {
        let err = MemixError::not_found("brain.db");
        assert_eq!(err.user_message(), "Not found: brain.db");
    }
}
