use crate::api::{
    clear_cache, load_newer_tracks_fresh, load_recent_tracks_from_page, load_tracks_at_cache_end,
};
use crate::types::AppState;
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;

#[component]
pub fn CacheManagementPage(mut state: Signal<AppState>) -> Element {
    let mut loading_tracks_for_stats = use_signal(|| false);
    let mut loading_next_page = use_signal(|| false);
    let mut loading_newer_tracks = use_signal(|| false);
    let mut tracks_display_count = use_signal(|| 50usize);
    let mut error_message = use_signal(String::new);
    let mut success_message = use_signal(String::new);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            // Header with Actions
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h2 { style: "font-size: 1.5rem; font-weight: bold; margin: 0;", "Cache Management" }
                }

                p { style: "color: #6b7280; margin: 0 0 1.5rem 0;",
                    "Manage cached track data with infinite scrolling. Load more tracks from both ends of the cache."
                }

                // Success/Error Messages
                if !error_message.read().is_empty() {
                    div { style: "background: #fee2e2; border: 1px solid #fca5a5; color: #dc2626; padding: 1rem; border-radius: 0.5rem; margin-bottom: 1rem;",
                        p { "Error: {error_message}" }
                    }
                }

                if !success_message.read().is_empty() {
                    div { style: "background: #d1fae5; border: 1px solid #6ee7b7; color: #059669; padding: 1rem; border-radius: 0.5rem; margin-bottom: 1rem;",
                        p { "{success_message}" }
                    }
                }

                // Cache Actions
                div { style: "display: flex; gap: 0.5rem; flex-wrap: wrap;",
                    button {
                        style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| {
                            spawn(async move {
                                match clear_cache().await {
                                    Ok(msg) => {
                                        println!("✅ {msg}");
                                        state.with_mut(|s| s.track_cache = TrackCache::load());
                                    }
                                    Err(e) => {
                                        eprintln!("❌ Failed to clear cache: {e}");
                                    }
                                }
                            });
                        },
                        "Clear All Cache"
                    }

                    button {
                        style: "background: #8b5cf6; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| {
                            state.with_mut(|s| s.track_cache = TrackCache::load());
                        },
                        "Reload Cache from Disk"
                    }

                    button {
                        style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; opacity: {};",
                            "#059669",
                            if *loading_tracks_for_stats.read() { "0.5" } else { "1" }
                        ),
                        disabled: *loading_tracks_for_stats.read(),
                        onclick: move |_| {
                            spawn(async move {
                                loading_tracks_for_stats.set(true);
                                let session_json = state.read().session.clone();
                                if let Some(session_json) = session_json {
                                    if let Ok(_tracks) = load_recent_tracks_from_page(session_json, 1).await {
                                        state.with_mut(|s| {
                                            s.current_page = 1;
                                            s.track_cache = TrackCache::load();
                                        });
                                    }
                                }
                                loading_tracks_for_stats.set(false);
                            });
                        },
                        if *loading_tracks_for_stats.read() {
                            "Loading..."
                        } else {
                            "Refresh Cache Data"
                        }
                    }

                    button {
                        style: format!("background: #3b82f6; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; opacity: {};",
                            if *loading_next_page.read() || *loading_tracks_for_stats.read() { "0.5" } else { "1" }
                        ),
                        disabled: *loading_next_page.read() || *loading_tracks_for_stats.read(),
                        onclick: move |_| {
                            spawn(async move {
                                loading_next_page.set(true);
                                error_message.set(String::new());
                                success_message.set(String::new());

                                let session_json = state.read().session.clone();
                                if let Some(session_json) = session_json {
                                    // Load tracks at the end of the cache
                                    match load_tracks_at_cache_end(session_json).await {
                                        Ok(tracks) => {
                                            state.with_mut(|s| {
                                                s.track_cache = TrackCache::load();
                                            });
                                            success_message.set(format!("Loaded {} older tracks at cache end", tracks.len()));
                                        }
                                        Err(e) => {
                                            error_message.set(format!("Failed to load tracks at cache end: {e}"));
                                        }
                                    }
                                } else {
                                    error_message.set("No session available - please log in first".to_string());
                                }
                                loading_next_page.set(false);
                            });
                        },
                        if *loading_next_page.read() {
                            "⏳ Loading Next Page..."
                        } else {
                            "📄 Load Next Page"
                        }
                    }

                    button {
                        style: format!("background: #10b981; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; opacity: {};",
                            if *loading_newer_tracks.read() || *loading_tracks_for_stats.read() { "0.5" } else { "1" }
                        ),
                        disabled: *loading_newer_tracks.read() || *loading_tracks_for_stats.read(),
                        onclick: move |_| {
                            spawn(async move {
                                loading_newer_tracks.set(true);
                                error_message.set(String::new());
                                success_message.set(String::new());

                                let session_json = state.read().session.clone();
                                if let Some(session_json) = session_json {
                                    // Load fresh newer tracks (always bypass cache)
                                    match load_newer_tracks_fresh(session_json).await {
                                        Ok(tracks) => {
                                            state.with_mut(|s| {
                                                s.track_cache = TrackCache::load();
                                            });
                                            success_message.set(format!("Loaded {} fresh newer tracks", tracks.len()));
                                        }
                                        Err(e) => {
                                            error_message.set(format!("Failed to load fresh newer tracks: {e}"));
                                        }
                                    }
                                } else {
                                    error_message.set("No session available - please log in first".to_string());
                                }
                                loading_newer_tracks.set(false);
                            });
                        },
                        if *loading_newer_tracks.read() {
                            "⏳ Loading Newer Tracks..."
                        } else {
                            "⬆️ Load Newer Tracks"
                        }
                    }

                    {
                        let state_read = state.read();
                        let all_tracks = state_read.track_cache.get_all_recent_tracks();
                        let display_count = *tracks_display_count.read();
                        if display_count < all_tracks.len() {
                            rsx! {
                                button {
                                    style: "background: #6b7280; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        let current_count = *tracks_display_count.read();
                                        let state_read = state.read();
                                        let all_tracks = state_read.track_cache.get_all_recent_tracks();
                                        tracks_display_count.set((current_count + 50).min(all_tracks.len()));
                                    },
                                    "Show More Tracks ({display_count}/{all_tracks.len()})"
                                }
                            }
                        } else {
                            rsx! { span {} }
                        }
                    }
                }
            }


            // Infinite Scroll Tracks View
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Cached Tracks" }

                {
                    let state_read = state.read();
                    let anchor_timestamp = state_read.scrubber_state.current_anchor_timestamp;
                    let all_tracks = state_read.track_cache.get_all_recent_tracks();
                    let display_count = *tracks_display_count.read();

                    if all_tracks.is_empty() {
                        rsx! {
                            div { style: "text-center; color: #6b7280; padding: 2rem;",
                                p { "No recent tracks cached yet." }
                                p { style: "font-size: 0.875rem; margin-top: 0.5rem;",
                                    "Recent tracks will be cached automatically when you load them in the Rule Workshop or Rewrite Rules page."
                                }
                            }
                        }
                    } else {
                        // Find anchor position in the track list
                        let anchor_index = if let Some(anchor_ts) = anchor_timestamp {
                            all_tracks.iter().position(|track| track.timestamp == Some(anchor_ts))
                        } else {
                            None
                        };

                        rsx! {
                            // Anchor info if present
                            if let Some(anchor_idx) = anchor_index {
                                div { style: "padding: 0.75rem; background: #fef3c7; border: 2px solid #f59e0b; border-radius: 0.5rem; margin-bottom: 1rem;",
                                    div { style: "font-weight: bold; color: #92400e; margin-bottom: 0.5rem;",
                                        "📍 Processing Anchor Position"
                                    }
                                    div { style: "color: #92400e;",
                                        "Track #{anchor_idx + 1} of {all_tracks.len()} • "
                                        {
                                            if let Some(anchor_track) = all_tracks.get(anchor_idx) {
                                                format!("\"{}\" by \"{}\"", anchor_track.name, anchor_track.artist)
                                            } else {
                                                "Unknown track".to_string()
                                            }
                                        }
                                    }
                                }
                            }


                            // Tracks list
                            div { style: "border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                for (index, track) in all_tracks.iter().take(display_count).enumerate() {
                                    div {
                                        key: "{track.name}_{track.artist}_{track.timestamp.unwrap_or(0)}",
                                        style: format!(
                                            "padding: 0.75rem; border-bottom: 1px solid #f3f4f6; display: flex; justify-content: space-between; align-items: center; {}",
                                            if Some(index) == anchor_index {
                                                "background: #fef3c7; border-left: 4px solid #f59e0b;"
                                            } else if let Some(anchor_idx) = anchor_index {
                                                if index < anchor_idx {
                                                    "background: #f0f9ff;" // Pending (above anchor)
                                                } else {
                                                    "background: #f0fdf4;" // Processed (below anchor)
                                                }
                                            } else {
                                                ""
                                            }
                                        ),
                                        div { style: "flex: 1;",
                                            div { style: "font-weight: 500; color: #374151;",
                                                "\"{track.name}\" by {track.artist}"
                                            }
                                            if let Some(album) = &track.album {
                                                div { style: "font-size: 0.875rem; color: #6b7280;",
                                                    "Album: {album}"
                                                }
                                            }
                                            if let Some(timestamp) = track.timestamp {
                                                div { style: "font-size: 0.75rem; color: #9ca3af;",
                                                    {
                                                        use chrono::DateTime;
                                                        DateTime::from_timestamp(timestamp as i64, 0)
                                                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                                                            .unwrap_or_else(|| "Invalid timestamp".to_string())
                                                    }
                                                }
                                            }
                                        }
                                        div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                            span { style: "font-size: 0.625rem; color: #6b7280; background: #f3f4f6; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                                                "#{index + 1}"
                                            }
                                            if Some(index) == anchor_index {
                                                span { style: "font-size: 0.625rem; color: #92400e; background: #fef3c7; padding: 0.125rem 0.25rem; border-radius: 0.25rem; font-weight: bold;",
                                                    "📍 ANCHOR"
                                                }
                                            } else if let Some(anchor_idx) = anchor_index {
                                                if index < anchor_idx {
                                                    span { style: "font-size: 0.625rem; color: #1d4ed8; background: #dbeafe; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                                                        "⏳ pending"
                                                    }
                                                } else {
                                                    span { style: "font-size: 0.625rem; color: #059669; background: #d1fae5; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                                                        "✅ processed"
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
            }



        }
    }
}
