use chrono::{DateTime, Utc};
use lastfm_edit::ClientEvent;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use tokio::sync::broadcast;

use crate::config::ScrubberConfig;
use crate::events::{ScrubberEvent, ScrubberEventType};

/// JSON log entry for scrobble edit attempts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditLogEntry {
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub error_message: Option<String>,
    pub duration_ms: u64,
    pub edit: EditDetails,
}

/// Detailed edit information extracted from ExactScrobbleEdit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditDetails {
    pub timestamp: u64,
    pub edit_all: bool,
    pub original: TrackMetadata,
    pub new: TrackMetadata,
}

/// Track metadata for before/after comparison
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub album_artist_name: String,
}

/// JSON logger that focuses on ClientEvent::EditAttempted events
pub struct JsonLogger {
    log_file_path: String,
    enabled: bool,
    receiver: broadcast::Receiver<ScrubberEvent>,
    config: ScrubberConfig,
}

impl JsonLogger {
    pub fn new(
        log_file_path: String,
        enabled: bool,
        receiver: broadcast::Receiver<ScrubberEvent>,
        config: ScrubberConfig,
    ) -> Self {
        Self {
            log_file_path,
            enabled,
            receiver,
            config,
        }
    }

    /// Start listening for events and logging them
    pub async fn run(&mut self) {
        while let Ok(event) = self.receiver.recv().await {
            if let Err(e) = self.process_event(&event) {
                log::warn!("Failed to log event: {e}");
            }
        }
    }

    /// Process a single ScrubberEvent and log if it contains ClientEvent::EditAttempted
    fn process_event(&self, event: &ScrubberEvent) -> Result<(), Box<dyn std::error::Error>> {
        if !self.enabled {
            return Ok(());
        }

        // Defensive check: Do not log during dry run mode
        if self.config.dry_run {
            log::trace!("JSON logger: Skipping log during dry run mode");
            return Ok(());
        }

        // Only process ClientEvent::EditAttempted variants
        if let ScrubberEventType::ClientEvent(ClientEvent::EditAttempted {
            edit,
            success,
            error_message,
            duration_ms,
        }) = &event.event_type
        {
            let log_entry = EditLogEntry {
                timestamp: event.timestamp,
                success: *success,
                error_message: error_message.clone(),
                duration_ms: *duration_ms,
                edit: EditDetails {
                    timestamp: edit.timestamp,
                    edit_all: edit.edit_all,
                    original: TrackMetadata {
                        track_name: edit.track_name_original.clone(),
                        artist_name: edit.artist_name_original.clone(),
                        album_name: edit.album_name_original.clone(),
                        album_artist_name: edit.album_artist_name_original.clone(),
                    },
                    new: TrackMetadata {
                        track_name: edit.track_name.clone(),
                        artist_name: edit.artist_name.clone(),
                        album_name: edit.album_name.clone(),
                        album_artist_name: edit.album_artist_name.clone(),
                    },
                },
            };

            self.write_log_entry(&log_entry)?;
        }

        Ok(())
    }

    /// Write a log entry to the file
    fn write_log_entry(&self, entry: &EditLogEntry) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure directory exists
        if let Some(parent) = Path::new(&self.log_file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Append JSON line to log file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)?;

        let json_line = serde_json::to_string(entry)?;
        writeln!(file, "{json_line}")?;

        log::trace!("Logged edit attempt to {}", self.log_file_path);
        Ok(())
    }
}
