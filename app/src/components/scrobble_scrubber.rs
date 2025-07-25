use crate::server_functions::load_recent_tracks_from_page;
use crate::types::{event_formatting, AppState, ScrubberStatus};
use ::scrobble_scrubber::events::ScrubberEvent;
use ::scrobble_scrubber::track_cache::TrackCache;
use chrono::Utc;
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;
use lastfm_edit::Track;
use std::sync::Arc;
use tokio::sync::broadcast;

#[component]
pub fn ScrobbleScrubberPage(mut state: Signal<AppState>) -> Element {
    let scrubber_state = state.read().scrubber_state.clone();

    // Create a timer that ticks every second - following dioxus-timer pattern
    let mut timer_tick = use_signal(chrono::Utc::now);

    use_future(move || async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            // Always update the tick - let the component decide if it needs the current time
            timer_tick.set(chrono::Utc::now());
        }
    });

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            // Header with controls
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: flex-end; align-items: center; margin-bottom: 1rem;",
                    div { style: "display: flex; align-items: center; gap: 1rem;",
                        // Status indicator
                        div {
                            style: format!(
                                "padding: 0.5rem 1rem; border-radius: 0.375rem; font-size: 0.875rem; font-weight: 500; {}",
                                match scrubber_state.status {
                                    ScrubberStatus::Stopped => "background: #fee2e2; color: #991b1b;",
                                    ScrubberStatus::Starting => "background: #fef3c7; color: #92400e;",
                                    ScrubberStatus::Running => "background: #dcfce7; color: #166534;",
                                    ScrubberStatus::Sleeping { .. } => "background: #e0e7ff; color: #3730a3;",
                                    ScrubberStatus::Stopping => "background: #fef3c7; color: #92400e;",
                                    ScrubberStatus::Error(_) => "background: #fecaca; color: #dc2626;",
                                }
                            ),
                            {match &scrubber_state.status {
                                ScrubberStatus::Stopped => "Stopped".to_string(),
                                ScrubberStatus::Starting => "Starting...".to_string(),
                                ScrubberStatus::Running => "Running".to_string(),
                                ScrubberStatus::Sleeping { until_timestamp } => {
                                    // Read timer_tick to ensure re-renders happen during countdown
                                    let _tick = timer_tick.read();
                                    let now = chrono::Utc::now();
                                    let remaining_seconds = (*until_timestamp - now).num_seconds().max(0);

                                    if remaining_seconds > 0 {
                                        format!("💤 Sleeping ({remaining_seconds}s)")
                                    } else {
                                        "💤 Sleeping".to_string()
                                    }
                                },
                                ScrubberStatus::Stopping => "Stopping...".to_string(),
                                ScrubberStatus::Error(err) => format!("Error: {err}"),
                            }}
                        }

                        // Control buttons
                        match scrubber_state.status {
                            ScrubberStatus::Stopped | ScrubberStatus::Error(_) => rsx! {
                                button {
                                    style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; margin-right: 0.5rem;",
                                    onclick: move |_| {
                                        spawn(async move {
                                            start_scrubber(state).await;
                                        });
                                    },
                                    "Start Scrubber"
                                }
                                button {
                                    style: "background: #2563eb; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        spawn(async move {
                                            trigger_manual_processing(state).await;
                                        });
                                    },
                                    "Process Now"
                                }
                            },
                            ScrubberStatus::Running | ScrubberStatus::Sleeping { .. } => rsx! {
                                button {
                                    style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; margin-right: 0.5rem;",
                                    onclick: move |_| {
                                        spawn(async move {
                                            stop_scrubber(state).await;
                                        });
                                    },
                                    "Stop Scrubber"
                                }
                                button {
                                    style: "background: #7c3aed; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        spawn(async move {
                                            trigger_manual_processing(state).await;
                                        });
                                    },
                                    "Process Now"
                                }
                            },
                            ScrubberStatus::Starting | ScrubberStatus::Stopping => rsx! {
                                button {
                                    style: "background: #6b7280; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: not-allowed; font-size: 0.875rem;",
                                    disabled: true,
                                    "Please wait..."
                                }
                            },
                        }
                    }
                }

                p { style: "color: #6b7280; margin: 0;",
                    "Monitor and control the scrobble scrubber. The scrubber processes your scrobbles and applies rewrite rules automatically."
                }
            }

            // Statistics
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Statistics" }
                div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 1rem;",
                    div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                        div { style: "font-size: 1.5rem; font-weight: bold; color: #2563eb;", "{scrubber_state.processed_count}" }
                        div { style: "font-size: 0.875rem; color: #6b7280;", "Tracks Processed" }
                    }
                    div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                        div { style: "font-size: 1.5rem; font-weight: bold; color: #059669;", "{scrubber_state.rules_applied_count}" }
                        div { style: "font-size: 0.875rem; color: #6b7280;", "Rules Applied" }
                    }
                    div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                        div { style: "font-size: 1.5rem; font-weight: bold; color: #dc2626;", "{scrubber_state.events.len()}" }
                        div { style: "font-size: 0.875rem; color: #6b7280;", "Total Events" }
                    }
                }
            }

            // Artist-Specific Processing
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Process Specific Artist" }
                p { style: "color: #6b7280; margin-bottom: 1rem; font-size: 0.875rem;",
                    "Enter an artist name to process only tracks by that artist. This will apply your saved rules to all tracks by the specified artist in your cache."
                }

                {
                    let mut artist_input = use_signal(String::new);

                    rsx! {
                        div { style: "display: flex; gap: 0.75rem; align-items: flex-end;",
                            div { style: "flex: 1;",
                                label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.25rem;",
                                    "Artist Name"
                                }
                                input {
                                    r#type: "text",
                                    placeholder: "Enter artist name...",
                                    value: "{artist_input.read()}",
                                    style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem; focus:outline-none; focus:ring-2; focus:ring-blue-500; focus:border-transparent;",
                                    oninput: move |event| {
                                        artist_input.set(event.value());
                                    },
                                    onkeypress: move |event| {
                                        if event.key() == Key::Enter {
                                            let artist_name = artist_input.read().trim().to_string();
                                            if !artist_name.is_empty() {
                                                spawn(async move {
                                                    trigger_artist_processing(state, artist_name).await;
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            button {
                                style: format!(
                                    "padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; font-size: 0.875rem; font-weight: 500; {}",
                                    if artist_input.read().trim().is_empty() {
                                        "background: #9ca3af; color: white; cursor: not-allowed;"
                                    } else {
                                        "background: #7c3aed; color: white; cursor: pointer; hover:background: #6d28d9;"
                                    }
                                ),
                                disabled: artist_input.read().trim().is_empty(),
                                onclick: move |_| {
                                    let artist_name = artist_input.read().trim().to_string();
                                    if !artist_name.is_empty() {
                                        spawn(async move {
                                            trigger_artist_processing(state, artist_name).await;
                                        });
                                    }
                                },
                                "Process Artist"
                            }
                        }

                        // Show helpful info about cached artists
                        {
                            let state_read = state.read();
                            let artist_tracks = &state_read.track_cache.recent_tracks;
                            let mut artist_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

                            for track in artist_tracks {
                                *artist_counts.entry(track.artist.clone()).or_insert(0) += 1;
                            }

                            let mut sorted_artists: Vec<_> = artist_counts.into_iter().collect();
                            sorted_artists.sort_by(|a, b| b.1.cmp(&a.1));

                            if !sorted_artists.is_empty() {
                                rsx! {
                                    div { style: "margin-top: 1rem; border-top: 1px solid #e5e7eb; padding-top: 1rem;",
                                        p { style: "font-size: 0.75rem; color: #6b7280; margin-bottom: 0.5rem;",
                                            "Top artists in cache (click to auto-fill):"
                                        }
                                        div { style: "display: flex; flex-wrap: wrap; gap: 0.5rem;",
                                            for (artist, count) in sorted_artists.iter().take(8) {
                                                {
                                                    let artist_name = artist.clone();
                                                    rsx! {
                                                        button {
                                                            style: "background: #f3f4f6; color: #374151; padding: 0.25rem 0.5rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem; hover:background: #e5e7eb;",
                                                            onclick: move |_| {
                                                                artist_input.set(artist_name.clone());
                                                            },
                                                            "{artist} ({count})"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                rsx! { div {} }
                            }
                        }
                    }
                }
            }

            // Activity Log
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h3 { style: "font-size: 1.25rem; font-weight: bold; margin: 0;", "Activity Log" }
                    button {
                        style: "background: #6b7280; color: white; padding: 0.25rem 0.5rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem;",
                        onclick: move |_| {
                            state.with_mut(|s| s.scrubber_state.events.clear());
                        },
                        "Clear Log"
                    }
                }

                div { style: "max-height: 400px; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem; padding: 0.5rem;",
                    if scrubber_state.events.is_empty() {
                        div { style: "text-center; color: #6b7280; padding: 2rem;",
                            "No events yet. Start the scrubber to see activity."
                        }
                    } else {
                        div { style: "display: flex; flex-direction: column-reverse; gap: 0.25rem;",
                            for (index, event) in scrubber_state.events.iter().rev().take(100).enumerate() {
                                {
                                    let event = event.clone();
                                    let event_category = event_formatting::get_event_category(&event);
                                    let (icon, color) = match event_category {
                                        "started" => ("🟢", "#059669"),
                                        "stopped" => ("🔴", "#dc2626"),
                                        "sleeping" => ("💤", "#3730a3"),
                                        "track_processed" => ("🎵", "#2563eb"),
                                        "rule_applied" => ("✏️", "#059669"),
                                        "error" => ("❌", "#dc2626"),
                                        "info" => ("ℹ️", "#6b7280"),
                                        "cycle_completed" => ("✅", "#059669"),
                                        "cycle_started" => ("🔄", "#2563eb"),
                                        "anchor_updated" => ("📍", "#f59e0b"),
                                        "tracks_found" => ("🔍", "#7c3aed"),
                                        "track_edited" => ("✅", "#059669"),
                                        "track_edit_failed" => ("❌", "#dc2626"),
                                        "track_skipped" => ("⏭️", "#f59e0b"),
                                        _ => ("ℹ️", "#6b7280"),
                                    };
                                    let formatted_time = event.timestamp.format("%H:%M:%S").to_string();
                                    let message = event_formatting::format_event_message(&event);

                                    rsx! {
                                        div {
                                            key: "{index}",
                                            style: "display: flex; align-items: center; gap: 0.75rem; padding: 0.5rem; border-radius: 0.25rem; font-size: 0.875rem; hover:background: #f9fafb;",
                                            span { style: "color: {color}; font-weight: 500; min-width: 16ch; text-align: right;", "{formatted_time}" }
                                            span { style: "font-size: 1rem; min-width: 1.5rem; text-align: center;", "{icon}" }
                                            span { style: "color: #374151; flex: 1;", "{message}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Timestamp Management
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h3 { style: "font-size: 1.25rem; font-weight: bold; margin: 0;", "Processing Anchor" }
                    {
                        let state_read = state.read();
                        let total_tracks: usize = state_read.track_cache.recent_tracks.len();
                        let recent_tracks_loaded = total_tracks > 0;
                        if recent_tracks_loaded {
                            rsx! {
                                div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                    span { style: "font-size: 0.875rem; color: #059669; background: #d1fae5; padding: 0.25rem 0.5rem; border-radius: 0.25rem;",
                                        "📂 Using cached recent tracks ({total_tracks} tracks)"
                                    }
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
                                                        // Reload cache to get the newly cached tracks
                                                        s.track_cache = TrackCache::load();
                                                    });
                                                }
                                            }
                                        },
                                        "Load More Tracks"
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                button {
                                    style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| async move {
                                        let session_str = state.read().session.clone();
                                        if let Some(session_str) = session_str {
                                            if load_recent_tracks_from_page(session_str, 1).await.is_ok() {
                                                state.with_mut(|s| {
                                                    s.current_page = 1;
                                                    // Reload cache to get the newly cached tracks
                                                    s.track_cache = TrackCache::load();
                                                });
                                            }
                                        }
                                    },
                                    "Load Recent Tracks"
                                }
                            }
                        }
                    }
                }

                p { style: "color: #6b7280; margin-bottom: 1rem; font-size: 0.875rem;",
                    "Set the processing anchor to control where the scrubber starts processing. Moving the anchor backwards will cause the scrubber to reprocess older tracks."
                }

                {
                    let state_read = state.read();
                    let all_cached_tracks = state_read.track_cache.recent_tracks.clone();

                    if all_cached_tracks.is_empty() {
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
                                for (index, track) in all_cached_tracks.iter().enumerate() {
                                    {
                                        let track = track.clone();
                                        let timestamp_str = if let Some(ts) = track.timestamp {
                                            let dt = chrono::DateTime::from_timestamp(ts as i64, 0).unwrap_or_else(chrono::Utc::now);
                                            dt.format("%Y-%m-%d %H:%M:%S").to_string()
                                        } else {
                                            "No timestamp".to_string()
                                        };

                                        rsx! {
                                            div {
                                                key: "{index}",
                                                style: "display: flex; justify-content: space-between; align-items: center; padding: 0.75rem; border-bottom: 1px solid #f3f4f6; hover:background: #f9fafb;",
                                                div { style: "flex-grow: 1;",
                                                    div { style: "font-weight: 500; color: #1f2937;", "{track.name}" }
                                                    div { style: "font-size: 0.875rem; color: #6b7280;", "{track.artist}" }
                                                    if let Some(album) = &track.album {
                                                        div { style: "font-size: 0.75rem; color: #9ca3af;", "{album}" }
                                                    }
                                                }
                                                div { style: "text-align: right; margin-right: 1rem;",
                                                    div { style: "font-size: 0.75rem; color: #6b7280;", "{timestamp_str}" }
                                                }
                                                button {
                                                    style: "background: #f59e0b; color: white; padding: 0.25rem 0.75rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem;",
                                                    onclick: move |_| {
                                                        let track_clone = track.clone();
                                                        spawn(async move {
                                                            set_timestamp_anchor(state, track_clone).await;
                                                        });
                                                    },
                                                    "Set Anchor"
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

async fn start_scrubber(mut state: Signal<AppState>) {
    // Set status to starting
    state.with_mut(|s| s.scrubber_state.status = ScrubberStatus::Starting);

    // Get necessary data from state
    let (session_json, storage, saved_rules, config) = {
        let state_read = state.read();
        (
            state_read.session.clone(),
            state_read.storage.clone(),
            state_read.saved_rules.clone(),
            state_read.config.clone(),
        )
    };

    if let (Some(session_json), Some(storage), Some(config)) = (session_json, storage, config) {
        match create_scrubber_instance(session_json, storage, saved_rules, config).await {
            Ok(scrubber) => {
                // Create event channel for UI updates
                let (sender, _receiver) = broadcast::channel(1000);
                let sender_arc = Arc::new(sender);

                let start_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Started(
                        "Scrobble scrubber started".to_string(),
                    ),
                };

                let _ = sender_arc.send(start_event.clone());

                state.with_mut(|s| {
                    s.scrubber_state.status = ScrubberStatus::Running;
                    s.scrubber_state.events.push(start_event);
                    s.scrubber_state.event_sender = Some(sender_arc.clone());
                });

                // Start the scrubber background task
                spawn(run_scrubber_with_instance(state, scrubber, sender_arc));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                        "Failed to start scrubber: {e}"
                    )),
                };

                state.with_mut(|s| {
                    s.scrubber_state.status = ScrubberStatus::Error(e.to_string());
                    s.scrubber_state.events.push(error_event);
                });
            }
        }
    } else {
        let error_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                "Cannot start scrubber: missing session, storage, or config".to_string(),
            ),
        };

        state.with_mut(|s| {
            s.scrubber_state.status = ScrubberStatus::Error("Missing configuration".to_string());
            s.scrubber_state.events.push(error_event);
        });
    }
}

async fn stop_scrubber(mut state: Signal<AppState>) {
    state.with_mut(|s| s.scrubber_state.status = ScrubberStatus::Stopping);

    let stop_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Stopped(
            "Scrobble scrubber stopped".to_string(),
        ),
    };

    state.with_mut(|s| {
        s.scrubber_state.status = ScrubberStatus::Stopped;
        s.scrubber_state.events.push(stop_event);
        s.scrubber_state.event_sender = None;
    });
}

async fn trigger_manual_processing(mut state: Signal<AppState>) {
    let process_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
            "Manual processing triggered".to_string(),
        ),
    };

    // Create a temporary sender for this operation if we don't have one
    let sender = if let Some(existing_sender) = state.read().scrubber_state.event_sender.clone() {
        existing_sender
    } else {
        let (new_sender, _) = broadcast::channel(1000);
        Arc::new(new_sender)
    };

    let _ = sender.send(process_event.clone());
    state.with_mut(|s| s.scrubber_state.events.push(process_event));

    // Get necessary data from state for manual processing
    let (session_json, storage, saved_rules, config) = {
        let state_read = state.read();
        (
            state_read.session.clone(),
            state_read.storage.clone(),
            state_read.saved_rules.clone(),
            state_read.config.clone(),
        )
    };

    if let (Some(session_json), Some(storage), Some(config)) = (session_json, storage, config) {
        match create_scrubber_instance(session_json, storage, saved_rules, config).await {
            Ok(mut scrubber) => {
                match process_with_scrubber(&mut scrubber, &sender, &mut state).await {
                    Ok(()) => {
                        let success_event = ScrubberEvent {
                            timestamp: Utc::now(),
                            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                                "Manual processing completed successfully".to_string(),
                            ),
                        };
                        let _ = sender.send(success_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(success_event));
                    }
                    Err(e) => {
                        let error_event = ScrubberEvent {
                            timestamp: Utc::now(),
                            event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                                format!("Manual processing failed: {e}"),
                            ),
                        };
                        let _ = sender.send(error_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(error_event));
                    }
                }
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                        "Failed to create scrubber: {e}"
                    )),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                "Cannot process: missing session, storage, or config".to_string(),
            ),
        };

        let _ = sender.send(warning_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(warning_event));
    }
}

async fn set_timestamp_anchor(mut state: Signal<AppState>, track: Track) {
    let set_anchor_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(format!(
            "Setting timestamp anchor to '{}' by '{}'...",
            track.name, track.artist
        )),
    };

    // Always add to events, whether scrubber is running or not
    if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
        let _ = sender.send(set_anchor_event.clone());
    }
    state.with_mut(|s| s.scrubber_state.events.push(set_anchor_event));

    // Get necessary data from state - only need storage
    let storage = {
        let state_read = state.read();
        state_read.storage.clone()
    };

    // Only proceed if we have storage
    if let Some(storage) = storage {
        match set_timestamp_anchor_direct(storage, track, &mut state).await {
            Ok(()) => {
                let success_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                        "Successfully set timestamp anchor".to_string(),
                    ),
                };
                if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
                    let _ = sender.send(success_event.clone());
                }
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                        "Failed to set timestamp anchor: {e}"
                    )),
                };
                if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
                    let _ = sender.send(error_event.clone());
                }
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                "Cannot set timestamp anchor: missing storage".to_string(),
            ),
        };

        if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
            let _ = sender.send(warning_event.clone());
        }
        state.with_mut(|s| s.scrubber_state.events.push(warning_event));
    }
}

