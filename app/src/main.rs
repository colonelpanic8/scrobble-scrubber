use ::scrobble_scrubber::config::{ScrobbleScrubberConfig, StorageConfig};
use ::scrobble_scrubber::persistence::{FileStorage, StateStorage};
use ::scrobble_scrubber::rewrite::RewriteRule;
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;
use dioxus_router::*;
use image::ImageReader;
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::Mutex;

mod api;
mod components;
mod error_utils;
mod scrubber_manager;
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

fn create_tray_icon(
    tray_icon_image: tray_icon::Icon,
    _window: Option<dioxus::desktop::DesktopContext>,
) -> Result<
    (
        tray_icon::TrayIcon,
        crossbeam_channel::Receiver<tray_icon::menu::MenuEvent>,
    ),
    Box<dyn std::error::Error>,
> {
    use tray_icon::{
        menu::{Menu, MenuId, MenuItem, Submenu},
        TrayIconBuilder,
    };

    log::debug!("Creating tray menu items");

    // Create menu items with IDs for handling clicks
    let show_hide_item = MenuItem::with_id(MenuId::new("show_hide"), "Show Window", true, None);
    let separator1 = MenuItem::new("", false, None); // Separator

    // Status item (disabled, shows current state)
    let status_item = MenuItem::new("Status: Loading...", false, None);
    let separator_status = MenuItem::new("", false, None); // Separator

    // Scrubber submenu
    let start_scrubber_item =
        MenuItem::with_id(MenuId::new("start_scrubber"), "Start Scrubber", true, None);
    let stop_scrubber_item =
        MenuItem::with_id(MenuId::new("stop_scrubber"), "Stop Scrubber", true, None);
    let process_now_item = MenuItem::with_id(MenuId::new("process_now"), "Process Now", true, None);
    let scrubber_submenu = Submenu::new("Scrubber", true);
    scrubber_submenu.append_items(&[
        &start_scrubber_item,
        &stop_scrubber_item,
        &process_now_item,
    ])?;

    let separator2 = MenuItem::new("", false, None); // Separator
    let config_item = MenuItem::with_id(MenuId::new("config"), "Settings", true, None);
    let about_item = MenuItem::with_id(MenuId::new("about"), "About", true, None);
    let separator3 = MenuItem::new("", false, None); // Separator
    let quit_item = MenuItem::with_id(MenuId::new("quit"), "Exit", true, None);

    let menu = Menu::new();

    menu.append_items(&[
        &show_hide_item,
        &separator1,
        &status_item,
        &separator_status,
        &scrubber_submenu,
        &separator2,
        &config_item,
        &about_item,
        &separator3,
        &quit_item,
    ])?;
    log::debug!("Menu items appended successfully");

    log::debug!("Building tray icon with TrayIconBuilder");

    // Build tray icon with menu
    let tray_icon = TrayIconBuilder::new()
        .with_tooltip("Scrobble Scrubber - Right-click for options")
        .with_icon(tray_icon_image)
        .with_menu(Box::new(menu))
        .build()?;

    println!("🔧 TrayIconBuilder.build() completed successfully");
    log::info!("TrayIconBuilder.build() completed successfully");

    // Get menu event receiver for handling clicks
    let menu_channel = tray_icon::menu::MenuEvent::receiver().clone();

    Ok((tray_icon, menu_channel))
}

fn main() {
    use dioxus::desktop::tao::window::Icon;
    use dioxus::desktop::{Config, WindowBuilder};

    let icon_data = include_bytes!("../assets/icons/256x256.png");

    // Load and decode PNG to RGBA
    let img = ImageReader::new(Cursor::new(icon_data))
        .with_guessed_format()
        .expect("Failed to guess format")
        .decode()
        .expect("Failed to decode image")
        .to_rgba8();

    let (width, height) = img.dimensions();
    let rgba = img.into_raw();

    let icon = Icon::from_rgba(rgba, width, height).expect("Failed to create icon");

    let config = Config::new().with_window(WindowBuilder::new().with_window_icon(Some(icon)));

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(app);
}

fn app() -> Element {
    use dioxus::desktop::{window, WindowCloseBehaviour};

    // Initialize tray icon at app level
    use_hook(|| {
        let icon_data = include_bytes!("../assets/icons/256x256.png");

        // Load and decode PNG to RGBA for tray icon
        let img = match ImageReader::new(Cursor::new(icon_data)).with_guessed_format() {
            Ok(reader) => match reader.decode() {
                Ok(img) => img.to_rgba8(),
                Err(e) => {
                    log::warn!("Failed to decode tray icon image: {e}");
                    return;
                }
            },
            Err(e) => {
                log::warn!("Failed to guess tray icon image format: {e}");
                return;
            }
        };

        let (width, height) = img.dimensions();
        let rgba = img.into_raw();

        // Create tray icon
        let tray_icon_image = match tray_icon::Icon::from_rgba(rgba, width, height) {
            Ok(icon) => icon,
            Err(e) => {
                log::warn!("Failed to create tray icon image: {e}");
                return;
            }
        };

        match create_tray_icon(tray_icon_image, None) {
            Ok((tray_icon, menu_channel)) => {
                log::info!("System tray icon initialized successfully");

                // Handle menu events
                spawn(async move {
                    loop {
                        if let Ok(event) = menu_channel.try_recv() {
                            match event.id.0.as_str() {
                                "show_hide" => {
                                    log::info!("Show/Hide window clicked from tray menu");

                                    // Toggle window visibility using window() function
                                    let win = window();
                                    let is_visible = win.is_visible();
                                    if is_visible {
                                        win.set_visible(false);
                                        log::info!("Window hidden via tray menu");
                                    } else {
                                        win.set_visible(true);
                                        win.set_focus();
                                        log::info!("Window shown and focused via tray menu");
                                    }
                                }
                                "start_scrubber" => {
                                    log::info!("Start scrubber clicked from tray menu");
                                }
                                "stop_scrubber" => {
                                    log::info!("Stop scrubber clicked from tray menu");
                                }
                                "process_now" => {
                                    log::info!("Process now clicked from tray menu");
                                }
                                "config" => {
                                    log::info!("Settings clicked from tray menu");
                                }
                                "about" => {
                                    log::info!("About clicked from tray menu");
                                }
                                "quit" => {
                                    log::info!(
                                        "Exit clicked from tray menu - shutting down application"
                                    );
                                    std::process::exit(0);
                                }
                                _ => {
                                    log::warn!("Unknown tray menu item clicked: {}", event.id.0);
                                }
                            }
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                });

                // Keep tray icon alive
                std::mem::forget(tray_icon);
            }
            Err(e) => {
                log::warn!("Failed to create system tray icon: {e}");
            }
        }
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
