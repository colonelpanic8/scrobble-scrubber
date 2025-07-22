use lastfm_edit::Track;
use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::persistence::FileStorage;
use scrobble_scrubber::rewrite::RewriteRule;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableTrack {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub timestamp: Option<u64>,
    pub playcount: u32,
}

impl From<Track> for SerializableTrack {
    fn from(track: Track) -> Self {
        Self {
            name: track.name,
            artist: track.artist,
            album: track.album,
            timestamp: track.timestamp,
            playcount: track.playcount,
        }
    }
}

impl From<SerializableTrack> for Track {
    fn from(strack: SerializableTrack) -> Self {
        Self {
            name: strack.name,
            artist: strack.artist,
            album: strack.album,
            timestamp: strack.timestamp,
            playcount: strack.playcount,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    RuleWorkshop,
    RewriteRules,
}

#[derive(Clone, Debug)]
pub struct TrackSourceState {
    pub enabled: bool,
    pub tracks: Vec<SerializableTrack>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreviewType {
    CurrentRule,   // Only apply the rule being edited
    AllSavedRules, // Apply all saved rules collectively
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            logged_in: false,
            session: None,
            recent_tracks: TrackSourceState {
                enabled: true,
                tracks: Vec::new(),
            },
            artist_tracks: std::collections::HashMap::new(),
            current_rule: RewriteRule::new(),
            show_all_tracks: true, // Default to showing all tracks
            current_page: 1,       // Start at page 1
            active_page: Page::RuleWorkshop,
            config: None,
            storage: None,
            saved_rules: Vec::new(),
        }
    }
}