async fn set_timestamp_anchor_direct(
    storage: Arc<tokio::sync::Mutex<::scrobble_scrubber::persistence::FileStorage>>,
    track: Track,
    state: &mut Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::persistence::{StateStorage, TimestampState};
    use chrono::{DateTime, Utc};

    // Track is already the correct type, no conversion needed
    let lastfm_track = track;

    // Use storage directly to set the timestamp anchor
    let mut storage_guard = storage.lock().await;

    // Set the timestamp anchor to the track's timestamp
    if let Some(timestamp) = lastfm_track.timestamp {
        // Convert timestamp to DateTime<Utc>
        let timestamp_dt = DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now);

        // Create new timestamp state
        let timestamp_state = TimestampState {
            last_processed_timestamp: Some(timestamp_dt),
        };

        // Save the updated timestamp state
        storage_guard.save_timestamp_state(&timestamp_state).await?;

        let info_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(format!(
                "Set timestamp anchor to track '{}' by '{}' (timestamp: {})",
                lastfm_track.name,
                lastfm_track.artist,
                timestamp_dt.format("%Y-%m-%d %H:%M:%S")
            )),
        };

        state.with_mut(|s| s.scrubber_state.events.push(info_event));
    } else {
        return Err("Track has no timestamp - cannot set as anchor".into());
    }

    Ok(())
}

