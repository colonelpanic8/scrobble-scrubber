use crate::api::load_recent_tracks_from_page;
use crate::components::scrobble_scrubber::set_timestamp_anchor;
use crate::types::AppState;
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;
use dioxus_router::prelude::*;

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
                let all_cached_tracks = state_read.track_cache.recent_tracks.clone();
                rsx! {
                    TracksList {
                        tracks: all_cached_tracks,
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
    let total_tracks: usize = state_read.track_cache.recent_tracks.len();
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
            "📂 Using cached recent tracks ({props.total_tracks} tracks)"
        }
    }
}

#[component]
fn LoadMoreTracksButton(mut state: Signal<AppState>) -> Element {
    rsx! {
        button {
            style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
            onclick: move |_| async move {
                let (session_str, current_page) = {
                    let s = state.read();
                    (s.session.clone(), s.current_page)
                };
                if let Some(session_str) = session_str {
                    let next_page = current_page + 1;
                    if load_recent_tracks_from_page(session_str, next_page).await.is_ok() {
                        state.with_mut(|s| {
                            s.current_page = next_page;
                            s.track_cache = TrackCache::load();
                        });
                    }
                }
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
                let session_str = state.read().session.clone();
                if let Some(session_str) = session_str {
                    if load_recent_tracks_from_page(session_str, 1).await.is_ok() {
                        state.with_mut(|s| {
                            s.current_page = 1;
                            s.track_cache = TrackCache::load();
                        });
                    }
                }
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
