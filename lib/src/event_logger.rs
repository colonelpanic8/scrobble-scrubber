use crate::events::ScrubberEvent;
use crate::json_logger::JsonLogger;
use tokio::sync::broadcast;

/// Event-driven JSON logger that subscribes to scrubber events
/// This is now a simple wrapper that spawns the JsonLogger task
pub struct EventLogger {
    json_logger: JsonLogger,
}

impl EventLogger {
    pub fn new(
        log_file_path: String,
        enabled: bool,
        event_receiver: broadcast::Receiver<ScrubberEvent>,
    ) -> Self {
        Self {
            json_logger: JsonLogger::new(log_file_path, enabled, event_receiver),
        }
    }

    /// Start listening to events and logging them
    /// This should be run in a background task
    pub async fn run(&mut self) {
        self.json_logger.run().await;
    }
}
