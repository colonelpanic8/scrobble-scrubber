use chrono::{DateTime, Utc};
use lastfm_edit::{ScrobbleEdit, Track};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// JSON log entry for track edit events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackEditLogEntry {
    /// Timestamp when the event occurred
    pub timestamp: DateTime<Utc>,
    /// Type of event
    pub event_type: TrackEditEventType,
    /// Original track information
    pub original_track: LogTrackInfo,
    /// Edit applied (if any)
    pub edit: Option<LogEditInfo>,
    /// Result of the processing
    pub result: TrackEditResult,
    /// Processing context
    pub context: ProcessingContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrackEditEventType {
    /// Track was processed but no changes were made
    TrackProcessed,
    /// Track was edited successfully
    TrackEdited,
    /// Track edit failed
    TrackEditFailed,
    /// Track was skipped (e.g., already has pending edit)
    TrackSkipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogTrackInfo {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub timestamp: Option<u64>,
    pub playcount: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackEditResult {
    /// Whether the processing was successful
    pub success: bool,
    /// Number of rules applied
    pub rules_applied: usize,
    /// Error message if processing failed
    pub error: Option<String>,
    /// List of rules that were applied
    pub applied_rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessingContext {
    /// Processing run ID for grouping related events
    pub run_id: String,
    /// Batch ID within the run
    pub batch_id: Option<String>,
    /// Track index within the batch
    pub track_index: Option<usize>,
    /// Total tracks in batch
    pub batch_size: Option<usize>,
}

impl From<&Track> for LogTrackInfo {
    fn from(track: &Track) -> Self {
        Self {
            name: track.name.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            album_artist: track.album_artist.clone(),
            timestamp: track.timestamp,
            playcount: track.playcount,
        }
    }
}

impl From<&ScrobbleEdit> for LogEditInfo {
    fn from(edit: &ScrobbleEdit) -> Self {
        Self {
            original_track_name: edit.track_name_original.clone(),
            original_artist_name: edit.artist_name_original.clone(),
            original_album_name: edit.album_name_original.clone(),
            original_album_artist_name: edit.album_artist_name_original.clone(),
            new_track_name: Some(edit.track_name.clone()),
            new_artist_name: Some(edit.artist_name.clone()),
            new_album_name: Some(edit.album_name.clone()),
            new_album_artist_name: Some(edit.album_artist_name.clone()),
        }
    }
}

/// JSON logger for track edit events
pub struct JsonLogger {
    log_file_path: String,
    enabled: bool,
}

impl JsonLogger {
    pub fn new(log_file_path: String, enabled: bool) -> Self {
        Self {
            log_file_path,
            enabled,
        }
    }

    /// Log a track edit event
    pub fn log_track_event(
        &self,
        entry: TrackEditLogEntry,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.enabled {
            return Ok(());
        }

        // Ensure directory exists
        if let Some(parent) = Path::new(&self.log_file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Append JSON line to log file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)?;

        let json_line = serde_json::to_string(&entry)?;
        writeln!(file, "{json_line}")?;

        log::trace!("Logged track edit event to {}", self.log_file_path);
        Ok(())
    }

    /// Log a track processed event (no changes made)
    pub fn log_track_processed(
        &self,
        track: &Track,
        rules_applied: usize,
        applied_rules: Vec<String>,
        context: ProcessingContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry = TrackEditLogEntry {
            timestamp: Utc::now(),
            event_type: TrackEditEventType::TrackProcessed,
            original_track: LogTrackInfo::from(track),
            edit: None,
            result: TrackEditResult {
                success: true,
                rules_applied,
                error: None,
                applied_rules,
            },
            context,
        };

        self.log_track_event(entry)
    }

    /// Log a successful track edit
    pub fn log_track_edited(
        &self,
        track: &Track,
        edit: &ScrobbleEdit,
        rules_applied: usize,
        applied_rules: Vec<String>,
        context: ProcessingContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry = TrackEditLogEntry {
            timestamp: Utc::now(),
            event_type: TrackEditEventType::TrackEdited,
            original_track: LogTrackInfo::from(track),
            edit: Some(LogEditInfo::from(edit)),
            result: TrackEditResult {
                success: true,
                rules_applied,
                error: None,
                applied_rules,
            },
            context,
        };

        self.log_track_event(entry)
    }

    /// Log a failed track edit
    pub fn log_track_edit_failed(
        &self,
        track: &Track,
        edit: Option<&ScrobbleEdit>,
        error: String,
        rules_applied: usize,
        applied_rules: Vec<String>,
        context: ProcessingContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry = TrackEditLogEntry {
            timestamp: Utc::now(),
            event_type: TrackEditEventType::TrackEditFailed,
            original_track: LogTrackInfo::from(track),
            edit: edit.map(LogEditInfo::from),
            result: TrackEditResult {
                success: false,
                rules_applied,
                error: Some(error),
                applied_rules,
            },
            context,
        };

        self.log_track_event(entry)
    }

    /// Log a skipped track
    pub fn log_track_skipped(
        &self,
        track: &Track,
        reason: String,
        context: ProcessingContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry = TrackEditLogEntry {
            timestamp: Utc::now(),
            event_type: TrackEditEventType::TrackSkipped,
            original_track: LogTrackInfo::from(track),
            edit: None,
            result: TrackEditResult {
                success: true,
                rules_applied: 0,
                error: Some(reason),
                applied_rules: vec![],
            },
            context,
        };

        self.log_track_event(entry)
    }
}
