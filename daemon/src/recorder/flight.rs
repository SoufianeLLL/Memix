use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use parking_lot::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    AstMutation { file: String, nodes_changed: usize },
    MemoryAccessed { memory_id: String },
    IntentDetected { intent_type: String },
    ScorePenalty { reason: String, severity: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightRecord {
    pub timestamp: DateTime<Utc>,
    pub event: SessionEvent,
}

/// Aggregate session analytics computed on-the-fly from the flight timeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionAnalytics {
    /// Total AST mutations recorded in this session
    pub total_mutations: u64,
    /// Unique files that received AST mutations
    pub unique_files_mutated: usize,
    /// Top files by mutation frequency (descending)
    pub hottest_files: Vec<(String, u64)>,
    /// Total memory access events
    pub total_memory_accesses: u64,
    /// Total penalties accrued
    pub total_penalties: u64,
    /// Weighted penalty severity sum
    pub penalty_severity_sum: u64,
    /// Intent type distribution
    pub intent_distribution: HashMap<String, u64>,
    /// Session duration estimate (first→last record span in seconds)
    pub session_duration_secs: i64,
    /// Events per minute (velocity of development activity)
    pub events_per_minute: f64,
}

/// The Flight Recorder provides a high-performance, bounded, lock-efficient
/// timeline of everything Memix observes during a coding session.
///
/// Uses `parking_lot::Mutex` for minimal overhead on macOS (no syscall in
/// the uncontended path) and maintains running analytics counters to avoid
/// recomputing aggregates on every query.
pub struct FlightRecorder {
    timeline: Arc<Mutex<VecDeque<FlightRecord>>>,
    max_capacity: usize,
    /// Running counters updated on every insert — O(1) analytics.
    counters: Arc<Mutex<RunningCounters>>,
}

#[derive(Debug, Clone, Default)]
struct RunningCounters {
    total_mutations: u64,
    total_memory_accesses: u64,
    total_penalties: u64,
    penalty_severity_sum: u64,
    file_mutation_counts: HashMap<String, u64>,
    intent_counts: HashMap<String, u64>,
}

impl FlightRecorder {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            timeline: Arc::new(Mutex::new(VecDeque::with_capacity(max_capacity))),
            max_capacity,
            counters: Arc::new(Mutex::new(RunningCounters::default())),
        }
    }

    /// Records a session event into the ring buffer and updates running counters.
    pub fn record_event(&self, event: SessionEvent) {
        // Update running counters atomically
        {
            let mut counters = self.counters.lock();
            match &event {
                SessionEvent::AstMutation { file, .. } => {
                    counters.total_mutations += 1;
                    *counters.file_mutation_counts.entry(file.clone()).or_default() += 1;
                }
                SessionEvent::MemoryAccessed { .. } => {
                    counters.total_memory_accesses += 1;
                }
                SessionEvent::IntentDetected { intent_type } => {
                    *counters.intent_counts.entry(intent_type.clone()).or_default() += 1;
                }
                SessionEvent::ScorePenalty { severity, .. } => {
                    counters.total_penalties += 1;
                    counters.penalty_severity_sum += *severity as u64;
                }
            }
        }

        // Insert into ring buffer
        let mut buffer = self.timeline.lock();
        if buffer.len() >= self.max_capacity {
            buffer.pop_front();
        }
        buffer.push_back(FlightRecord {
            timestamp: Utc::now(),
            event,
        });
    }

    /// Returns the full session timeline (most recent `max_capacity` events).
    pub fn dump_blackbox(&self) -> Vec<FlightRecord> {
        let buffer = self.timeline.lock();
        buffer.iter().cloned().collect()
    }

    /// Returns events recorded after a given timestamp.
    pub fn events_since(&self, since: DateTime<Utc>) -> Vec<FlightRecord> {
        let buffer = self.timeline.lock();
        buffer.iter()
            .filter(|record| record.timestamp >= since)
            .cloned()
            .collect()
    }

    /// Returns events filtered by a predicate.
    pub fn events_matching<F>(&self, predicate: F) -> Vec<FlightRecord>
    where
        F: Fn(&FlightRecord) -> bool,
    {
        let buffer = self.timeline.lock();
        buffer.iter().filter(|r| predicate(r)).cloned().collect()
    }

    /// Computes full session analytics from running counters + timeline in O(n).
    pub fn analytics(&self) -> SessionAnalytics {
        let counters = self.counters.lock();
        let buffer = self.timeline.lock();

        let mut hottest: Vec<(String, u64)> = counters.file_mutation_counts
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        hottest.sort_by(|a, b| b.1.cmp(&a.1));
        hottest.truncate(10);

        let session_duration_secs = if buffer.len() >= 2 {
            let first = buffer.front().unwrap().timestamp;
            let last = buffer.back().unwrap().timestamp;
            (last - first).num_seconds()
        } else {
            0
        };

        let events_per_minute = if session_duration_secs > 0 {
            (buffer.len() as f64 / session_duration_secs as f64) * 60.0
        } else {
            0.0
        };

        SessionAnalytics {
            total_mutations: counters.total_mutations,
            unique_files_mutated: counters.file_mutation_counts.len(),
            hottest_files: hottest,
            total_memory_accesses: counters.total_memory_accesses,
            total_penalties: counters.total_penalties,
            penalty_severity_sum: counters.penalty_severity_sum,
            intent_distribution: counters.intent_counts.clone(),
            session_duration_secs,
            events_per_minute,
        }
    }

    /// Returns the number of events currently stored.
    pub fn len(&self) -> usize {
        self.timeline.lock().len()
    }

    /// Returns true if no events have been recorded.
    pub fn is_empty(&self) -> bool {
        self.timeline.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_events_and_computes_analytics() {
        let recorder = FlightRecorder::new(100);

        recorder.record_event(SessionEvent::AstMutation {
            file: "src/main.rs".to_string(),
            nodes_changed: 3,
        });
        recorder.record_event(SessionEvent::AstMutation {
            file: "src/main.rs".to_string(),
            nodes_changed: 1,
        });
        recorder.record_event(SessionEvent::AstMutation {
            file: "src/lib.rs".to_string(),
            nodes_changed: 2,
        });
        recorder.record_event(SessionEvent::IntentDetected {
            intent_type: "refactoring".to_string(),
        });
        recorder.record_event(SessionEvent::ScorePenalty {
            reason: "reverted edit".to_string(),
            severity: 5,
        });

        assert_eq!(recorder.len(), 5);

        let analytics = recorder.analytics();
        assert_eq!(analytics.total_mutations, 3);
        assert_eq!(analytics.unique_files_mutated, 2);
        assert_eq!(analytics.total_penalties, 1);
        assert_eq!(analytics.penalty_severity_sum, 5);
        assert_eq!(analytics.intent_distribution.get("refactoring"), Some(&1));
        assert_eq!(analytics.hottest_files.first().map(|f| f.0.as_str()), Some("src/main.rs"));
    }

    #[test]
    fn ring_buffer_evicts_oldest_events() {
        let recorder = FlightRecorder::new(3);
        for i in 0..5 {
            recorder.record_event(SessionEvent::AstMutation {
                file: format!("file_{}.rs", i),
                nodes_changed: 1,
            });
        }
        assert_eq!(recorder.len(), 3);
        let events = recorder.dump_blackbox();
        // Only the last 3 should remain
        assert!(events.iter().all(|r| matches!(&r.event,
            SessionEvent::AstMutation { file, .. } if file.starts_with("file_2") || file.starts_with("file_3") || file.starts_with("file_4")
        )));
    }
}
