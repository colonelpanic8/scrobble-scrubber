use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Events emitted by the ScrobbleScrubber during operation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrubberEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: ScrubberEventType,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ScrubberEventType {
    /// Scrubber has started running
    Started,
    /// Scrubber has stopped
    Stopped,
    /// A track has been processed (whether changed or not)
    TrackProcessed,
    /// A rule was applied to a track, resulting in changes
    RuleApplied,
    /// An error occurred during processing
    Error,
    /// General informational message
    Info,
    /// Processing cycle completed
    CycleCompleted,
    /// Processing cycle started
    CycleStarted,
}

impl ScrubberEvent {
    pub fn new(event_type: ScrubberEventType, message: String) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            message,
        }
    }

    pub fn started(message: String) -> Self {
        Self::new(ScrubberEventType::Started, message)
    }

    pub fn stopped(message: String) -> Self {
        Self::new(ScrubberEventType::Stopped, message)
    }

    pub fn track_processed(track_name: &str, artist_name: &str) -> Self {
        Self::new(
            ScrubberEventType::TrackProcessed,
            format!("Processed '{track_name}' by '{artist_name}'"),
        )
    }

    pub fn rule_applied(track_name: &str, artist_name: &str, rule_description: &str) -> Self {
        Self::new(
            ScrubberEventType::RuleApplied,
            format!("Applied rule '{rule_description}' to '{track_name}' by '{artist_name}'"),
        )
    }

    pub fn error(message: String) -> Self {
        Self::new(ScrubberEventType::Error, message)
    }

    pub fn info(message: String) -> Self {
        Self::new(ScrubberEventType::Info, message)
    }

    pub fn cycle_started(message: String) -> Self {
        Self::new(ScrubberEventType::CycleStarted, message)
    }

    pub fn cycle_completed(processed_count: usize, applied_count: usize) -> Self {
        Self::new(
            ScrubberEventType::CycleCompleted,
            format!("Processing cycle completed: {processed_count} tracks processed, {applied_count} rules applied"),
        )
    }
}