// Helper function to create a scrubber instance
async fn create_scrubber_instance(
    session_json: String,
    storage: Arc<tokio::sync::Mutex<::scrobble_scrubber::persistence::FileStorage>>,
    saved_rules: Vec<::scrobble_scrubber::rewrite::RewriteRule>,
    config: ::scrobble_scrubber::config::ScrobbleScrubberConfig,
) -> Result<
    ::scrobble_scrubber::scrubber::ScrobbleScrubber<
        ::scrobble_scrubber::persistence::FileStorage,
        ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider,
    >,
    Box<dyn std::error::Error + Send + Sync>,
> {
    use ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
    use ::scrobble_scrubber::scrubber::ScrobbleScrubber;
    use lastfm_edit::{LastFmEditClientImpl, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = serde_json::from_str(&session_json)?;

    // Create a client with the restored session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClientImpl::from_session(Box::new(http_client), session);

    // Create action provider with current rules
    let action_provider = RewriteRulesScrubActionProvider::from_rules(saved_rules);

    // Create scrubber instance
    let scrubber = ScrobbleScrubber::new(
        storage.clone(),
        Box::new(client),
        action_provider,
        config.clone(),
    );

    // Start event logger for JSON logging of edit attempts
    {
        let event_receiver = scrubber.subscribe_events();
        let log_file_path = ::scrobble_scrubber::config::StorageConfig::get_edit_log_path(
            &config.storage.state_file,
        );
        let mut event_logger = ::scrobble_scrubber::event_logger::EventLogger::new(
            log_file_path.clone(),
            true,
            event_receiver,
        );

        tokio::spawn(async move {
            // Log to console in web context if needed
            log::info!("Started edit logging to: {log_file_path}");
            event_logger.run().await;
        });
    }

    Ok(scrubber)
}

// Helper function to run the scrubber loop with a single instance
async fn run_scrubber_with_instance(
    mut state: Signal<AppState>,
    mut scrubber: ::scrobble_scrubber::scrubber::ScrobbleScrubber<
        ::scrobble_scrubber::persistence::FileStorage,
        ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider,
    >,
    sender: Arc<broadcast::Sender<ScrubberEvent>>,
) {
    // Get interval from config
    let interval_seconds = state
        .read()
        .config
        .as_ref()
        .map(|c| c.scrubber.interval)
        .unwrap_or(30);

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));

    loop {
        // Check if we should stop
        {
            let current_status = state.read().scrubber_state.status.clone();
            if !matches!(
                current_status,
                ScrubberStatus::Running | ScrubberStatus::Sleeping { .. }
            ) {
                break;
            }
        }

        // Log that we're checking for scrobbles
        let info_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                "Checking for new scrobbles...".to_string(),
            ),
        };

        let _ = sender.send(info_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(info_event));

        // Process with the single scrubber instance
        match process_with_scrubber(&mut scrubber, &sender, &mut state).await {
            Ok(()) => {
                let success_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                        "Processing cycle completed successfully".to_string(),
                    ),
                };
                let _ = sender.send(success_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                        "Error during processing: {e}"
                    )),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| {
                    s.scrubber_state.events.push(error_event);
                    s.scrubber_state.status = ScrubberStatus::Error(e.to_string());
                });
                break;
            }
        }

        // Update status to sleeping (don't add sleeping events to activity log)
        state.with_mut(|s| {
            let until_timestamp = Utc::now() + chrono::Duration::seconds(interval_seconds as i64);
            s.scrubber_state.status = ScrubberStatus::Sleeping { until_timestamp };
            s.scrubber_state.next_cycle_timestamp = Some(until_timestamp);
        });

        // Wait for the interval
        interval.tick().await;

        // Set back to running state when we wake up
        state.with_mut(|s| {
            s.scrubber_state.status = ScrubberStatus::Running;
        });
    }
}

