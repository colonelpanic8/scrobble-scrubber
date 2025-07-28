use crate::api::{load_artist_tracks, load_recent_tracks_from_page};
use crate::types::{AppState, TrackSourceState};
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;
use futures::FutureExt;

#[component]
pub fn TrackSourcesSection(mut state: Signal<AppState>) -> Element {
    let loading_tracks = use_signal(|| false);
    let loading_artist_tracks = use_signal(|| false);
    let artist_name = use_signal(String::new);
    let artist_error = use_signal(|| Option::<String>::None);

    rsx! {
        div {
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
            h2 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Track Sources" }

            ArtistLoadingSection {
                state,
                loading_artist_tracks,
                artist_name,
                artist_error
            }

            TrackSourceControls { state }

            LoadMoreTracksButton { state, loading_tracks }
        }
    }
}

#[component]
fn ArtistLoadingSection(
    mut state: Signal<AppState>,
    mut loading_artist_tracks: Signal<bool>,
    mut artist_name: Signal<String>,
    mut artist_error: Signal<Option<String>>,
) -> Element {
    rsx! {
        div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; margin-bottom: 1rem;",
            h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Load All Tracks for Artist" }

            CachedArtistsInfo { state }

            div { style: "display: flex; gap: 1rem; align-items: end;",
                div { style: "flex: 1;",
                    label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Artist Name" }
                    input {
                        style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                        placeholder: "Enter artist name",
                        value: "{artist_name}",
                        oninput: move |e| artist_name.set(e.value())
                    }
                }

                LoadArtistButton {
                    state,
                    loading_artist_tracks,
                    artist_name,
                    artist_error
                }
            }

            ArtistErrorDisplay { artist_error }
        }
    }
}

#[component]
fn CachedArtistsInfo(state: Signal<AppState>) -> Element {
    let cached_artists_count = state.read().track_cache.artist_tracks.len();

    if cached_artists_count > 0 {
        rsx! {
            p { style: "font-size: 0.875rem; color: #6b7280; margin-bottom: 1rem;",
                "📂 {cached_artists_count} artists loaded from cache (see Track Source Controls below)"
            }
        }
    } else {
        rsx! { div {} }
    }
}

#[component]
fn LoadArtistButton(
    mut state: Signal<AppState>,
    mut loading_artist_tracks: Signal<bool>,
    artist_name: Signal<String>,
    mut artist_error: Signal<Option<String>>,
) -> Element {
    rsx! {
        button {
            style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {};",
                "#2563eb",
                if *loading_artist_tracks.read() { "0.5" } else { "1" }
            ),
            disabled: *loading_artist_tracks.read() || artist_name.read().trim().is_empty(),
            onclick: move |_| async move {
                let session_str = state.read().session.clone();
                let artist = artist_name.read().trim().to_string();

                if let Some(session_str) = session_str {
                    if !artist.is_empty() {
                        loading_artist_tracks.set(true);

                        let result = std::panic::AssertUnwindSafe(load_artist_tracks(session_str, artist.clone()))
                            .catch_unwind()
                            .await;

                        match result {
                            Ok(Ok(_tracks)) => {
                                state.with_mut(|s| {
                                    s.artist_tracks.insert(artist, TrackSourceState {
                                        enabled: true,
                                    });
                                    s.track_cache = TrackCache::load();
                                });
                                artist_error.set(None);
                            }
                            Ok(Err(e)) => {
                                let error_msg = format!("Failed to load artist tracks: {e}");
                                eprintln!("{error_msg}");
                                artist_error.set(Some(error_msg));
                            }
                            Err(panic_err) => {
                                let error_msg = format!("PANIC in load_artist_tracks: {panic_err:?}");
                                eprintln!("{error_msg}");
                                artist_error.set(Some("Function crashed unexpectedly".to_string()));
                            }
                        }

                        loading_artist_tracks.set(false);
                    }
                }
            },
            if *loading_artist_tracks.read() {
                "Loading..."
            } else {
                "Load Artist Tracks"
            }
        }
    }
}

#[component]
fn ArtistErrorDisplay(artist_error: Signal<Option<String>>) -> Element {
    if let Some(error) = artist_error.read().as_ref() {
        rsx! {
            div { style: "margin-top: 0.5rem; padding: 0.75rem; background: #fef2f2; border: 1px solid #fecaca; border-radius: 0.375rem; color: #991b1b; font-size: 0.875rem;",
                "❌ {error}"
            }
        }
    } else {
        rsx! { div {} }
    }
}

