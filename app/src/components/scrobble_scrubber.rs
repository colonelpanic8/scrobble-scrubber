use crate::types::{AppState, ScrubberEvent, ScrubberEventType, ScrubberStatus};
use chrono::Utc;
use dioxus::prelude::*;
use lastfm_edit::iterator::AsyncPaginatedIterator;
use std::sync::Arc;
use tokio::sync::broadcast;

#[component]
pub fn ScrobbleScrubberPage(mut state: Signal<AppState>) -> Element {
    let scrubber_state = state.read().scrubber_state.clone();

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            // Header with controls
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h2 { style: "font-size: 1.5rem; font-weight: bold; margin: 0;", "Scrobble Scrubber" }
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
                                    style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        spawn(async move {
                                            start_scrubber(state).await;
                                        });
                                    },
                                    "Start Scrubber"
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
                                    style: "background: #2563eb; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; margin-right: 0.5rem;",
                                    onclick: move |_| {
                                        spawn(async move {
                                            trigger_manual_processing(state).await;
                                        });
                                    },
                                    "Process Now"
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
                            for event in scrubber_state.events.iter().take(100) {
                                {
                                    let event = event.clone();
                                    let (icon, color) = match event.event_type {
                                        ScrubberEventType::Started => ("🟢", "#059669"),
                                        ScrubberEventType::Stopped => ("🔴", "#dc2626"),
                                        ScrubberEventType::TrackProcessed => ("🎵", "#2563eb"),
                                        ScrubberEventType::RuleApplied => ("✏️", "#059669"),
                                        ScrubberEventType::Error => ("❌", "#dc2626"),
                                        ScrubberEventType::Info => ("ℹ️", "#6b7280"),
                                    };
                                    let formatted_time = event.timestamp.format("%H:%M:%S").to_string();

                                    rsx! {
                                        div {
                                            key: "{event.timestamp}",
                                            style: "display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; border-radius: 0.25rem; font-size: 0.875rem; hover:background: #f9fafb;",
                                            span { style: "font-size: 1rem;", "{icon}" }
                                            span { style: "color: {color}; font-weight: 500; min-width: 16ch;", "{formatted_time}" }
                                            span { style: "color: #374151;", "{event.message}" }
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
                    button {
                        style: "background: #8b5cf6; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| {
                            spawn(async move {
                                load_recent_tracks_for_timestamp(state).await;
                            });
                        },
                        "Load Recent Tracks"
                    }
                }

                p { style: "color: #6b7280; margin-bottom: 1rem; font-size: 0.875rem;",
                    "Set the processing anchor to control where the scrubber starts processing. Moving the anchor backwards will cause the scrubber to reprocess older tracks."
                }

                if state.read().recent_tracks.tracks.is_empty() {
                    div { style: "text-align: center; color: #6b7280; padding: 2rem;",
                        "Click 'Load Recent Tracks' to see your recent scrobbles and set the processing anchor."
                    }
                } else {
                    div { style: "max-height: 400px; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                        for (index, track) in state.read().recent_tracks.tracks.iter().enumerate().take(50) {
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

async fn start_scrubber(mut state: Signal<AppState>) {
    // Set status to starting
    state.with_mut(|s| s.scrubber_state.status = ScrubberStatus::Starting);

    // Create event channel
    let (sender, _receiver) = broadcast::channel(1000);
    let sender_arc = Arc::new(sender);

    // Create initial event
    let start_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ScrubberEventType::Started,
        message: "Scrobble scrubber started".to_string(),
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
        event_type: ScrubberEventType::Stopped,
        message: "Scrobble scrubber stopped".to_string(),
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
        event_type: ScrubberEventType::Info,
        message: "Manual processing triggered".to_string(),
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
                    event_type: ScrubberEventType::Info,
                    message: "Manual processing completed successfully".to_string(),
                };
                let _ = sender.send(success_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Error,
                    message: format!("Manual processing failed: {e}"),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::Info,
            message: "Cannot process: missing session, storage, or event sender".to_string(),
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
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

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
            event_type: ScrubberEventType::Info,
            message: "Checking for new scrobbles...".to_string(),
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
            // Attempt to recreate LastFmEditSession and process scrobbles
            match process_scrobbles(session_json, storage, saved_rules, &sender, &mut state).await {
                Ok(()) => {
                    let success_event = ScrubberEvent {
                        timestamp: Utc::now(),
                        event_type: ScrubberEventType::Info,
                        message: "Processing cycle completed successfully".to_string(),
                    };
                    let _ = sender.send(success_event.clone());
                    state.with_mut(|s| s.scrubber_state.events.push(success_event));
                }
                Err(e) => {
                    let error_event = ScrubberEvent {
                        timestamp: Utc::now(),
                        event_type: ScrubberEventType::Error,
                        message: format!("Error during processing: {e}"),
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
                event_type: ScrubberEventType::Info,
                message: "No session or storage available - skipping processing".to_string(),
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
    use lastfm_edit::{LastFmEditClient, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = serde_json::from_str(&session_json)?;

    // Create a client with the restored session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClient::from_session(Box::new(http_client), session);

    // Get the config from state
    let config = {
        let state_read = state.read();
        state_read.config.clone()
    };

    if let Some(config) = config {
        // Create action provider with current rules
        let action_provider = RewriteRulesScrubActionProvider::from_rules(saved_rules);

        // Create scrubber instance
        let mut scrubber =
            ScrobbleScrubber::new(storage.clone(), client, action_provider, config.clone());

        // Run a single processing cycle (process last 50 tracks)
        let processing_result = scrubber.process_last_n_tracks(50).await;

        match processing_result {
            Ok(()) => {
                let success_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Info,
                    message: "Scrubber processing completed successfully".to_string(),
                };
                let _ = sender.send(success_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Error,
                    message: format!("Scrubber processing failed: {e}"),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
                return Err(e.into());
            }
        }
    } else {
        let config_error = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::Error,
            message: "No configuration available for scrubber".to_string(),
        };
        let _ = sender.send(config_error.clone());
        state.with_mut(|s| s.scrubber_state.events.push(config_error));
        return Err("No configuration available".into());
    }

    Ok(())
}

async fn trigger_immediate_cycle(mut state: Signal<AppState>) {
    let trigger_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ScrubberEventType::Info,
        message: "Immediate processing cycle triggered".to_string(),
    };

    if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
        let _ = sender.send(trigger_event.clone());
    }

    state.with_mut(|s| s.scrubber_state.events.push(trigger_event));

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
                    event_type: ScrubberEventType::Info,
                    message: "Immediate processing trigger sent successfully".to_string(),
                };
                let _ = sender.send(success_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Error,
                    message: format!("Failed to trigger immediate processing: {e}"),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::Info,
            message:
                "Cannot trigger immediate processing: missing session, storage, or event sender"
                    .to_string(),
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
    sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
    use ::scrobble_scrubber::scrubber::ScrobbleScrubber;
    use lastfm_edit::{LastFmEditClient, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = serde_json::from_str(&session_json)?;

    // Create a client with the restored session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClient::from_session(Box::new(http_client), session);

    // Get the config from state
    let config = {
        let state_read = state.read();
        state_read.config.clone()
    };

    if let Some(config) = config {
        // Create action provider with current rules
        let action_provider = RewriteRulesScrubActionProvider::from_rules(saved_rules);

        // Create scrubber instance
        let scrubber =
            ScrobbleScrubber::new(storage.clone(), client, action_provider, config.clone());

        // Trigger immediate processing
        scrubber.trigger_immediate_processing().await;

        let info_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::Info,
            message: "Sent immediate processing trigger to running scrubber".to_string(),
        };
        let _ = sender.send(info_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(info_event));
    } else {
        return Err("No configuration available".into());
    }

    Ok(())
}

async fn load_recent_tracks_for_timestamp(mut state: Signal<AppState>) {
    let load_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ScrubberEventType::Info,
        message: "Loading recent tracks for timestamp management...".to_string(),
    };

    if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
        let _ = sender.send(load_event.clone());
    }

    state.with_mut(|s| s.scrubber_state.events.push(load_event));

    // Get session from state
    let session_json = state.read().session.clone();

    if let Some(session_json) = session_json {
        // Create client and fetch recent tracks
        match serde_json::from_str::<lastfm_edit::LastFmEditSession>(&session_json) {
            Ok(session) => {
                let http_client = http_client::native::NativeClient::new();
                let client =
                    lastfm_edit::LastFmEditClient::from_session(Box::new(http_client), session);

                let mut recent_iterator = client.recent_tracks();
                let mut tracks = Vec::new();
                let mut count = 0;

                // Fetch up to 50 recent tracks
                while let Ok(Some(track)) = recent_iterator.next().await {
                    tracks.push(crate::types::SerializableTrack::from(track));
                    count += 1;
                    if count >= 50 {
                        break;
                    }
                }

                // Update state with recent tracks
                state.with_mut(|s| {
                    s.recent_tracks.tracks = tracks;
                });

                let success_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Info,
                    message: format!("Loaded {count} recent tracks"),
                };

                if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
                    let _ = sender.send(success_event.clone());
                }

                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Error,
                    message: format!("Failed to deserialize session: {e}"),
                };

                if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
                    let _ = sender.send(error_event.clone());
                }

                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let no_session_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::Info,
            message: "No session available for loading tracks".to_string(),
        };

        if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
            let _ = sender.send(no_session_event.clone());
        }

        state.with_mut(|s| s.scrubber_state.events.push(no_session_event));
    }
}

