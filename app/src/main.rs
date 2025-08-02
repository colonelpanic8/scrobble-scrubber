use ::scrobble_scrubber::config::{ScrobbleScrubberConfig, StorageConfig};
use ::scrobble_scrubber::persistence::{FileStorage, StateStorage};
use ::scrobble_scrubber::rewrite::RewriteRule;
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;
use dioxus_router::*;
use std::sync::Arc;
use tokio::sync::Mutex;

mod api;
mod components;
mod error_utils;
mod icons;
mod scrubber_manager;
mod tray;
mod types;
mod utils;

use api::*;
use components::*;
use types::*;

#[derive(Routable, Clone, Debug, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    ScrobbleScrubber {},
    #[route("/rule-workshop")]
    RuleWorkshop {},
    #[route("/rewrite-rules")]
    RewriteRules {},
    #[route("/pending-edits")]
    PendingEdits {},
    #[route("/pending-rules")]
    PendingRules {},
    #[route("/cache-management")]
    CacheManagement {},
    #[route("/musicbrainz")]
    MusicBrainz {},
    #[route("/config")]
    Config {},
}

// Helper function to initialize config and storage
async fn initialize_app_state() -> Result<
    (
        ScrobbleScrubberConfig,
        Option<Arc<Mutex<FileStorage>>>,
        Vec<RewriteRule>,
    ),
    String,
> {
    let mut config =
        ScrobbleScrubberConfig::load().map_err(|e| format!("Failed to load config: {e}"))?;

    // Update state file path to use per-user directory if we have a username
    if !config.lastfm.username.is_empty() {
        config.storage.state_file =
            StorageConfig::get_default_state_file_path_for_user(Some(&config.lastfm.username));
    }

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

// Helper function to initialize app state from config and storage
async fn initialize_app_state_signal(mut state: Signal<AppState>) {
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
}

// Helper function to load tracks after successful login/session restore
async fn load_tracks_after_login(
    mut state: Signal<AppState>,
    session_str: String,
) -> Result<(), String> {
    load_recent_tracks_from_page(session_str, 1)
        .await
        .map_err(|e| format!("Failed to load recent tracks: {e}"))?;

    state.with_mut(|s| {
        s.current_page = 1;
        s.track_cache = TrackCache::load();
    });

    Ok(())
}

// Helper function to handle successful login
async fn handle_successful_login(mut state: Signal<AppState>, session_str: String) {
    state.with_mut(|s| {
        s.logged_in = true;
        s.session = Some(session_str.clone());
        // Clear scrubber instance so it gets recreated with new session
        s.scrubber_instance = None;
    });
    let _ = load_tracks_after_login(state, session_str).await;
}

// Helper function to handle session restoration and auto-login
async fn handle_session_restore_and_login(state: Signal<AppState>) {
    if state.read().logged_in {
        return;
    }

    // First try to restore a saved session
    match try_restore_session().await {
        Ok(Some(session_str)) => {
            handle_successful_login(state, session_str).await;
            // Check for auto-start after successful login
            check_auto_start_scrubber(state).await;
        }
        Ok(None) => {
            // No saved session, try auto-login with config/env vars
            let config = state.read().config.as_ref().cloned();
            if let Some(session_str) = attempt_auto_login(config.as_ref()).await {
                handle_successful_login(state, session_str).await;
                // Check for auto-start after successful login
                check_auto_start_scrubber(state).await;
            }
        }
        Err(e) => {
            eprintln!("Failed to restore session: {e}");
            // Fallback to auto-login
            let config = state.read().config.as_ref().cloned();
            if let Some(session_str) = attempt_auto_login(config.as_ref()).await {
                handle_successful_login(state, session_str).await;
                // Check for auto-start after successful login
                check_auto_start_scrubber(state).await;
            }
        }
    }
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

// Helper function to check if scrubber should auto-start
async fn check_auto_start_scrubber(state: Signal<AppState>) {
    let should_auto_start = state
        .read()
        .config
        .as_ref()
        .map(|config| config.scrubber.auto_start)
        .unwrap_or(false);

    if should_auto_start {
        start_scrubber(state).await;
    }
}

fn main() {
    let config = icons::create_desktop_config_with_icon();

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(app);
}

fn app() -> Element {
    use dioxus::desktop::{window, WindowCloseBehaviour};

    // Initialize tray icon at app level
    use_hook(|| {
        tray::initialize_tray();
    });

    // Set window close behavior to hide instead of exit
    use_effect(move || {
        spawn(async move {
            // Set window close behavior to hide instead of exit - using new API
            window().set_close_behavior(WindowCloseBehaviour::WindowHides);
            log::info!("Window close behavior set to hide instead of exit");
        });
    });

    rsx! {
        App {}
    }
}

#[component]
fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

#[component]
fn AppLayout() -> Element {
    let state = use_signal(AppState::default);
    use_context_provider(|| state);

    // Initialize config and storage
    use_effect(move || {
        spawn(initialize_app_state_signal(state));
    });

    // Try auto-restore session or fallback to auto-login
    use_effect(move || {
        spawn(handle_session_restore_and_login(state));
    });

    rsx! {
        div {
            style: "min-height: 100vh; background: #f5f5f5; padding: 20px; font-family: Arial, sans-serif;",
            div {
                style: "max-width: 1200px; margin: 0 auto;",
                if !state.read().logged_in {
                    LoginPage { state }
                } else {
                    div {
                        Navigation { state }
                        Outlet::<Route> {}
                    }
                }
            }
        }
    }
}

#[component]
fn RuleWorkshop() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { components::RuleWorkshop { state } }
}

#[component]
fn RewriteRules() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { RewriteRulesPage { state } }
}

#[component]
fn ScrobbleScrubber() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { ScrobbleScrubberPage { state } }
}

#[component]
fn PendingEdits() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { PendingEditsPage { state } }
}

#[component]
fn PendingRules() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { PendingRulesPage { _state: state } }
}

#[component]
fn CacheManagement() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { CacheManagementPage { state } }
}

#[component]
fn MusicBrainz() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { MusicBrainzPage { state } }
}

#[component]
fn Config() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! { ConfigPage { state } }
}
