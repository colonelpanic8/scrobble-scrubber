use crate::components::TrackProgressState;
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

#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitState {
    pub is_rate_limited: bool,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub retry_after: Option<chrono::DateTime<chrono::Utc>>,
    pub message: String,
    pub delay_seconds: u64,
    pub rate_limit_type: Option<String>,
}

// Using library events directly - no need for duplicate types

pub mod event_formatting {
    use super::RateLimitState;
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
                // Try to extract meaningful information from client events
                let debug_str = format!("{client_event:?}");
                if is_rate_limit_event(&debug_str) {
                    format!(
                        "⚠️ Rate Limited: {}",
                        extract_rate_limit_message(&debug_str)
                    )
                } else {
                    format!("Client: {debug_str}")
                }
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
            ScrubberEventType::ProcessingBatchStarted {
                tracks,
                processing_type,
            } => {
                format!(
                    "Started {}: {} tracks to process",
                    processing_type.display_name(),
                    tracks.len()
                )
            }
            ScrubberEventType::TrackProcessingStarted {
                track,
                track_index,
                total_tracks,
            } => {
                format!(
                    "Processing [{}/{}]: '{}' by '{}'",
                    track_index + 1,
                    total_tracks,
                    track.name,
                    track.artist
                )
            }
            ScrubberEventType::TrackProcessingCompleted {
                track,
                track_index,
                total_tracks,
                success,
                result,
            } => {
                let status = if *success { "✓" } else { "✗" };
                format!(
                    "Completed [{}/{}] {}: '{}' by '{}' - {}",
                    track_index + 1,
                    total_tracks,
                    status,
                    track.name,
                    track.artist,
                    result.summary()
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

    /// Detect if a ClientEvent debug string indicates rate limiting
    fn is_rate_limit_event(debug_str: &str) -> bool {
        let rate_limit_indicators = [
            "RateLimit",
            "rate limit",
            "429",
            "Too Many Requests",
            "Retry-After",
            "quota exceeded",
            "requests per",
            "rate exceeded",
        ];

        let debug_lower = debug_str.to_lowercase();
        rate_limit_indicators
            .iter()
            .any(|indicator| debug_lower.contains(&indicator.to_lowercase()))
    }

    /// Extract a user-friendly rate limit message from ClientEvent debug string
    fn extract_rate_limit_message(debug_str: &str) -> String {
        // Try to extract useful information from debug output
        if debug_str.contains("429") {
            "HTTP 429 - Too Many Requests received from Last.fm API"
        } else if debug_str.to_lowercase().contains("retry-after") {
            "Rate limited by Last.fm API - retry after some time"
        } else if debug_str.to_lowercase().contains("quota") {
            "API quota exceeded"
        } else {
            "Rate limited by Last.fm API"
        }
        .to_string()
    }

    /// Detect rate limiting and create rate limit state from ClientEvent
    pub fn detect_rate_limit_from_event(event: &ScrubberEvent) -> Option<RateLimitState> {
        if let ScrubberEventType::ClientEvent(client_event) = &event.event_type {
            // Check for the new structured rate limiting events first
            match client_event {
                lastfm_edit::ClientEvent::RateLimited {
                    delay_seconds,
                    rate_limit_type,
                    rate_limit_timestamp: _,
                    request,
                } => {
                    let detected_at = event.timestamp;
                    let retry_after =
                        detected_at + chrono::Duration::seconds(*delay_seconds as i64);

                    let message = format!(
                        "Rate limited by Last.fm API ({:?}) - waiting {} seconds{}",
                        rate_limit_type,
                        delay_seconds,
                        request
                            .as_ref()
                            .map(|r| format!(" (request: {})", r.short_description()))
                            .unwrap_or_default()
                    );

                    return Some(RateLimitState {
                        is_rate_limited: true,
                        detected_at,
                        retry_after: Some(retry_after),
                        message,
                        delay_seconds: *delay_seconds,
                        rate_limit_type: Some(format!("{rate_limit_type:?}")),
                    });
                }
                _ => {
                    // Fall back to the legacy debug string detection for other event types
                    let debug_str = format!("{client_event:?}");
                    if is_rate_limit_event(&debug_str) {
                        return Some(RateLimitState {
                            is_rate_limited: true,
                            detected_at: event.timestamp,
                            retry_after: None, // Can't determine from debug string
                            message: extract_rate_limit_message(&debug_str),
                            delay_seconds: 0, // Unknown from debug string
                            rate_limit_type: None,
                        });
                    }
                }
            }
        }
        None
    }

    /// Check if rate limiting has ended from ClientEvent
    pub fn detect_rate_limit_ended_from_event(event: &ScrubberEvent) -> bool {
        if let ScrubberEventType::ClientEvent(client_event) = &event.event_type {
            matches!(
                client_event,
                lastfm_edit::ClientEvent::RateLimitEnded { .. }
            )
        } else {
            false
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
            ScrubberEventType::ProcessingBatchStarted { .. } => "processing_batch_started",
            ScrubberEventType::TrackProcessingStarted { .. } => "track_processing_started",
            ScrubberEventType::TrackProcessingCompleted { .. } => "track_processing_completed",
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
    pub rate_limit_state: Option<RateLimitState>,
}

impl PartialEq for ScrubberState {
    fn eq(&self, other: &Self) -> bool {
        self.status == other.status
            && self.processed_count == other.processed_count
            && self.rules_applied_count == other.rules_applied_count
            && self.current_anchor_timestamp == other.current_anchor_timestamp
            && self.next_cycle_timestamp == other.next_cycle_timestamp
            && self.rate_limit_state == other.rate_limit_state
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
    pub track_progress_state: TrackProgressState, // Track processing progress for UI
    #[allow(dead_code)]
    pub track_cache: TrackCache, // Disk cache for track data
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
                rate_limit_state: None,
            },
            scrubber_instance: None, // No scrubber instance initially
            track_progress_state: TrackProgressState::default(), // Default progress state
            track_cache: TrackCache::load(), // Load cache from disk on startup
        }
    }
}