// Renamed from process_scrobbles - now works with a single scrubber instance
async fn process_with_scrubber(
    scrubber: &mut ::scrobble_scrubber::scrubber::ScrobbleScrubber<
        ::scrobble_scrubber::persistence::FileStorage,
        ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider,
    >,
    sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Subscribe to detailed events from the scrubber library
    let mut event_receiver = scrubber.subscribe_events();

    // Start logging before processing
    let start_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
            "Starting cache-based track processing...".to_string(),
        ),
    };
    let _ = sender.send(start_event.clone());
    state.with_mut(|s| s.scrubber_state.events.push(start_event));

    // Run a single processing cycle using cache-based processing
    let processing_result = scrubber.trigger_run().await;

    // Process events after scrubbing completes to collect and compress them
    let mut tracks_processed = 0;
    let mut rules_applied = 0;
    let final_message;

    // Track events per track to compress them
    use std::collections::HashMap;
    let mut track_events: HashMap<
        String,
        (::scrobble_scrubber::events::LogTrackInfo, Vec<String>, bool),
    > = HashMap::new(); // (track, rule_descriptions, had_errors)

    // Process events with a timeout to collect statistics
    let mut has_cycle_completed = false;

    while !has_cycle_completed {
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(500),
            event_receiver.recv(),
        )
        .await
        {
            Ok(Ok(lib_event)) => {
                // Process events and group by track
                match &lib_event.event_type {
                    ::scrobble_scrubber::events::ScrubberEventType::TrackProcessed {
                        track,
                        ..
                    } => {
                        tracks_processed += 1;
                        let track_key = format!("{}:{}", track.artist, track.name);
                        let log_track = ::scrobble_scrubber::events::LogTrackInfo {
                            name: track.name.clone(),
                            artist: track.artist.clone(),
                            album: track.album.clone(),
                            album_artist: track.album_artist.clone(),
                            timestamp: track.timestamp,
                            playcount: track.playcount,
                        };
                        track_events
                            .entry(track_key)
                            .or_insert((log_track, Vec::new(), false));
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::RuleApplied {
                        track,
                        description,
                        ..
                    } => {
                        rules_applied += 1;
                        let track_key = format!("{}:{}", track.artist, track.name);
                        let log_track = ::scrobble_scrubber::events::LogTrackInfo {
                            name: track.name.clone(),
                            artist: track.artist.clone(),
                            album: track.album.clone(),
                            album_artist: track.album_artist.clone(),
                            timestamp: track.timestamp,
                            playcount: track.playcount,
                        };
                        track_events
                            .entry(track_key.clone())
                            .or_insert((log_track, Vec::new(), false))
                            .1
                            .push(description.clone());
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::TrackEditFailed {
                        track,
                        ..
                    } => {
                        let track_key = format!("{}:{}", track.artist, track.name);
                        track_events
                            .entry(track_key.clone())
                            .or_insert((track.clone(), Vec::new(), false))
                            .2 = true;
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::CycleCompleted { .. } => {
                        has_cycle_completed = true;
                        // Don't forward the library's cycle completed event - we'll create our own summary
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::AnchorUpdated {
                        anchor_timestamp,
                        ..
                    } => {
                        // Forward anchor updates immediately as they're important
                        let _ = sender.send(lib_event.clone());
                        state.with_mut(|s| {
                            s.scrubber_state.events.push(lib_event.clone());
                            s.scrubber_state.current_anchor_timestamp = Some(*anchor_timestamp);
                        });
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::Info(_)
                    | ::scrobble_scrubber::events::ScrubberEventType::Error(_)
                    | ::scrobble_scrubber::events::ScrubberEventType::CycleStarted(_) => {
                        // Forward non-track events immediately
                        let _ = sender.send(lib_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(lib_event.clone()));
                    }
                    _ => {
                        // Forward other events as-is
                        let _ = sender.send(lib_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(lib_event.clone()));
                    }
                }

                // Update global counters
                state.with_mut(|s| match &lib_event.event_type {
                    ::scrobble_scrubber::events::ScrubberEventType::TrackProcessed { .. } => {
                        s.scrubber_state.processed_count += 1;
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::RuleApplied { .. } => {
                        s.scrubber_state.rules_applied_count += 1;
                    }
                    _ => {}
                });
            }
            Ok(Err(_)) => break, // Channel closed
            Err(_) => break,     // Timeout - stop waiting
        }
    }

    // Now create compressed summary events for each track
    for (track, rule_descriptions, had_errors) in track_events.values() {
        let summary_message = if rule_descriptions.is_empty() {
            if *had_errors {
                format!(
                    "'{}' by '{}' - processed with errors",
                    track.name, track.artist
                )
            } else {
                format!("'{}' by '{}' - no changes needed", track.name, track.artist)
            }
        } else {
            let rules_text = if rule_descriptions.len() == 1 {
                format!("applied rule: {}", rule_descriptions[0])
            } else {
                format!(
                    "applied {} rules: {}",
                    rule_descriptions.len(),
                    rule_descriptions.join(", ")
                )
            };

            if *had_errors {
                format!(
                    "'{}' by '{}' - {} (with errors)",
                    track.name, track.artist, rules_text
                )
            } else {
                format!("'{}' by '{}' - {}", track.name, track.artist, rules_text)
            }
        };

        let summary_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(summary_message),
        };

        let _ = sender.send(summary_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(summary_event));
    }

    match processing_result {
        Ok(()) => {
            // Always use the local counts we tracked, as they're more reliable
            final_message = format!("Cache-based processing completed: {tracks_processed} tracks processed, {rules_applied} rules applied");

            let success_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(final_message),
            };
            let _ = sender.send(success_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(success_event));
        }
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                    "Scrubber processing failed: {e}"
                )),
            };
            let _ = sender.send(error_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(error_event));
            return Err(e.into());
        }
    }

    Ok(())
}

