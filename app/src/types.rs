use ::scrobble_scrubber::config::ScrobbleScrubberConfig;
use ::scrobble_scrubber::events::ScrubberEvent;
use ::scrobble_scrubber::persistence::FileStorage;
use ::scrobble_scrubber::rewrite::RewriteRule;
use ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
use ::scrobble_scrubber::scrubber::ScrobbleScrubber;
use ::scrobble_scrubber::track_cache::TrackCache;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSourceState {
    pub enabled: bool,
    // tracks are now stored only in the cache, not duplicated here
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreviewType {
    CurrentRule,   // Only apply the rule being edited
    AllSavedRules, // Apply all saved rules collectively
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // These will be used when proper scrubber is implemented
pub enum ScrubberStatus {
    Stopped,
    Starting,
    Running,
    Sleeping {
        until_timestamp: chrono::DateTime<chrono::Utc>,
    },
    Stopping,
    Error(String),
}

// Using library events directly - no need for duplicate types

pub mod event_formatting {
    use ::scrobble_scrubber::events::{ScrubberEvent, ScrubberEventType};

    /// Format a library event for display in the UI
    pub fn format_event_message(event: &ScrubberEvent) -> String {
        match &event.event_type {
            ScrubberEventType::Started(msg) => msg.clone(),
            ScrubberEventType::Stopped(msg) => msg.clone(),
            ScrubberEventType::Sleeping {
                until_next_cycle_seconds,
                sleep_until_timestamp,
            } => {
                let now = chrono::Utc::now();
                let remaining_seconds = (*sleep_until_timestamp - now).num_seconds().max(0) as u64;

                if remaining_seconds > 0 {
                    format!("💤 Sleeping ({remaining_seconds}s remaining)")
                } else {
                    format!("Sleeping for {until_next_cycle_seconds} seconds until next processing cycle")
                }
            }
            ScrubberEventType::TrackProcessed { track, result, .. } => {
                format!("'{}' by '{}' - {}", track.name, track.artist, result)
            }
            ScrubberEventType::RuleApplied {
                track, description, ..
            } => {
                format!(
                    "Applied rule '{}' to '{}' by '{}'",
                    description, track.name, track.artist
                )
            }
            ScrubberEventType::Error(msg) => msg.clone(),
            ScrubberEventType::Info(msg) => msg.clone(),
            ScrubberEventType::CycleCompleted {
                processed_count,
                applied_count,
            } => {
                format!("Processing cycle completed: {processed_count} tracks processed, {applied_count} rules applied")
            }
            ScrubberEventType::CycleStarted(msg) => msg.clone(),
            ScrubberEventType::AnchorUpdated {
                anchor_timestamp: _,
                track,
            } => {
                format!(
                    "Processing anchor updated to '{}' by '{}'",
                    track.name, track.artist
                )
            }
            ScrubberEventType::TracksFound { count, .. } => {
                format!("Found {count} tracks to process")
            }
            ScrubberEventType::TrackEdited { track, .. } => {
                format!("Edited track '{}' by '{}'", track.artist, track.name)
            }
            ScrubberEventType::TrackEditFailed { track, error, .. } => {
                format!(
                    "Failed to edit '{}' by '{}': {}",
                    track.artist, track.name, error
                )
            }
            ScrubberEventType::TrackSkipped { track, reason, .. } => {
                format!("Skipped '{}' by '{}': {}", track.artist, track.name, reason)
            }
            ScrubberEventType::ClientEvent(client_event) => {
                format!("Client: {client_event:?}")
            }
            ScrubberEventType::PendingEditCreated {
                pending_edit_id,
                track,
                ..
            } => {
                format!(
                    "Created pending edit (ID: {}) for '{}' by '{}'",
                    pending_edit_id, track.name, track.artist
                )
            }
        }
    }

    /// Get the anchor timestamp from an event if it has one
    #[allow(dead_code)] // Will be used when proper scrubber is implemented
    pub fn get_anchor_timestamp(event: &ScrubberEvent) -> Option<u64> {
        match &event.event_type {
            ScrubberEventType::AnchorUpdated {
                anchor_timestamp, ..
            } => Some(*anchor_timestamp),
            ScrubberEventType::TracksFound {
                anchor_timestamp, ..
            } => Some(*anchor_timestamp),
            _ => None,
        }
    }

    /// Get a simple event type string for categorization
    pub fn get_event_category(event: &ScrubberEvent) -> &'static str {
        match &event.event_type {
            ScrubberEventType::Started(_) => "started",
            ScrubberEventType::Stopped(_) => "stopped",
            ScrubberEventType::Sleeping { .. } => "sleeping",
            ScrubberEventType::TrackProcessed { .. } => "track_processed",
            ScrubberEventType::RuleApplied { .. } => "rule_applied",
            ScrubberEventType::Error(_) => "error",
            ScrubberEventType::Info(_) => "info",
            ScrubberEventType::CycleCompleted { .. } => "cycle_completed",
            ScrubberEventType::CycleStarted(_) => "cycle_started",
            ScrubberEventType::AnchorUpdated { .. } => "anchor_updated",
            ScrubberEventType::TracksFound { .. } => "tracks_found",
            ScrubberEventType::TrackEdited { .. } => "track_edited",
            ScrubberEventType::TrackEditFailed { .. } => "track_edit_failed",
            ScrubberEventType::TrackSkipped { .. } => "track_skipped",
            ScrubberEventType::ClientEvent(_) => "client_event",
            ScrubberEventType::PendingEditCreated { .. } => "pending_edit_created",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScrubberState {
    pub status: ScrubberStatus,
    pub events: Vec<ScrubberEvent>,
    pub processed_count: usize,
    pub rules_applied_count: usize,
    pub event_sender: Option<Arc<broadcast::Sender<ScrubberEvent>>>,
    pub current_anchor_timestamp: Option<u64>,
    pub next_cycle_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

impl PartialEq for ScrubberState {
    fn eq(&self, other: &Self) -> bool {
        self.status == other.status
            && self.processed_count == other.processed_count
            && self.rules_applied_count == other.rules_applied_count
            && self.current_anchor_timestamp == other.current_anchor_timestamp
            && self.next_cycle_timestamp == other.next_cycle_timestamp
            && self.events.len() == other.events.len()
    }
}

pub type GlobalScrubber = ScrobbleScrubber<FileStorage, RewriteRulesScrubActionProvider>;

#[derive(Clone)]
pub struct AppState {
    pub logged_in: bool,
    pub session: Option<String>,         // Serialized LastFmEditSession
    pub recent_tracks: TrackSourceState, // Recent tracks with enable/disable
    pub artist_tracks: std::collections::HashMap<String, TrackSourceState>, // Artist tracks by artist name
    pub current_rule: RewriteRule,
    pub show_all_tracks: bool, // Toggle to show all tracks or only matching ones
    pub current_page: u32,     // Current page for pagination (for recent tracks)
    pub config: Option<ScrobbleScrubberConfig>, // Loaded configuration
    pub storage: Option<Arc<Mutex<FileStorage>>>, // Persistence storage
    pub saved_rules: Vec<RewriteRule>, // Rules loaded from storage
    pub scrubber_state: ScrubberState, // Scrobble scrubber state and observability
    pub scrubber_instance: Option<Arc<tokio::sync::Mutex<GlobalScrubber>>>, // Global scrubber instance
    #[allow(dead_code)]
    pub track_cache: TrackCache,                     // Disk cache for track data
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            logged_in: false,
            session: None,
            recent_tracks: TrackSourceState { enabled: true },
            artist_tracks: std::collections::HashMap::new(),
            current_rule: RewriteRule::new(),
            show_all_tracks: true, // Default to showing all tracks
            current_page: 1,       // Start at page 1
            config: None,
            storage: None,
            saved_rules: Vec::new(),
            scrubber_state: ScrubberState {
                status: ScrubberStatus::Stopped,
                events: Vec::new(),
                processed_count: 0,
                rules_applied_count: 0,
                event_sender: None,
                current_anchor_timestamp: None,
                next_cycle_timestamp: None,
            },
            scrubber_instance: None,         // No scrubber instance initially
            track_cache: TrackCache::load(), // Load cache from disk on startup
        }
    }
}
