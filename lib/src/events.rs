use crate::scrub_action_provider::ScrubActionSuggestion;
use chrono::{DateTime, Utc};
use lastfm_edit::ClientEvent;
use lastfm_edit::Track;
use serde::{Deserialize, Serialize};

// Re-export for backwards compatibility with existing code
// pub use crate::json_logger::{EditInfo, TrackInfo}; // Removed - no longer needed

// Keep ProcessingContext for backwards compatibility with existing event consumers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessingContext {
    pub run_id: String,
    pub batch_id: Option<String>,
    pub track_index: Option<usize>,
    pub batch_size: Option<usize>,
    pub is_artist_processing: bool,
}

// LogTrackInfo was removed - now using Track from lastfm-edit directly

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LogEditInfo {
    pub original_track_name: Option<String>,
    pub original_artist_name: Option<String>,
    pub original_album_name: Option<String>,
    pub original_album_artist_name: Option<String>,
    pub new_track_name: Option<String>,
    pub new_artist_name: Option<String>,
    pub new_album_name: Option<String>,
    pub new_album_artist_name: Option<String>,
}

/// Types of processing that can be performed by the scrubber
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProcessingType {
    /// Processing recent tracks from user's listening history
    Track,
    /// Processing all tracks by a specific artist
    Artist,
    /// Processing all tracks from a specific album
    Album,
    /// Processing tracks from search query results
    Search,
    /// Manual processing triggered by user
    Manual,
    /// Batch processing of multiple items
    Batch,
}

impl ProcessingType {
    /// Get a human-readable display name for the processing type
    pub fn display_name(&self) -> &'static str {
        match self {
            ProcessingType::Track => "Track Processing",
            ProcessingType::Artist => "Artist Processing",
            ProcessingType::Album => "Album Processing",
            ProcessingType::Search => "Search Processing",
            ProcessingType::Manual => "Manual Processing",
            ProcessingType::Batch => "Batch Processing",
        }
    }
}

/// Results of processing a track
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProcessingResult {
    /// No changes were needed or applied
    NoChanges,
    /// Edits were applied directly
    EditsApplied(u32),
    /// Edits were created but require confirmation
    EditsPending(u32),
    /// A new rewrite rule was proposed
    RuleProposed,
    /// Both edits were applied and a rule was proposed
    EditsAppliedAndRuleProposed(u32),
    /// Both edits are pending and a rule was proposed
    EditsPendingAndRuleProposed(u32),
    /// Processing failed with an error
    Failed(String),
    /// Track was skipped because edit requires confirmation
    RequiresConfirmation,
    /// Track was skipped due to dry run mode
    DryRun,
}

impl ProcessingResult {
    /// Get a human-readable summary of the processing result
    pub fn summary(&self) -> String {
        match self {
            ProcessingResult::NoChanges => "no changes".to_string(),
            ProcessingResult::EditsApplied(count) => {
                if *count == 1 {
                    "1 edit applied".to_string()
                } else {
                    format!("{count} edits applied")
                }
            }
            ProcessingResult::EditsPending(count) => {
                if *count == 1 {
                    "1 edit pending".to_string()
                } else {
                    format!("{count} edits pending")
                }
            }
            ProcessingResult::RuleProposed => "proposed rule".to_string(),
            ProcessingResult::EditsAppliedAndRuleProposed(count) => {
                let edit_part = if *count == 1 {
                    "1 edit applied".to_string()
                } else {
                    format!("{count} edits applied")
                };
                format!("{edit_part}, proposed rule")
            }
            ProcessingResult::EditsPendingAndRuleProposed(count) => {
                let edit_part = if *count == 1 {
                    "1 edit pending".to_string()
                } else {
                    format!("{count} edits pending")
                };
                format!("{edit_part}, proposed rule")
            }
            ProcessingResult::Failed(error) => format!("failed: {error}"),
            ProcessingResult::RequiresConfirmation => "requires confirmation".to_string(),
            ProcessingResult::DryRun => "dry run - would apply edit".to_string(),
        }
    }
}

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
    /// Scrubber is sleeping until next cycle
    Sleeping {
        until_next_cycle_seconds: u64,
        sleep_until_timestamp: DateTime<Utc>,
    },
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
    /// Track edit was successful (legacy - use ClientEvent::EditAttempted instead)
    TrackEdited {
        track: Track,
        edit: LogEditInfo,
        context: ProcessingContext,
    },
    /// Track edit failed (legacy - use ClientEvent::EditAttempted instead)
    TrackEditFailed {
        track: Track,
        edit: Option<LogEditInfo>,
        context: ProcessingContext,
        error: String,
    },
    /// Track was skipped (dry run, requires confirmation, etc.)
    TrackSkipped {
        track: Track,
        context: ProcessingContext,
        reason: String,
    },
    /// Client event forwarded from lastfm-edit client
    ClientEvent(ClientEvent),
    /// A pending edit was created requiring confirmation
    PendingEditCreated {
        pending_edit_id: String,
        track: Track,
        edit: LogEditInfo,
        context: ProcessingContext,
    },
    /// Processing batch started with full track list for progress UI
    ProcessingBatchStarted {
        tracks: Vec<Track>,
        processing_type: ProcessingType,
    },
    /// Individual track processing started (for progress UI)
    TrackProcessingStarted {
        track: Track,
        track_index: usize,
        total_tracks: usize,
    },
    /// Individual track processing completed (for progress UI)
    TrackProcessingCompleted {
        track: Track,
        track_index: usize,
        total_tracks: usize,
        success: bool,
        result: ProcessingResult,
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

    pub fn sleeping(until_next_cycle_seconds: u64) -> Self {
        let sleep_until_timestamp =
            Utc::now() + chrono::Duration::seconds(until_next_cycle_seconds as i64);
        Self::new(ScrubberEventType::Sleeping {
            until_next_cycle_seconds,
            sleep_until_timestamp,
        })
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

    pub fn track_edited(track: &Track, edit: &LogEditInfo, context: ProcessingContext) -> Self {
        Self::new(ScrubberEventType::TrackEdited {
            track: track.clone(),
            edit: edit.clone(),
            context,
        })
    }

    pub fn track_edit_failed(
        track: &Track,
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

    pub fn track_skipped(track: &Track, context: ProcessingContext, reason: String) -> Self {
        Self::new(ScrubberEventType::TrackSkipped {
            track: track.clone(),
            context,
            reason,
        })
    }

    pub fn client_event(client_event: ClientEvent) -> Self {
        Self::new(ScrubberEventType::ClientEvent(client_event))
    }

    pub fn pending_edit_created(
        pending_edit_id: String,
        track: &Track,
        edit: &LogEditInfo,
        context: ProcessingContext,
    ) -> Self {
        Self::new(ScrubberEventType::PendingEditCreated {
            pending_edit_id,
            track: track.clone(),
            edit: edit.clone(),
            context,
        })
    }

    pub fn processing_batch_started(tracks: Vec<Track>, processing_type: ProcessingType) -> Self {
        Self::new(ScrubberEventType::ProcessingBatchStarted {
            tracks,
            processing_type,
        })
    }

    pub fn track_processing_started(track: Track, track_index: usize, total_tracks: usize) -> Self {
        Self::new(ScrubberEventType::TrackProcessingStarted {
            track,
            track_index,
            total_tracks,
        })
    }

    pub fn track_processing_completed(
        track: Track,
        track_index: usize,
        total_tracks: usize,
        success: bool,
        result: ProcessingResult,
    ) -> Self {
        Self::new(ScrubberEventType::TrackProcessingCompleted {
            track,
            track_index,
            total_tracks,
            success,
            result,
        })
    }
}
