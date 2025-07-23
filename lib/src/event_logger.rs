use crate::events::{ScrubberEvent, ScrubberEventType};
use crate::json_logger::{JsonLogger, TrackEditEventType, TrackEditLogEntry, TrackEditResult};
use tokio::sync::broadcast;

/// Event-driven JSON logger that subscribes to scrubber events
pub struct EventLogger {
    json_logger: JsonLogger,
    receiver: broadcast::Receiver<ScrubberEvent>,
}

impl EventLogger {
    pub fn new(
        log_file_path: String,
        enabled: bool,
        event_receiver: broadcast::Receiver<ScrubberEvent>,
    ) -> Self {
        Self {
            json_logger: JsonLogger::new(log_file_path, enabled),
            receiver: event_receiver,
        }
    }

    /// Start listening to events and logging them
    /// This should be run in a background task
    pub async fn run(&mut self) {
        while let Ok(event) = self.receiver.recv().await {
            if let Err(e) = self.process_event(&event).await {
                log::warn!("Failed to log event: {e}");
            }
        }
    }

    async fn process_event(&self, event: &ScrubberEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Only log edit-related events by default
        match event.event_type {
            ScrubberEventType::TrackEdited => {
                if let Some(edit_data) = &event.edit_data {
                    let entry = TrackEditLogEntry {
                        timestamp: event.timestamp,
                        event_type: TrackEditEventType::TrackEdited,
                        original_track: edit_data.track.clone(),
                        edit: edit_data.edit.clone(),
                        result: TrackEditResult {
                            success: true,
                            rules_applied: 1,
                            error: None,
                            applied_rules: vec!["applied_edit".to_string()],
                        },
                        context: edit_data.context.clone(),
                    };
                    self.json_logger.log_track_event(entry)?;
                }
            }
            ScrubberEventType::TrackEditFailed => {
                if let Some(edit_data) = &event.edit_data {
                    let entry = TrackEditLogEntry {
                        timestamp: event.timestamp,
                        event_type: TrackEditEventType::TrackEditFailed,
                        original_track: edit_data.track.clone(),
                        edit: edit_data.edit.clone(),
                        result: TrackEditResult {
                            success: false,
                            rules_applied: 0,
                            error: edit_data.error.clone(),
                            applied_rules: vec![],
                        },
                        context: edit_data.context.clone(),
                    };
                    self.json_logger.log_track_event(entry)?;
                }
            }
            ScrubberEventType::TrackSkipped => {
                if let Some(edit_data) = &event.edit_data {
                    let entry = TrackEditLogEntry {
                        timestamp: event.timestamp,
                        event_type: TrackEditEventType::TrackSkipped,
                        original_track: edit_data.track.clone(),
                        edit: edit_data.edit.clone(),
                        result: TrackEditResult {
                            success: false,
                            rules_applied: 0,
                            error: None,
                            applied_rules: vec![],
                        },
                        context: edit_data.context.clone(),
                    };
                    self.json_logger.log_track_event(entry)?;
                }
            }
            // Ignore other event types for JSON logging (only log edits by default)
            _ => {}
        }
        Ok(())
    }
}