#[component]
fn TrackSourceControls(mut state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let all_tracks = crate::utils::get_current_tracks(&state_read);

    rsx! {
        div { style: "margin-top: 1rem; border-top: 1px solid #e5e7eb; padding-top: 1rem;",
            h4 { style: "font-weight: 600; margin-bottom: 0.75rem; color: #374151;", "Track Source Controls" }
            div { style: "display: flex; flex-direction: column; gap: 0.5rem;",
                RecentTracksControl { state }

                for (artist_name, track_state) in &state_read.artist_tracks {
                    ArtistTrackControl {
                        key: "{artist_name}",
                        state,
                        artist_name: artist_name.clone(),
                        track_state: track_state.clone()
                    }
                }
            }

            div { style: "margin-top: 0.5rem; font-size: 0.875rem; color: #6b7280;",
                "Total enabled tracks: {all_tracks.len()}"
            }
        }
    }
}

#[component]
fn RecentTracksControl(mut state: Signal<AppState>) -> Element {
    let state_read = state.read();

    rsx! {
        div { style: "display: flex; align-items: center; justify-content: space-between; padding: 0.5rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                input {
                    r#type: "checkbox",
                    id: "enable-recent-tracks",
                    checked: "{state_read.recent_tracks.enabled}",
                    onchange: move |e| {
                        state.with_mut(|s| s.recent_tracks.enabled = e.checked());
                    }
                }
                label {
                    r#for: "enable-recent-tracks",
                    style: "font-size: 0.875rem; font-weight: 500; color: #374151; cursor: pointer;",
                    "Recent Tracks"
                }
            }
            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                span { style: "font-size: 0.75rem; color: #6b7280;",
                    {
                        let total_tracks: usize = state_read.track_cache.recent_tracks.len();
                        format!("{total_tracks} tracks")
                    }
                }
                if !state_read.track_cache.recent_tracks.is_empty() {
                    span { style: "font-size: 0.625rem; color: #059669; background: #d1fae5; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                        "📂 cached"
                    }
                }
            }
        }
    }
}

#[component]
fn ArtistTrackControl(
    mut state: Signal<AppState>,
    artist_name: String,
    track_state: TrackSourceState,
) -> Element {
    let state_read = state.read();
    let track_count = state_read
        .track_cache
        .artist_tracks
        .get(&artist_name)
        .map(|v| v.len())
        .unwrap_or(0);
    let is_cached = state_read
        .track_cache
        .artist_tracks
        .contains_key(&artist_name);

    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: space-between; padding: 0.5rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                input {
                    r#type: "checkbox",
                    id: "enable-artist-{artist_name}",
                    checked: "{track_state.enabled}",
                    onchange: {
                        let artist_name = artist_name.clone();
                        move |e: Event<FormData>| {
                            let enabled = e.checked();
                            state.with_mut(|s| {
                                if let Some(track_state) = s.artist_tracks.get_mut(&artist_name) {
                                    track_state.enabled = enabled;
                                }
                            });
                        }
                    }
                }
                label {
                    r#for: "enable-artist-{artist_name}",
                    style: "font-size: 0.875rem; font-weight: 500; color: #374151; cursor: pointer;",
                    "{artist_name}"
                }
            }
            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                span { style: "font-size: 0.75rem; color: #6b7280;",
                    "{track_count} tracks"
                }
                if is_cached {
                    span { style: "font-size: 0.625rem; color: #059669; background: #d1fae5; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                        "📂 cached"
                    }
                }
                button {
                    style: "background: #dc2626; color: white; padding: 0.25rem 0.5rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem;",
                    onclick: {
                        let artist_name = artist_name.clone();
                        move |_| {
                            state.with_mut(|s| {
                                s.artist_tracks.remove(&artist_name);
                            });
                        }
                    },
                    "Remove"
                }
            }
        }
    }
}

#[component]
fn LoadMoreTracksButton(mut state: Signal<AppState>, mut loading_tracks: Signal<bool>) -> Element {
    rsx! {
        div { style: "margin-top: 1rem; text-align: right;",
            button {
                style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {};",
                    "#059669",
                    if *loading_tracks.read() { "0.5" } else { "1" }
                ),
                disabled: *loading_tracks.read(),
                onclick: move |_| async move {
                    let (session_str, current_page) = {
                        let s = state.read();
                        (s.session.clone(), s.current_page)
                    };
                    if let Some(session_str) = session_str {
                        loading_tracks.set(true);
                        let next_page = current_page + 1;
                        if let Ok(_new_tracks) = load_recent_tracks_from_page(session_str, next_page).await {
                            state.with_mut(|s| {
                                s.current_page = next_page;
                                s.track_cache = TrackCache::load();
                            });
                        }
                        loading_tracks.set(false);
                    }
                },
                if *loading_tracks.read() {
                    "Loading..."
                } else {
                    "Load More Recent Tracks"
                }
            }
        }
    }
}
