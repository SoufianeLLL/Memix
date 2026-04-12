//! Agent hooks system for command interception and token optimization.
//!
//! Provides transparent command rewriting for AI agents (Claude Code, Cursor, Windsurf).
//! When an agent executes a shell command, the hook intercepts it, applies
//! TOML filters, and returns optimized output - saving 60-90% tokens.
//!
//! # Architecture
//!
//! Memix hooks call the daemon API directly:
//! - Zero subprocess overhead
//! - Shared filter registry with terminal proxy
//! - Unified token savings tracking
//!
//! # Supported Agents
//!
//! | Agent | Mechanism | Hook Type |
//! |-------|-----------|-----------|
//! | Claude Code | Shell hook (PreToolUse) | Transparent rewrite |
//! | Cursor | Shell hook (preToolUse) | Transparent rewrite |
//! | Windsurf | Rules file (.windsurfrules) | Prompt-level guidance |
//! | VS Code Copilot | Extension API | Transparent rewrite |

pub mod registry;
pub mod installer;
pub mod claude;
pub mod cursor;
pub mod windsurf;

pub use registry::{CommandRegistry, RewriteResult, Classification, COMMAND_REGISTRY};
pub use installer::HookInstaller;