async fn trigger_artist_processing(mut state: Signal<AppState>, artist_name: String) {
    let start_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(format!(
            "Starting artist processing for '{artist_name}'..."
        )),
    };

    // Create a temporary sender for this operation if we don't have one
    let sender = if let Some(existing_sender) = state.read().scrubber_state.event_sender.clone() {
        existing_sender
    } else {
        let (new_sender, _) = broadcast::channel(1000);
        Arc::new(new_sender)
    };

    let _ = sender.send(start_event.clone());
    state.with_mut(|s| s.scrubber_state.events.push(start_event));

    // Get necessary data from state for artist processing
    let (session_json, storage, saved_rules, config) = {
        let state_read = state.read();
        (
            state_read.session.clone(),
            state_read.storage.clone(),
            state_read.saved_rules.clone(),
            state_read.config.clone(),
        )
    };

    if let (Some(session_json), Some(storage), Some(config)) = (session_json, storage, config) {
        match create_scrubber_instance(session_json, storage, saved_rules, config).await {
            Ok(mut scrubber) => {
                // Use the same event processing pattern as process_with_scrubber
                match process_artist_with_events(&mut scrubber, &sender, &mut state, &artist_name)
                    .await
                {
                    Ok(()) => {
                        let success_event = ScrubberEvent {
                            timestamp: Utc::now(),
                            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                                format!(
                                    "Artist processing completed successfully for '{artist_name}'"
                                ),
                            ),
                        };
                        let _ = sender.send(success_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(success_event));
                    }
                    Err(e) => {
                        let error_event = ScrubberEvent {
                            timestamp: Utc::now(),
                            event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                                format!("Artist processing failed for '{artist_name}': {e}"),
                            ),
                        };
                        let _ = sender.send(error_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(error_event));
                    }
                }
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                        "Failed to create scrubber for artist processing: {e}"
                    )),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                "Cannot process artist: missing session, storage, or config".to_string(),
            ),
        };

        let _ = sender.send(warning_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(warning_event));
    }
}

