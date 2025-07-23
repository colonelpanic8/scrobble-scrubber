use ::scrobble_scrubber::config::ScrobbleScrubberConfig;
use ::scrobble_scrubber::persistence::{FileStorage, StateStorage};
use ::scrobble_scrubber::rewrite::RewriteRule;
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

mod components;
mod error_utils;
mod server_functions;
mod types;
mod utils;

use components::*;
use server_functions::*;
use types::*;

// Helper function to initialize config and storage
async fn initialize_app_state() -> Result<
    (
        ScrobbleScrubberConfig,
        Option<Arc<Mutex<FileStorage>>>,
        Vec<RewriteRule>,
    ),
    String,
> {
    let config =
        ScrobbleScrubberConfig::load().map_err(|e| format!("Failed to load config: {e}"))?;

    let state_file = config.storage.state_file.clone();

    let storage =
        FileStorage::new(&state_file).map_err(|e| format!("Failed to initialize storage: {e}"))?;

    let saved_rules = storage
        .load_rewrite_rules_state()
        .await
        .map(|rules_state| rules_state.rewrite_rules)
        .unwrap_or_default();

    Ok((config, Some(Arc::new(Mutex::new(storage))), saved_rules))
}

// Helper function to attempt auto-login
async fn attempt_auto_login(config: Option<&ScrobbleScrubberConfig>) -> Option<String> {
    let (username, password) = match config {
        Some(config) => (
            config.lastfm.username.clone(),
            config.lastfm.password.clone(),
        ),
        None => (
            std::env::var("SCROBBLE_SCRUBBER_LASTFM_USERNAME").unwrap_or_default(),
            std::env::var("SCROBBLE_SCRUBBER_LASTFM_PASSWORD").unwrap_or_default(),
        ),
    };

    if username.trim().is_empty() || password.trim().is_empty() {
        return None;
    }

    login_to_lastfm(username.trim().to_string(), password.trim().to_string())
        .await
        .map_err(|e| eprintln!("Auto-login failed: {e}"))
        .ok()
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut state = use_signal(AppState::default);

    // Initialize config and storage
    use_effect(move || {
        spawn(async move {
            match initialize_app_state().await {
                Ok((config, storage, saved_rules)) => {
                    state.with_mut(|s| {
                        s.config = Some(config);
                        s.storage = storage;
                        s.saved_rules = saved_rules;

                        // Initialize artist track states for cached artists (enabled by default)
                        for artist_name in s.track_cache.artist_tracks.keys() {
                            s.artist_tracks
                                .insert(artist_name.clone(), TrackSourceState { enabled: true });
                        }

                        // With the new cache structure, we default to page 1 if tracks exist
                        if !s.track_cache.recent_tracks.is_empty() {
                            s.current_page = 1;
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Failed to initialize app: {e}");
                }
            }
        });
    });

    // Try auto-login with environment variables or config
    use_effect(move || {
        spawn(async move {
            if !state.read().logged_in {
                let config = state.read().config.as_ref().cloned();

                if let Some(session_str) = attempt_auto_login(config.as_ref()).await {
                    state.with_mut(|s| {
                        s.logged_in = true;
                        s.session = Some(session_str.clone());
                    });

                    // Load recent tracks using the session
                    if load_recent_tracks_from_page(session_str, 1).await.is_ok() {
                        state.with_mut(|s| {
                            s.current_page = 1;
                            // Reload cache to get the newly cached tracks
                            s.track_cache = TrackCache::load();
                        });
                    }
                }
            }
        });
    });

    rsx! {
        div {
            style: "min-height: 100vh; background: #f5f5f5; padding: 20px; font-family: Arial, sans-serif;",
            div {
                style: "max-width: 1200px; margin: 0 auto;",
                h1 {
                    style: "font-size: 2.5rem; font-weight: bold; text-align: center; margin-bottom: 2rem; color: #333;",
                    "Scrobble Scrubber"
                }

                if !state.read().logged_in {
                    LoginPage { state }
                } else {
                    div {
                        Navigation { state }
                        MainContent { state }
                    }
                }
            }
        }
    }
}

#[component]
fn MainContent(state: Signal<AppState>) -> Element {
    let active_page = state.read().active_page.clone();

    rsx! {
        match active_page {
            Page::RuleWorkshop => rsx! { RuleWorkshop { state } },
            Page::RewriteRules => rsx! { RewriteRulesPage { state } },
            Page::ScrobbleScrubber => rsx! { ScrobbleScrubberPage { state } },
            Page::PendingItems => rsx! { PendingItemsPage { state } },
            Page::CacheManagement => rsx! { CacheManagementPage { state } },
        }
    }
}