async fn set_timestamp_anchor(mut state: Signal<AppState>, track: crate::types::SerializableTrack) {
    let set_anchor_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ScrubberEventType::Info,
        message: format!(
            "Setting timestamp anchor to '{}' by '{}'...",
            track.name, track.artist
        ),
    };

    if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
        let _ = sender.send(set_anchor_event.clone());
    }

    state.with_mut(|s| s.scrubber_state.events.push(set_anchor_event));

    // Get necessary data from state
    let (session_json, storage, saved_rules, sender_opt) = {
        let state_read = state.read();
        (
            state_read.session.clone(),
            state_read.storage.clone(),
            state_read.saved_rules.clone(),
            state_read.scrubber_state.event_sender.clone(),
        )
    };

    // Only proceed if we have session, storage, and event sender
    if let (Some(session_json), Some(storage), Some(sender)) = (session_json, storage, sender_opt) {
        match create_scrubber_and_set_timestamp(
            session_json,
            storage,
            saved_rules,
            track,
            &sender,
            &mut state,
        )
        .await
        {
            Ok(()) => {
                let success_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Info,
                    message: "Successfully set timestamp anchor".to_string(),
                };
                let _ = sender.send(success_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(success_event));
            }
            Err(e) => {
                let error_event = ScrubberEvent {
                    timestamp: Utc::now(),
                    event_type: ScrubberEventType::Error,
                    message: format!("Failed to set timestamp anchor: {e}"),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        }
    } else {
        let warning_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::Info,
            message: "Cannot set timestamp anchor: missing session, storage, or event sender"
                .to_string(),
        };

        if let Some(sender) = state.read().scrubber_state.event_sender.clone() {
            let _ = sender.send(warning_event.clone());
        }
        state.with_mut(|s| s.scrubber_state.events.push(warning_event));
    }
}