// Process artist with detailed event handling similar to process_with_scrubber
async fn process_artist_with_events(
    scrubber: &mut ::scrobble_scrubber::scrubber::ScrobbleScrubber<
        ::scrobble_scrubber::persistence::FileStorage,
        ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider,
    >,
    sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
    artist_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Subscribe to detailed events from the scrubber library
    let mut event_receiver = scrubber.subscribe_events();

    // Start logging before processing
    let start_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(format!(
            "Starting artist track processing for '{artist_name}'..."
        )),
    };
    let _ = sender.send(start_event.clone());
    state.with_mut(|s| s.scrubber_state.events.push(start_event));

    // Run artist processing
    let processing_result = scrubber.process_artist(artist_name).await;

    // Process events after scrubbing completes to collect and compress them
    let mut tracks_processed = 0;
    let mut rules_applied = 0;

    // Track events per track to compress them
    use std::collections::HashMap;
    let mut track_events: HashMap<
        String,
        (::scrobble_scrubber::events::LogTrackInfo, Vec<String>, bool),
    > = HashMap::new(); // (track, rule_descriptions, had_errors)

    // Process events with a timeout to collect statistics
    let mut has_cycle_completed = false;

    while !has_cycle_completed {
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(500),
            event_receiver.recv(),
        )
        .await
        {
            Ok(Ok(lib_event)) => {
                // Process events and group by track
                match &lib_event.event_type {
                    ::scrobble_scrubber::events::ScrubberEventType::TrackProcessed {
                        track,
                        ..
                    } => {
                        tracks_processed += 1;
                        let track_key = format!("{}:{}", track.artist, track.name);
                        let log_track = ::scrobble_scrubber::events::LogTrackInfo {
                            name: track.name.clone(),
                            artist: track.artist.clone(),
                            album: track.album.clone(),
                            album_artist: track.album_artist.clone(),
                            timestamp: track.timestamp,
                            playcount: track.playcount,
                        };
                        track_events
                            .entry(track_key)
                            .or_insert((log_track, Vec::new(), false));
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::RuleApplied {
                        track,
                        description,
                        ..
                    } => {
                        rules_applied += 1;
                        let track_key = format!("{}:{}", track.artist, track.name);
                        let log_track = ::scrobble_scrubber::events::LogTrackInfo {
                            name: track.name.clone(),
                            artist: track.artist.clone(),
                            album: track.album.clone(),
                            album_artist: track.album_artist.clone(),
                            timestamp: track.timestamp,
                            playcount: track.playcount,
                        };
                        track_events
                            .entry(track_key.clone())
                            .or_insert((log_track, Vec::new(), false))
                            .1
                            .push(description.clone());
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::TrackEditFailed {
                        track,
                        ..
                    } => {
                        let track_key = format!("{}:{}", track.artist, track.name);
                        track_events
                            .entry(track_key.clone())
                            .or_insert((track.clone(), Vec::new(), false))
                            .2 = true;
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::CycleCompleted { .. } => {
                        has_cycle_completed = true;
                        // Don't forward the library's cycle completed event - we'll create our own summary
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::AnchorUpdated {
                        anchor_timestamp,
                        ..
                    } => {
                        // Forward anchor updates immediately as they're important
                        let _ = sender.send(lib_event.clone());
                        state.with_mut(|s| {
                            s.scrubber_state.events.push(lib_event.clone());
                            s.scrubber_state.current_anchor_timestamp = Some(*anchor_timestamp);
                        });
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::Info(_)
                    | ::scrobble_scrubber::events::ScrubberEventType::Error(_)
                    | ::scrobble_scrubber::events::ScrubberEventType::CycleStarted(_) => {
                        // Forward non-track events immediately
                        let _ = sender.send(lib_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(lib_event.clone()));
                    }
                    _ => {
                        // Forward other events as-is
                        let _ = sender.send(lib_event.clone());
                        state.with_mut(|s| s.scrubber_state.events.push(lib_event.clone()));
                    }
                }

                // Update global counters
                state.with_mut(|s| match &lib_event.event_type {
                    ::scrobble_scrubber::events::ScrubberEventType::TrackProcessed { .. } => {
                        s.scrubber_state.processed_count += 1;
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::RuleApplied { .. } => {
                        s.scrubber_state.rules_applied_count += 1;
                    }
                    _ => {}
                });
            }
            Ok(Err(_)) => break, // Channel closed
            Err(_) => break,     // Timeout - stop waiting
        }
    }

    // Now create compressed summary events for each track
    for (track, rule_descriptions, had_errors) in track_events.values() {
        let summary_message = if rule_descriptions.is_empty() {
            if *had_errors {
                format!(
                    "'{}' by '{}' - processed with errors",
                    track.name, track.artist
                )
            } else {
                format!("'{}' by '{}' - no changes needed", track.name, track.artist)
            }
        } else {
            let rules_text = if rule_descriptions.len() == 1 {
                format!("applied rule: {}", rule_descriptions[0])
            } else {
                format!(
                    "applied {} rules: {}",
                    rule_descriptions.len(),
                    rule_descriptions.join(", ")
                )
            };

            if *had_errors {
                format!(
                    "'{}' by '{}' - {} (with errors)",
                    track.name, track.artist, rules_text
                )
            } else {
                format!("'{}' by '{}' - {}", track.name, track.artist, rules_text)
            }
        };

        let summary_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(summary_message),
        };

        let _ = sender.send(summary_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(summary_event));
    }

    match processing_result {
        Ok(()) => {
            // Always use the local counts we tracked, as they're more reliable
            let final_message = format!("Artist processing completed for '{artist_name}': {tracks_processed} tracks processed, {rules_applied} rules applied");

            let success_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(final_message),
            };
            let _ = sender.send(success_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(success_event));
        }
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                    "Artist processing failed for '{artist_name}': {e}"
                )),
            };
            let _ = sender.send(error_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(error_event));
            return Err(e.into());
        }
    }

    Ok(())
}
