use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Re-export for event-based logging
pub use crate::json_logger::{LogEditInfo, LogTrackInfo, ProcessingContext};

/// Events emitted by the ScrobbleScrubber during operation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrubberEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: ScrubberEventType,
    pub message: String,
    /// For AnchorUpdated events, contains the anchor timestamp
    pub anchor_timestamp: Option<u64>,
    /// For edit-related events, contains detailed tracking info
    pub edit_data: Option<EditEventData>,
}

/// Detailed data for edit-related events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditEventData {
    pub track: LogTrackInfo,
    pub edit: Option<LogEditInfo>,
    pub context: ProcessingContext,
    pub error: Option<String>,
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
    /// Processing anchor timestamp was updated
    AnchorUpdated,
    /// Tracks were found that need processing
    TracksFound,
    /// Track edit was successful
    TrackEdited,
    /// Track edit failed
    TrackEditFailed,
    /// Track was skipped (dry run, requires confirmation, etc.)
    TrackSkipped,
}

impl ScrubberEvent {
    pub fn new(event_type: ScrubberEventType, message: String) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            message,
            anchor_timestamp: None,
            edit_data: None,
        }
    }

    pub fn new_with_anchor(
        event_type: ScrubberEventType,
        message: String,
        anchor_timestamp: u64,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            message,
            anchor_timestamp: Some(anchor_timestamp),
            edit_data: None,
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
            format!("🎵 '{track_name}' by '{artist_name}'"),
        )
    }

    pub fn track_processed_with_result(track_name: &str, artist_name: &str, result: &str) -> Self {
        Self::new(
            ScrubberEventType::TrackProcessed,
            format!("🎵 '{track_name}' by '{artist_name}' - {result}"),
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

    pub fn anchor_updated(anchor_timestamp: u64, track_name: &str, artist_name: &str) -> Self {
        Self::new_with_anchor(
            ScrubberEventType::AnchorUpdated,
            format!("Processing anchor updated to '{track_name}' by '{artist_name}'"),
            anchor_timestamp,
        )
    }

    pub fn tracks_found(count: usize, anchor_timestamp: u64) -> Self {
        Self::new_with_anchor(
            ScrubberEventType::TracksFound,
            format!("Found {count} tracks to process"),
            anchor_timestamp,
        )
    }

    pub fn track_edited(
        track: &crate::json_logger::LogTrackInfo,
        edit: &crate::json_logger::LogEditInfo,
        context: crate::json_logger::ProcessingContext,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::TrackEdited,
            message: format!("Track '{}' by '{}' was edited", track.name, track.artist),
            anchor_timestamp: None,
            edit_data: Some(EditEventData {
                track: track.clone(),
                edit: Some(edit.clone()),
                context,
                error: None,
            }),
        }
    }

    pub fn track_edit_failed(
        track: &crate::json_logger::LogTrackInfo,
        edit: Option<&crate::json_logger::LogEditInfo>,
        context: crate::json_logger::ProcessingContext,
        error: String,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::TrackEditFailed,
            message: format!(
                "Failed to edit track '{}' by '{}': {}",
                track.name, track.artist, error
            ),
            anchor_timestamp: None,
            edit_data: Some(EditEventData {
                track: track.clone(),
                edit: edit.cloned(),
                context,
                error: Some(error),
            }),
        }
    }

    pub fn track_skipped(
        track: &crate::json_logger::LogTrackInfo,
        context: crate::json_logger::ProcessingContext,
        reason: String,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::TrackSkipped,
            message: format!(
                "Skipped track '{}' by '{}': {}",
                track.name, track.artist, reason
            ),
            anchor_timestamp: None,
            edit_data: Some(EditEventData {
                track: track.clone(),
                edit: None,
                context,
                error: None,
            }),
        }
    }
}
