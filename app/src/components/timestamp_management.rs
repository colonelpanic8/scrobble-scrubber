use crate::components::scrobble_scrubber::set_timestamp_anchor;
use crate::types::AppState;
use crate::{error_utils, types::ScrubberStatus};
use ::scrobble_scrubber::mirror::LibraryMirror;
use dioxus::prelude::*;
use dioxus_router::*;

#[component]
pub fn TimestampManagementSection(mut state: Signal<AppState>) -> Element {
    rsx! {
        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
            TimestampManagementHeader { state }

            p { style: "color: #6b7280; margin-bottom: 1rem; font-size: 0.875rem;",
                "Set the processing anchor to control where the scrubber starts processing. Moving the anchor backwards will cause the scrubber to reprocess older tracks."
            }

            {
                let state_read = state.read();
                let tracks = state_read.timestamp_tracks.clone();
                rsx! {
                    TracksList {
                        tracks,
                        state
                    }
                }
            }
        }
    }
}

#[component]
fn TimestampManagementHeader(mut state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let total_tracks: usize = state_read.timestamp_tracks.len();
    let recent_tracks_loaded = total_tracks > 0;

    rsx! {
        div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
            h3 { style: "font-size: 1.25rem; font-weight: bold; margin: 0;", "Processing Anchor" }

            if recent_tracks_loaded {
                div { style: "display: flex; align-items: center; gap: 0.5rem;",
                    TracksLoadedIndicator { total_tracks }
                    LoadMoreTracksButton { state }
                }
            } else {
                LoadRecentTracksButton { state }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TracksLoadedIndicatorProps {
    total_tracks: usize,
}

#[component]
fn TracksLoadedIndicator(props: TracksLoadedIndicatorProps) -> Element {
    rsx! {
        span { style: "font-size: 0.875rem; color: #059669; background: #d1fae5; padding: 0.25rem 0.5rem; border-radius: 0.25rem;",
            "📂 Using mirror recent scrobbles ({props.total_tracks} tracks)"
        }
    }
}

#[component]
fn LoadMoreTracksButton(mut state: Signal<AppState>) -> Element {
    rsx! {
        button {
            style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
            onclick: move |_| async move {
                let (session_str, current_page, config, status) = {
                    let s = state.read();
                    (
                        s.session.clone(),
                        s.current_page,
                        s.config.clone(),
                        s.scrubber_state.status.clone(),
                    )
                };

                // Avoid surprising writes while the scrubber is actively running.
                if matches!(status, ScrubberStatus::Running | ScrubberStatus::Starting) {
                    return;
                }

                let Some(config) = config else { return; };
                let username = config.lastfm.username.trim().to_string();
                if username.is_empty() { return; }
                let state_file = config.storage.state_file.clone();

                let next_page = current_page + 1;
                let want = (next_page as usize) * 50;

                let mirror = match LibraryMirror::open(&username, &state_file) {
                    Ok(m) => m,
                    Err(_) => return,
                };
                let _ = mirror.ingest_repo();

                // Try local mirror first.
                let mut rows = mirror.recent_scrobbles(want, None).unwrap_or_default();

                // If we don't have enough rows, opportunistically sync from Last.fm using the
                // existing session.
                if rows.len() < want {
                    if let Some(session_str) = session_str {
                        if let Ok(session) = error_utils::deserialize_session(&session_str) {
                            let client = error_utils::create_client_from_session(session);
                            let _ = mirror.sync_tip_pages(&client, next_page).await;
                            rows = mirror.recent_scrobbles(want, None).unwrap_or_default();
                        }
                    }
                }

                let tracks: Vec<lastfm_edit::Track> = rows
                    .into_iter()
                    .map(|r| lastfm_edit::Track {
                        name: r.track,
                        artist: r.artist,
                        playcount: 0,
                        timestamp: Some(r.played_at),
                        album: r.album,
                        album_artist: r.album_artist,
                    })
                    .collect();

                state.with_mut(|s| {
                    s.current_page = next_page;
                    s.timestamp_tracks = tracks;
                });
            },
            "Load More Tracks"
        }
    }
}

#[component]
fn LoadRecentTracksButton(mut state: Signal<AppState>) -> Element {
    rsx! {
        button {
            style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
            onclick: move |_| async move {
                let (session_str, config, status) = {
                    let s = state.read();
                    (s.session.clone(), s.config.clone(), s.scrubber_state.status.clone())
                };

                if matches!(status, ScrubberStatus::Running | ScrubberStatus::Starting) {
                    return;
                }

                let Some(config) = config else { return; };
                let username = config.lastfm.username.trim().to_string();
                if username.is_empty() { return; }
                let state_file = config.storage.state_file.clone();

                let page = 1u32;
                let want = 50usize;

                let mirror = match LibraryMirror::open(&username, &state_file) {
                    Ok(m) => m,
                    Err(_) => return,
                };
                let _ = mirror.ingest_repo();

                let mut rows = mirror.recent_scrobbles(want, None).unwrap_or_default();
                if rows.len() < want {
                    if let Some(session_str) = session_str {
                        if let Ok(session) = error_utils::deserialize_session(&session_str) {
                            let client = error_utils::create_client_from_session(session);
                            let _ = mirror.sync_tip_pages(&client, page).await;
                            rows = mirror.recent_scrobbles(want, None).unwrap_or_default();
                        }
                    }
                }

                let tracks: Vec<lastfm_edit::Track> = rows
                    .into_iter()
                    .map(|r| lastfm_edit::Track {
                        name: r.track,
                        artist: r.artist,
                        playcount: 0,
                        timestamp: Some(r.played_at),
                        album: r.album,
                        album_artist: r.album_artist,
                    })
                    .collect();

                state.with_mut(|s| {
                    s.current_page = 1;
                    s.timestamp_tracks = tracks;
                });
            },
            "Load Recent Tracks"
        }
    }
}

#[derive(Props, Clone)]
struct TracksListProps {
    tracks: Vec<lastfm_edit::Track>,
    state: Signal<AppState>,
}

impl PartialEq for TracksListProps {
    fn eq(&self, other: &Self) -> bool {
        self.tracks.len() == other.tracks.len()
    }
}

#[component]
fn TracksList(props: TracksListProps) -> Element {
    if props.tracks.is_empty() {
        rsx! {
            div { style: "text-align: center; color: #6b7280; padding: 2rem;",
                p { "No recent tracks loaded yet." }
                p { style: "font-size: 0.875rem; margin-top: 0.5rem;",
                    "Load recent tracks in the Rule Workshop or click 'Load Recent Tracks' above to see your scrobbles and set the processing anchor."
                }
            }
        }
    } else {
        rsx! {
            div { style: "max-height: 400px; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                for (index, track) in props.tracks.iter().enumerate() {
                    {
                        let track_clone = track.clone();
                        let state = props.state;
                        rsx! {
                            div {
                                key: "{index}",
                                style: "display: flex; justify-content: space-between; align-items: center; padding: 0.75rem; border-bottom: 1px solid #f3f4f6; hover:background: #f9fafb;",

                                div { style: "flex-grow: 1;",
                                    div { style: "font-weight: 500; color: #1f2937;", "{track_clone.name}" }
                                    div { style: "font-size: 0.875rem; color: #6b7280;", "{track_clone.artist}" }
                                    if let Some(album) = &track_clone.album {
                                        div { style: "font-size: 0.75rem; color: #9ca3af;", "{album}" }
                                    }
                                }

                                div { style: "text-align: right; margin-right: 1rem;",
                                    div { style: "font-size: 0.75rem; color: #6b7280;",
                                        {
                                            if let Some(ts) = track_clone.timestamp {
                                                let dt = chrono::DateTime::from_timestamp(ts as i64, 0).unwrap_or_else(chrono::Utc::now);
                                                dt.format("%Y-%m-%d %H:%M:%S").to_string()
                                            } else {
                                                "No timestamp".to_string()
                                            }
                                        }
                                    }
                                }

                                div { style: "display: flex; gap: 0.5rem;",
                                    button {
                                        style: "background: #f59e0b; color: white; padding: 0.25rem 0.75rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem;",
                                        onclick: move |_| {
                                            let track_to_process = track_clone.clone();
                                            spawn(async move {
                                                set_timestamp_anchor(state, track_to_process).await;
                                            });
                                        },
                                        "Set Anchor"
                                    }
                                    Link {
                                        to: format!(
                                            "/musicbrainz-lookup?artist={}&title={}&album={}",
                                            urlencoding::encode(&track_clone.artist),
                                            urlencoding::encode(&track_clone.name),
                                            urlencoding::encode(track_clone.album.as_deref().unwrap_or(""))
                                        ),
                                        style: "background: #7c3aed; color: white; padding: 0.25rem 0.75rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem; text-decoration: none; display: inline-block;",
                                        "MusicBrainz →"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
