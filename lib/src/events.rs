use crate::scrub_action_provider::ScrubActionSuggestion;
use chrono::{DateTime, Utc};
use lastfm_edit::Track;
use serde::{Deserialize, Serialize};

// Re-export for event-based logging
pub use crate::json_logger::{LogEditInfo, LogTrackInfo, ProcessingContext};

/// Events emitted by the ScrobbleScrubber during operation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScrubberEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: ScrubberEventType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ScrubberEventType {
    /// Scrubber has started running
    Started(String),
    /// Scrubber has stopped
    Stopped(String),
    /// A track has been processed with suggestions applied
    TrackProcessed {
        track: Track,
        suggestions: Vec<ScrubActionSuggestion>,
        result: String,
    },
    /// A rule was applied to a track
    RuleApplied {
        track: Track,
        suggestion: ScrubActionSuggestion,
        description: String,
    },
    /// An error occurred during processing
    Error(String),
    /// General informational message
    Info(String),
    /// Processing cycle completed
    CycleCompleted {
        processed_count: usize,
        applied_count: usize,
    },
    /// Processing cycle started
    CycleStarted(String),
    /// Processing anchor timestamp was updated
    AnchorUpdated { anchor_timestamp: u64, track: Track },
    /// Tracks were found that need processing
    TracksFound { count: usize, anchor_timestamp: u64 },
    /// Track edit was successful
    TrackEdited {
        track: LogTrackInfo,
        edit: LogEditInfo,
        context: ProcessingContext,
    },
    /// Track edit failed
    TrackEditFailed {
        track: LogTrackInfo,
        edit: Option<LogEditInfo>,
        context: ProcessingContext,
        error: String,
    },
    /// Track was skipped (dry run, requires confirmation, etc.)
    TrackSkipped {
        track: LogTrackInfo,
        context: ProcessingContext,
        reason: String,
    },
}

impl ScrubberEvent {
    pub fn new(event_type: ScrubberEventType) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
        }
    }

    pub fn started(message: String) -> Self {
        Self::new(ScrubberEventType::Started(message))
    }

    pub fn stopped(message: String) -> Self {
        Self::new(ScrubberEventType::Stopped(message))
    }

    pub fn track_processed(
        track: Track,
        suggestions: Vec<ScrubActionSuggestion>,
        result: String,
    ) -> Self {
        Self::new(ScrubberEventType::TrackProcessed {
            track,
            suggestions,
            result,
        })
    }

    pub fn track_processed_with_result(track_name: &str, artist_name: &str, result: &str) -> Self {
        // Create a minimal track for backwards compatibility
        let track = Track {
            name: track_name.to_string(),
            artist: artist_name.to_string(),
            album: None,
            album_artist: None,
            timestamp: None,
            playcount: 0,
        };
        Self::new(ScrubberEventType::TrackProcessed {
            track,
            suggestions: vec![],
            result: result.to_string(),
        })
    }

    pub fn rule_applied(
        track: Track,
        suggestion: ScrubActionSuggestion,
        description: String,
    ) -> Self {
        Self::new(ScrubberEventType::RuleApplied {
            track,
            suggestion,
            description,
        })
    }

    pub fn error(message: String) -> Self {
        Self::new(ScrubberEventType::Error(message))
    }

    pub fn info(message: String) -> Self {
        Self::new(ScrubberEventType::Info(message))
    }

    pub fn cycle_started(message: String) -> Self {
        Self::new(ScrubberEventType::CycleStarted(message))
    }

    pub fn cycle_completed(processed_count: usize, applied_count: usize) -> Self {
        Self::new(ScrubberEventType::CycleCompleted {
            processed_count,
            applied_count,
        })
    }

    pub fn anchor_updated(anchor_timestamp: u64, track: Track) -> Self {
        Self::new(ScrubberEventType::AnchorUpdated {
            anchor_timestamp,
            track,
        })
    }

    // Helper for backwards compatibility with old scrubber calls
    pub fn anchor_updated_from_names(
        anchor_timestamp: u64,
        track_name: &str,
        artist_name: &str,
    ) -> Self {
        let track = Track {
            name: track_name.to_string(),
            artist: artist_name.to_string(),
            album: None,
            album_artist: None,
            timestamp: Some(anchor_timestamp),
            playcount: 0,
        };
        Self::anchor_updated(anchor_timestamp, track)
    }

    pub fn tracks_found(count: usize, anchor_timestamp: u64) -> Self {
        Self::new(ScrubberEventType::TracksFound {
            count,
            anchor_timestamp,
        })
    }

    pub fn track_edited(
        track: &LogTrackInfo,
        edit: &LogEditInfo,
        context: ProcessingContext,
    ) -> Self {
        Self::new(ScrubberEventType::TrackEdited {
            track: track.clone(),
            edit: edit.clone(),
            context,
        })
    }

    pub fn track_edit_failed(
        track: &LogTrackInfo,
        edit: Option<&LogEditInfo>,
        context: ProcessingContext,
        error: String,
    ) -> Self {
        Self::new(ScrubberEventType::TrackEditFailed {
            track: track.clone(),
            edit: edit.cloned(),
            context,
            error,
        })
    }

    pub fn track_skipped(track: &LogTrackInfo, context: ProcessingContext, reason: String) -> Self {
        Self::new(ScrubberEventType::TrackSkipped {
            track: track.clone(),
            context,
            reason,
        })
    }
}
