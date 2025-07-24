use crate::server_functions::load_recent_tracks_from_page;
use crate::types::{event_formatting, AppState, ScrubberStatus};
use ::scrobble_scrubber::events::ScrubberEvent;
use ::scrobble_scrubber::track_cache::TrackCache;
use chrono::Utc;
use dioxus::prelude::*;
use lastfm_edit::Track;
use std::sync::Arc;
use tokio::sync::broadcast;

#[component]
pub fn ScrobbleScrubberPage(mut state: Signal<AppState>) -> Element {
    let scrubber_state = state.read().scrubber_state.clone();

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
                                    ScrubberStatus::Stopping => "background: #fef3c7; color: #92400e;",
                                    ScrubberStatus::Error(_) => "background: #fecaca; color: #dc2626;",
                                }
                            ),
                            {match &scrubber_state.status {
                                ScrubberStatus::Stopped => "Stopped".to_string(),
                                ScrubberStatus::Starting => "Starting...".to_string(),
                                ScrubberStatus::Running => "Running".to_string(),
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
                            ScrubberStatus::Running => rsx! {
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
                                            trigger_immediate_cycle(state).await;
                                        });
                                    },
                                    "Trigger Now"
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
                                            style: "display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; border-radius: 0.25rem; font-size: 0.875rem; hover:background: #f9fafb;",
                                            span { style: "font-size: 1rem;", "{icon}" }
                                            span { style: "color: {color}; font-weight: 500; min-width: 16ch;", "{formatted_time}" }
                                            span { style: "color: #374151;", "{message}" }
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
                                                    div { style: "font-size: 0.875rem; color: #6b7280;", "by {track.artist}" }
                                                    if let Some(album) = &track.album {
                                                        div { style: "font-size: 0.75rem; color: #9ca3af;", "from {album}" }
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

    // Create event channel
    let (sender, _receiver) = broadcast::channel(1000);
    let sender_arc = Arc::new(sender);

    // Create initial event using the library event system
    let start_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Started("Scrobble scrubber started".to_string()),
    };

    // Send event and update state
    let _ = sender_arc.send(start_event.clone());

    state.with_mut(|s| {
        s.scrubber_state.status = ScrubberStatus::Running;
        s.scrubber_state.events.push(start_event);
        s.scrubber_state.event_sender = Some(sender_arc.clone());
    });

    // Start the scrubber background task
    let sender_for_task = sender_arc.clone();
    spawn(run_scrubber_loop(state, sender_for_task));
}

async fn stop_scrubber(mut state: Signal<AppState>) {
    state.with_mut(|s| s.scrubber_state.status = ScrubberStatus::Stopping);

    let stop_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Stopped("Scrobble scrubber stopped".to_string()),
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
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Manual processing triggered".to_string()),
    };

    if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
        let _ = sender.send(process_event.clone());
    }

    state.with_mut(|s| s.scrubber_state.events.push(process_event));

    // Get necessary data from state for manual processing
    let (session_json, storage, saved_rules, sender_opt) = {
        let state_read = state.read();
        (
            state_read.session.clone(),
            state_read.storage.clone(),
            state_read.saved_rules.clone(),
            state_read.scrubber_state.event_sender.clone(),
        )
    };

    // Only proceed if we have a session, storage, and event sender
    if let (Some(session_json), Some(storage), Some(sender)) = (session_json, storage, sender_opt) {
        // Run manual processing
        match process_scrobbles(session_json, storage, saved_rules, &sender, &mut state).await {
            Ok(()) => {
                let success_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Manual processing completed successfully".to_string()),
                };
                let _ = sender.send(success_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!("Manual processing failed: {e}")),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Cannot process: missing session, storage, or event sender".to_string()),
        };

        if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
            let _ = sender.send(warning_event.clone());
        }
        state.with_mut(|s| s.scrubber_state.events.push(warning_event));
    }
}

