use ::scrobble_scrubber::config::ScrobbleScrubberConfig;
use ::scrobble_scrubber::persistence::{FileStorage, StateStorage};
use dioxus::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

mod cache;
mod components;
mod server_functions;
mod types;
mod utils;

use components::*;
use server_functions::*;
use types::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut state = use_signal(AppState::default);

    // Initialize config and storage
    use_effect(move || {
        spawn(async move {
            // Try to load config
            match ScrobbleScrubberConfig::load() {
                Ok(config) => {
                    let state_file = config.storage.state_file.clone();

                    // Try to initialize storage
                    match FileStorage::new(&state_file) {
                        Ok(storage) => {
                            // Try to load existing rewrite rules
                            let saved_rules = match storage.load_rewrite_rules_state().await {
                                Ok(rules_state) => rules_state.rewrite_rules,
                                Err(_) => Vec::new(),
                            };

                            state.with_mut(|s| {
                                s.config = Some(config);
                                s.storage = Some(Arc::new(Mutex::new(storage)));
                                s.saved_rules = saved_rules;
                            });
                        }
                        Err(e) => {
                            eprintln!("Failed to initialize storage: {e}");
                            // Still set config without storage
                            state.with_mut(|s| s.config = Some(config));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load config: {e}");
                }
            }
        });
    });

    // Try auto-login with environment variables or config
    use_effect(move || {
        spawn(async move {
            if !state.read().logged_in {
                let (username, password) = {
                    let s = state.read();
                    if let Some(config) = &s.config {
                        (
                            config.lastfm.username.clone(),
                            config.lastfm.password.clone(),
                        )
                    } else {
                        // Fallback to environment variables
                        (
                            std::env::var("SCROBBLE_SCRUBBER_LASTFM_USERNAME").unwrap_or_default(),
                            std::env::var("SCROBBLE_SCRUBBER_LASTFM_PASSWORD").unwrap_or_default(),
                        )
                    }
                };

                if !username.trim().is_empty() && !password.trim().is_empty() {
                    match login_to_lastfm(username.trim().to_string(), password.trim().to_string())
                        .await
                    {
                        Ok(session_str) => {
                            state.with_mut(|s| {
                                s.logged_in = true;
                                s.session = Some(session_str.clone());
                            });

                            // Load recent tracks using the session
                            if let Ok(tracks) = load_recent_tracks_from_page(session_str, 1).await {
                                state.with_mut(|s| {
                                    s.recent_tracks.tracks = tracks;
                                    s.current_page = 1;
                                });
                            }
                        }
                        Err(e) => {
                            eprintln!("Auto-login failed: {e}");
                        }
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
        }
    }
}
