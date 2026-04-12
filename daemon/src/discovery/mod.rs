//! Command discovery and pattern learning system.
//!
//! Automatically detects new command patterns from usage history and suggests
//! filter configurations. Learns from developer workflow to improve
//! token optimization over time.
//!
//! # Architecture
//!
//! 1. **Pattern Detection**: Analyzes command execution history
//! 2. **Frequency Tracking**: Counts command occurrences
//! 3. **Suggestion Engine**: Proposes new filter rules
//! 4. **Confidence Scoring**: Rates pattern reliability
//!
//! # Usage
//!
//! ```ignore
//! use crate::discovery::DiscoveryEngine;
//!
//! let engine = DiscoveryEngine::new();
//! engine.record("my-tool --flag file.txt");
//! let suggestions = engine.suggest_filters();
//! ```

pub mod detector;
pub mod suggester;
pub mod tracker;

pub use detector::PatternDetector;
pub use suggester::FilterSuggester;
pub use tracker::CommandTracker;