async fn run_scrubber_loop(
    mut state: Signal<AppState>,
    sender: Arc<broadcast::Sender<ScrubberEvent>>,
) {
    // Get interval from config, default to 30 seconds if not available
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
            if !matches!(current_status, ScrubberStatus::Running) {
                break;
            }
        }

        interval.tick().await;

        // Log that we're checking for scrobbles
        let info_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Checking for new scrobbles...".to_string()),
        };

        let _ = sender.send(info_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(info_event));

        // Get session, storage, and rules from state
        let (session_json, storage, saved_rules) = {
            let state_read = state.read();
            (
                state_read.session.clone(),
                state_read.storage.clone(),
                state_read.saved_rules.clone(),
            )
        };

        // Only proceed if we have a session and storage
        if let (Some(session_json), Some(storage)) = (session_json, storage) {
            // Attempt to recreate session and process scrobbles
            match process_scrobbles(session_json, storage, saved_rules, &sender, &mut state).await {
                Ok(()) => {
                    let success_event = ScrubberEvent {
                        timestamp: Utc::now(),
                        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Processing cycle completed successfully".to_string()),
                    };
                    let _ = sender.send(success_event.clone());
                    state.with_mut(|s| s.scrubber_state.events.push(success_event));
                }
                Err(e) => {
                    let error_event = ScrubberEvent {
                        timestamp: Utc::now(),
                        event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!("Error during processing: {e}")),
                    };
                    let _ = sender.send(error_event.clone());
                    state.with_mut(|s| {
                        s.scrubber_state.events.push(error_event);
                        s.scrubber_state.status = ScrubberStatus::Error(e.to_string());
                    });
                    break;
                }
            }
        } else {
            let warning_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("No session or storage available - skipping processing".to_string()),
            };
            let _ = sender.send(warning_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(warning_event));
        }
    }
}

async fn process_scrobbles(
    session_json: String,
    storage: Arc<tokio::sync::Mutex<::scrobble_scrubber::persistence::FileStorage>>,
    saved_rules: Vec<::scrobble_scrubber::rewrite::RewriteRule>,
    sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
    use ::scrobble_scrubber::scrubber::ScrobbleScrubber;
    use lastfm_edit::{LastFmEditClientImpl, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = serde_json::from_str(&session_json)?;

    // Create a client with the restored session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClientImpl::from_session(Box::new(http_client), session);

    // Get the config from state
    let config = {
        let state_read = state.read();
        state_read.config.clone()
    };

    if let Some(config) = config {
        // Create action provider with current rules
        let action_provider = RewriteRulesScrubActionProvider::from_rules(saved_rules);

        // Create scrubber instance
        let mut scrubber = ScrobbleScrubber::new(
            storage.clone(),
            Box::new(client),
            action_provider,
            config.clone(),
        );

        // Subscribe to detailed events from the scrubber library
        let mut event_receiver = scrubber.subscribe_events();

        // Start logging before processing
        let start_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Starting cache-based track processing...".to_string()),
        };
        let _ = sender.send(start_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(start_event));

        // Run a single processing cycle using cache-based processing
        let processing_result = scrubber.trigger_run().await;

        // Process events after scrubbing completes to collect and compress them
        let mut tracks_processed = 0;
        let mut rules_applied = 0;
        let mut final_message = "Processing completed".to_string();
        
        // Track events per track to compress them
        use std::collections::HashMap;
        let mut track_events: HashMap<String, (::scrobble_scrubber::events::LogTrackInfo, Vec<String>, bool)> = HashMap::new(); // (track, rule_descriptions, had_errors)

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
                        ::scrobble_scrubber::events::ScrubberEventType::TrackProcessed { track, .. } => {
                            tracks_processed += 1;
                            let track_key = format!("{}:{}", track.artist, track.name);
                            let log_track = ::scrobble_scrubber::events::LogTrackInfo::from(track);
                            track_events.entry(track_key).or_insert((log_track, Vec::new(), false));
                        }
                        ::scrobble_scrubber::events::ScrubberEventType::RuleApplied { track, description, .. } => {
                            rules_applied += 1;
                            let track_key = format!("{}:{}", track.artist, track.name);
                            let log_track = ::scrobble_scrubber::events::LogTrackInfo::from(track);
                            track_events.entry(track_key.clone())
                                .or_insert((log_track, Vec::new(), false))
                                .1.push(description.clone());
                        }
                        ::scrobble_scrubber::events::ScrubberEventType::TrackEditFailed { track, .. } => {
                            let track_key = format!("{}:{}", track.artist, track.name);
                            track_events.entry(track_key.clone())
                                .or_insert((track.clone(), Vec::new(), false))
                                .2 = true;
                        }
                        ::scrobble_scrubber::events::ScrubberEventType::CycleCompleted { .. } => {
                            has_cycle_completed = true;
                            // Don't forward the library's cycle completed event - we'll create our own summary
                        }
                        ::scrobble_scrubber::events::ScrubberEventType::AnchorUpdated { anchor_timestamp, .. } => {
                            // Forward anchor updates immediately as they're important
                            let _ = sender.send(lib_event.clone());
                            state.with_mut(|s| {
                                s.scrubber_state.events.push(lib_event.clone());
                                s.scrubber_state.current_anchor_timestamp = Some(*anchor_timestamp);
                            });
                        }
                        ::scrobble_scrubber::events::ScrubberEventType::Info(_) |
                        ::scrobble_scrubber::events::ScrubberEventType::Error(_) |
                        ::scrobble_scrubber::events::ScrubberEventType::CycleStarted(_) => {
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
                    state.with_mut(|s| {
                        match &lib_event.event_type {
                            ::scrobble_scrubber::events::ScrubberEventType::TrackProcessed { .. } => {
                                s.scrubber_state.processed_count += 1;
                            }
                            ::scrobble_scrubber::events::ScrubberEventType::RuleApplied { .. } => {
                                s.scrubber_state.rules_applied_count += 1;
                            }
                            _ => {}
                        }
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
                    format!("'{}' by '{}' - processed with errors", track.name, track.artist)
                } else {
                    format!("'{}' by '{}' - no changes needed", track.name, track.artist)
                }
            } else {
                let rules_text = if rule_descriptions.len() == 1 {
                    format!("applied rule: {}", rule_descriptions[0])
                } else {
                    format!("applied {} rules: {}", rule_descriptions.len(), rule_descriptions.join(", "))
                };
                
                if *had_errors {
                    format!("'{}' by '{}' - {} (with errors)", track.name, track.artist, rules_text)
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
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!("Scrubber processing failed: {e}")),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
                return Err(e.into());
            }
        }
    } else {
        let config_error = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Error("No configuration available for scrubber".to_string()),
        };
        let _ = sender.send(config_error.clone());
        state.with_mut(|s| s.scrubber_state.events.push(config_error));
        return Err("No configuration available".into());
    }

    Ok(())
}

