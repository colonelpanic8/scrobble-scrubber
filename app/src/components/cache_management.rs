use crate::api::{clear_cache, get_cache_stats, load_recent_tracks_from_page};
use crate::types::AppState;
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;

#[component]
pub fn CacheManagementPage(mut state: Signal<AppState>) -> Element {
    let mut cache_stats = use_signal(String::new);
    let mut show_cache_info = use_signal(|| false);
    let mut loading_tracks_for_stats = use_signal(|| false);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            // Header
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h2 { style: "font-size: 1.5rem; font-weight: bold; margin: 0;", "Cache Management" }
                    button {
                        style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; opacity: {};",
                            "#8b5cf6",
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
                }

                p { style: "color: #6b7280; margin: 0;",
                    "Manage cached track data and view cache statistics. The cache stores recent tracks and artist tracks to improve performance and reduce API calls."
                }
            }

            // Cache Overview
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Cache Overview" }

                {
                    let state_read = state.read();
                    let recent_pages_count = 0; // No longer using pages
                    let total_recent_tracks: usize = state_read.track_cache.recent_tracks.len();
                    let artist_count = state_read.track_cache.artist_tracks.len();
                    let total_artist_tracks: usize = state_read.track_cache.artist_tracks.values().map(|v| v.len()).sum();

                    rsx! {
                        div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 1rem;",
                            div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                div { style: "font-size: 1.5rem; font-weight: bold; color: #2563eb;", "{recent_pages_count}" }
                                div { style: "font-size: 0.875rem; color: #6b7280;", "Recent Track Pages" }
                            }
                            div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                div { style: "font-size: 1.5rem; font-weight: bold; color: #059669;", "{total_recent_tracks}" }
                                div { style: "font-size: 0.875rem; color: #6b7280;", "Total Recent Tracks" }
                            }
                            div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                div { style: "font-size: 1.5rem; font-weight: bold; color: #7c3aed;", "{artist_count}" }
                                div { style: "font-size: 0.875rem; color: #6b7280;", "Cached Artists" }
                            }
                            div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                div { style: "font-size: 1.5rem; font-weight: bold; color: #dc2626;", "{total_artist_tracks}" }
                                div { style: "font-size: 0.875rem; color: #6b7280;", "Total Artist Tracks" }
                            }
                        }
                    }
                }
            }

            // Processing Anchor Position
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Processing Anchor & Recent Tracks" }

                {
                    let state_read = state.read();
                    let anchor_timestamp = state_read.scrubber_state.current_anchor_timestamp;
                    let all_tracks = state_read.track_cache.get_all_recent_tracks();

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
                            div { style: "margin-bottom: 1rem;",
                                if let Some(_anchor_ts) = anchor_timestamp {
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
                                            if anchor_idx > 0 {
                                                div { style: "font-size: 0.875rem; color: #92400e; margin-top: 0.25rem;",
                                                    "⏳ {anchor_idx} tracks above anchor (pending processing)"
                                                }
                                            }
                                            if anchor_idx < all_tracks.len() - 1 {
                                                div { style: "font-size: 0.875rem; color: #92400e; margin-top: 0.25rem;",
                                                    "✅ {all_tracks.len() - anchor_idx - 1} tracks below anchor (processed)"
                                                }
                                            }
                                        }
                                    } else {
                                        div { style: "padding: 0.75rem; background: #fee2e2; border: 2px solid #f87171; border-radius: 0.5rem; margin-bottom: 1rem;",
                                            div { style: "font-weight: bold; color: #dc2626; margin-bottom: 0.5rem;",
                                                "⚠️ Anchor Not Found in Cache"
                                            }
                                            div { style: "color: #dc2626; font-size: 0.875rem;",
                                                "Anchor timestamp set but corresponding track not found in current cache."
                                            }
                                        }
                                    }
                                } else {
                                    div { style: "padding: 0.75rem; background: #e5e7eb; border: 2px solid #9ca3af; border-radius: 0.5rem; margin-bottom: 1rem;",
                                        div { style: "font-weight: bold; color: #4b5563; margin-bottom: 0.5rem;",
                                            "📍 No Processing Anchor Set"
                                        }
                                        div { style: "color: #4b5563; font-size: 0.875rem;",
                                            "The scrubber will process all tracks on first run."
                                        }
                                    }
                                }
                            }

                            // Show tracks with anchor highlighted
                            div { style: "max-height: 400px; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                for (index, track) in all_tracks.iter().take(50).enumerate() {
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
                                if all_tracks.len() > 50 {
                                    div { style: "padding: 1rem; text-align: center; color: #6b7280; font-size: 0.875rem;",
                                        "... and {all_tracks.len() - 50} more tracks (showing first 50)"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Recent Tracks Cache Pages Overview
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Cache Pages Overview" }

                {
                    let state_read = state.read();
                    if state_read.track_cache.recent_tracks.is_empty() {
                        rsx! {
                            div { style: "text-center; color: #6b7280; padding: 2rem;",
                                p { "No recent tracks cached yet." }
                                p { style: "font-size: 0.875rem; margin-top: 0.5rem;",
                                    "Recent tracks will be cached automatically when you load them in the Rule Workshop or Rewrite Rules page."
                                }
                            }
                        }
                    } else {
                        rsx! {
                            div { style: "text-center; color: #6b7280; padding: 2rem;",
                                p { "Page-based cache structure is no longer used." }
                                p { style: "font-size: 0.875rem; margin-top: 0.5rem;",
                                    "Tracks are now stored as a single chronologically ordered list."
                                }
                            }
                        }
                    }
                }
            }

            // Artist Tracks Cache Detail
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Artist Tracks Cache" }

                {
                    let state_read = state.read();
                    if state_read.track_cache.artist_tracks.is_empty() {
                        rsx! {
                            div { style: "text-center; color: #6b7280; padding: 2rem;",
                                p { "No artist tracks cached yet." }
                                p { style: "font-size: 0.875rem; margin-top: 0.5rem;",
                                    "Artist tracks will be cached automatically when you load them using the 'Load All Tracks for Artist' feature."
                                }
                            }
                        }
                    } else {
                        rsx! {
                            div { style: "display: flex; flex-direction: column; gap: 0.5rem;",
                                for (artist_name, tracks) in &state_read.track_cache.artist_tracks {
                                    div {
                                        key: "{artist_name}",
                                        style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                        div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                            span { style: "font-weight: 500; color: #374151;", "{artist_name}" }
                                            span { style: "font-size: 0.75rem; color: #6b7280;", "({tracks.len()} tracks)" }
                                        }
                                        div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                            span { style: "font-size: 0.625rem; color: #059669; background: #d1fae5; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                                                "📂 cached"
                                            }
                                            if state_read.artist_tracks.contains_key(artist_name) {
                                                span { style: "font-size: 0.625rem; color: #7c3aed; background: #ede9fe; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                                                    "enabled"
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

            // Cache Management Actions
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h3 { style: "font-weight: 600; color: #374151;", "Cache Actions" }
                    button {
                        style: "background: #6b7280; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| {
                            let current_state = *show_cache_info.read();
                            show_cache_info.set(!current_state);
                            if !current_state {
                                spawn(async move {
                                    match get_cache_stats().await {
                                        Ok(stats) => cache_stats.set(stats),
                                        Err(e) => cache_stats.set(format!("Error: {e}")),
                                    }
                                });
                            }
                        },
                        if *show_cache_info.read() { "Hide Detailed Stats" } else { "Show Detailed Stats" }
                    }
                }

                if *show_cache_info.read() {
                    div { style: "margin-bottom: 1rem;",
                        if !cache_stats.read().is_empty() {
                            div { style: "font-size: 0.875rem; color: #4b5563; margin-bottom: 1rem; padding: 0.5rem; background: #f9fafb; border-radius: 0.375rem;",
                                "{cache_stats}"
                            }
                        }
                    }
                }

                div { style: "display: flex; gap: 0.5rem; flex-wrap: wrap;",
                    button {
                        style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| {
                            spawn(async move {
                                match clear_cache().await {
                                    Ok(msg) => {
                                        println!("✅ {msg}");
                                        cache_stats.set("Cache cleared".to_string());
                                        // Reload the cache state
                                        state.with_mut(|s| s.track_cache = TrackCache::load());
                                    }
                                    Err(e) => {
                                        eprintln!("❌ Failed to clear cache: {e}");
                                        cache_stats.set(format!("Error: {e}"));
                                    }
                                }
                            });
                        },
                        "Clear All Cache"
                    }

                    button {
                        style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| {
                            spawn(async move {
                                match get_cache_stats().await {
                                    Ok(stats) => cache_stats.set(stats),
                                    Err(e) => cache_stats.set(format!("Error: {e}")),
                                }
                            });
                        },
                        "Refresh Stats"
                    }

                    button {
                        style: "background: #8b5cf6; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| {
                            state.with_mut(|s| s.track_cache = TrackCache::load());
                        },
                        "Reload Cache from Disk"
                    }
                }
            }
        }
    }
}