async fn create_scrubber_and_set_timestamp(
    session_json: String,
    storage: Arc<tokio::sync::Mutex<::scrobble_scrubber::persistence::FileStorage>>,
    saved_rules: Vec<::scrobble_scrubber::rewrite::RewriteRule>,
    track: crate::types::SerializableTrack,
    sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
    use ::scrobble_scrubber::scrubber::ScrobbleScrubber;
    use lastfm_edit::{LastFmEditClient, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = serde_json::from_str(&session_json)?;

    // Create a client with the restored session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClient::from_session(Box::new(http_client), session);

    // Get the config from state
    let config = {
        let state_read = state.read();
        state_read.config.clone()
    };

    if let Some(config) = config {
        // Create action provider with current rules
        let action_provider = RewriteRulesScrubActionProvider::from_rules(saved_rules);

        // Create scrubber instance
        let mut scrubber =
            ScrobbleScrubber::new(storage.clone(), client, action_provider, config.clone());

        // Convert SerializableTrack back to lastfm_edit::Track
        let lastfm_track = lastfm_edit::Track {
            name: track.name,
            artist: track.artist,
            album: track.album,
            playcount: track.playcount,
            timestamp: track.timestamp,
        };

        // Set timestamp to track
        scrubber.set_timestamp_to_track(&lastfm_track).await?;

        let info_event = ScrubberEvent {
            timestamp: Utc::now(),
            event_type: ScrubberEventType::Info,
            message: format!(
                "Set timestamp anchor to track '{}' by '{}'",
                lastfm_track.name, lastfm_track.artist
            ),
        };
        let _ = sender.send(info_event.clone());
        state.with_mut(|s| s.scrubber_state.events.push(info_event));
    } else {
        return Err("No configuration available".into());
    }

    Ok(())
}