async fn trigger_immediate_cycle(mut state: Signal<AppState>) {
    // Get necessary data from state for triggering immediate processing
    let (session_json, storage, saved_rules, sender_opt) = {
        let state_read = state.read();
        (
            state_read.session.clone(),
            state_read.storage.clone(),
            state_read.saved_rules.clone(),
            state_read.scrubber_state.event_sender.clone(),
        )
    };

    // Only proceed if we have a session, storage, and event sender
    if let (Some(session_json), Some(storage), Some(sender)) = (session_json, storage, sender_opt) {
        // Create scrubber to trigger immediate processing
        match create_scrubber_and_trigger_immediate(
            session_json,
            storage,
            saved_rules,
            &sender,
            &mut state,
        )
        .await
        {
            Ok(()) => {
                let success_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Immediate processing triggered".to_string()),
                };
                let _ = sender.send(success_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!("Failed to trigger immediate processing: {e}")),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                "Cannot trigger immediate processing: missing session, storage, or event sender"
                    .to_string(),
            ),
        };

        if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
            let _ = sender.send(warning_event.clone());
        }
        state.with_mut(|s| s.scrubber_state.events.push(warning_event));
    }
}

async fn create_scrubber_and_trigger_immediate(
    session_json: String,
    storage: Arc<tokio::sync::Mutex<::scrobble_scrubber::persistence::FileStorage>>,
    saved_rules: Vec<::scrobble_scrubber::rewrite::RewriteRule>,
    _sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
    use ::scrobble_scrubber::scrubber::ScrobbleScrubber;
    use lastfm_edit::{LastFmEditClientImpl, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = serde_json::from_str(&session_json)?;

    // Create a client with the restored session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClientImpl::from_session(Box::new(http_client), session);

    // Get the config from state
    let config = {
        let state_read = state.read();
        state_read.config.clone()
    };

    if let Some(config) = config {
        // Create action provider with current rules
        let action_provider = RewriteRulesScrubActionProvider::from_rules(saved_rules);

        // Create scrubber instance
        let scrubber = ScrobbleScrubber::new(
            storage.clone(),
            Box::new(client),
            action_provider,
            config.clone(),
        );

        // Trigger immediate processing
        scrubber.trigger_immediate_processing();
    } else {
        return Err("No configuration available".into());
    }

    Ok(())
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
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Successfully set timestamp anchor".to_string()),
                };
                if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
                    let _ = sender.send(success_event.clone());
                }
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!("Failed to set timestamp anchor: {e}")),
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
            event_type: ::scrobble_scrubber::events::ScrubberEventType::Info("Cannot set timestamp anchor: missing storage".to_string()),
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