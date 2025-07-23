use crate::server_functions::{load_recent_tracks_from_page, login_to_lastfm};
use crate::types::AppState;
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;

#[component]
pub fn LoginPage(mut state: Signal<AppState>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);

    rsx! {
        div {
            style: "max-width: 400px; margin: 0 auto; background: white; border-radius: 8px; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 2rem;",
            h2 {
                style: "font-size: 1.5rem; font-weight: bold; margin-bottom: 1.5rem; text-align: center;",
                "Login to Last.fm"
            }

            if !error.read().is_empty() {
                div {
                    style: "background: #fee; border: 1px solid #fcc; color: #c33; padding: 0.75rem 1rem; border-radius: 4px; margin-bottom: 1rem;",
                    "{error}"
                }
            }

            div { style: "display: flex; flex-direction: column; gap: 1rem;",
                div {
                    label {
                        style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.5rem;",
                        "Username"
                    }
                    input {
                        style: "width: 100%; padding: 0.5rem 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; outline: none;",
                        r#type: "text",
                        placeholder: "Your Last.fm username",
                        value: "{username}",
                        oninput: move |e| username.set(e.value())
                    }
                }

                div {
                    label {
                        style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.5rem;",
                        "Password"
                    }
                    input {
                        style: "width: 100%; padding: 0.5rem 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; outline: none;",
                        r#type: "password",
                        placeholder: "Your Last.fm password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value())
                    }
                }

                button {
                    style: format!("width: 100%; background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {};",
                        "#2563eb",
                        if *loading.read() { "0.5" } else { "1" }
                    ),
                    disabled: *loading.read(),
                    onclick: move |_| async move {
                        loading.set(true);
                        error.set(String::new());

                        let username_val = username.read().clone();
                        let password_val = password.read().clone();
                        match login_to_lastfm(username_val, password_val).await {
                            Ok(session_str) => {
                                state.with_mut(|s| {
                                    s.logged_in = true;
                                    s.session = Some(session_str.clone());
                                });

                                // Load recent tracks using the session
                                if let Ok(_tracks) = load_recent_tracks_from_page(session_str, 1).await {
                                    state.with_mut(|s| {
                                        s.current_page = 1;
                                        // Reload cache to get the newly cached tracks
                                        s.track_cache = TrackCache::load();
                                    });
                                }
                            }
                            Err(e) => {
                                error.set(format!("Login failed: {e}"));
                            }
                        }
                        loading.set(false);
                    },
                    if *loading.read() {
                        "Logging in..."
                    } else {
                        "Login"
                    }
                }
            }
        }
    }
}
