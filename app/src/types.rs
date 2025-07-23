use ::scrobble_scrubber::config::ScrobbleScrubberConfig;
use ::scrobble_scrubber::persistence::FileStorage;
use ::scrobble_scrubber::rewrite::RewriteRule;
use ::scrobble_scrubber::track_cache::TrackCache;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    RuleWorkshop,
    RewriteRules,
    ScrobbleScrubber,
    PendingItems,
    CacheManagement,
}

#[derive(Clone, Debug)]
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
pub enum ScrubberStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScrubberEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: ScrubberEventType,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScrubberEventType {
    Started,
    Stopped,
    #[allow(dead_code)]
    TrackProcessed,
    #[allow(dead_code)]
    RuleApplied,
    Error,
    Info,
}

#[derive(Clone, Debug)]
pub struct ScrubberState {
    pub status: ScrubberStatus,
    pub events: Vec<ScrubberEvent>,
    pub processed_count: usize,
    pub rules_applied_count: usize,
    pub event_sender: Option<Arc<broadcast::Sender<ScrubberEvent>>>,
}

#[derive(Clone)]
pub struct AppState {
    pub logged_in: bool,
    pub session: Option<String>,         // Serialized LastFmEditSession
    pub recent_tracks: TrackSourceState, // Recent tracks with enable/disable
    pub artist_tracks: std::collections::HashMap<String, TrackSourceState>, // Artist tracks by artist name
    pub current_rule: RewriteRule,
    pub show_all_tracks: bool, // Toggle to show all tracks or only matching ones
    pub current_page: u32,     // Current page for pagination (for recent tracks)
    pub active_page: Page,     // Current active page
    pub config: Option<ScrobbleScrubberConfig>, // Loaded configuration
    pub storage: Option<Arc<Mutex<FileStorage>>>, // Persistence storage
    pub saved_rules: Vec<RewriteRule>, // Rules loaded from storage
    pub scrubber_state: ScrubberState, // Scrobble scrubber state and observability
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
            active_page: Page::RuleWorkshop,
            config: None,
            storage: None,
            saved_rules: Vec::new(),
            scrubber_state: ScrubberState {
                status: ScrubberStatus::Stopped,
                events: Vec::new(),
                processed_count: 0,
                rules_applied_count: 0,
                event_sender: None,
            },
            track_cache: TrackCache::load(), // Load cache from disk on startup
        }
    }
}
