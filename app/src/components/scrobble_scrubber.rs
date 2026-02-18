use crate::components::{
    scrubber_controls::handle_scrubber_event, ActivityLogSection, ArtistProcessingSection,
    DelayIndicator, RateLimitIndicator, ScrubberControlsSection, TimestampManagementSection,
    TrackProcessingProgressView,
};
use crate::scrubber_manager::{clear_scrubber_instance, get_or_create_scrubber};
use crate::types::{AppState, ScrubberStatus};
use ::scrobble_scrubber::events::ScrubberEvent;
use chrono::Utc;
use dioxus::prelude::*;
use lastfm_edit::Track;
use std::sync::Arc;
use tokio::sync::broadcast;

#[component]
pub fn ScrobbleScrubberPage(mut state: Signal<AppState>) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            DelayIndicator { state }
            RateLimitIndicator { state }

            ScrubberControlsSection { state }

            TrackProcessingProgressView { state }

            ArtistProcessingSection { state }

            ActivityLogSection { state }

            TimestampManagementSection { state }
        }
    }
}

pub async fn start_scrubber(mut state: Signal<AppState>) {
    // Always recreate a scrubber instance for the background loop.
    //
    // Rationale: the scrubber handle uses a `Notify` for stop. Once notified, it can remain
    // "signaled" and cause a subsequent start to immediately stop without doing work, which
    // looks like "it did one batch and then stopped making progress".
    clear_scrubber_instance(state);
    state.with_mut(|s| s.scrubber_handle = None);

    // Set status to starting
    state.with_mut(|s| s.scrubber_state.status = ScrubberStatus::Starting);

    match get_or_create_scrubber(state).await {
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

            let stop_notify = state
                .read()
                .scrubber_handle
                .as_ref()
                .map(|h| h.stop_notifier())
                .unwrap_or_else(|| Arc::new(tokio::sync::Notify::new()));

            // Start the scrubber background task with the shared instance
            spawn(run_scrubber_with_shared_instance(
                state,
                scrubber,
                sender_arc,
                stop_notify,
            ));
        }
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                    ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                        "Failed to start scrubber: {e}"
                    )),
                ),
            };

            state.with_mut(|s| {
                s.scrubber_state.status = ScrubberStatus::Error(e.to_string());
                s.scrubber_state.events.push(error_event);
            });
        }
    }
}

pub async fn stop_scrubber(mut state: Signal<AppState>) {
    state.with_mut(|s| s.scrubber_state.status = ScrubberStatus::Stopping);

    if let Some(handle) = state.read().scrubber_handle.clone() {
        handle.stop();
    }

    // Force recreation on next start (see comment in `start_scrubber`).
    clear_scrubber_instance(state);
    state.with_mut(|s| s.scrubber_handle = None);
}

pub async fn trigger_manual_processing(mut state: Signal<AppState>) {
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

    // Use the global scrubber instance for manual processing
    match get_or_create_scrubber(state).await {
        Ok(scrubber) => match process_with_shared_scrubber(&scrubber, &sender, &mut state).await {
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
                        ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                            "Manual processing failed: {e}"
                        )),
                    ),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        },
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                    ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                        "Failed to get scrubber: {e}"
                    )),
                ),
            };
            let _ = sender.send(error_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(error_event));
        }
    }
}

pub async fn set_timestamp_anchor(mut state: Signal<AppState>, track: Track) {
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
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                        ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                            "Failed to set timestamp anchor: {e}"
                        )),
                    ),
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
    use ::scrobble_scrubber::persistence::StateStorage;
    use chrono::{DateTime, Utc};

    // Track is already the correct type, no conversion needed
    let lastfm_track = track;

    // Use storage directly to set the timestamp anchor
    let mut storage_guard = storage.lock().await;

    // Set the timestamp anchor to the track's timestamp
    if let Some(timestamp) = lastfm_track.timestamp {
        // Convert timestamp to DateTime<Utc>
        let timestamp_dt = DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(Utc::now);

        // Update the unified frontiers state so the scrubber will honor it even if a prior run
        // created a `frontiers_state` record.
        let mut frontiers = storage_guard.load_frontiers_state().await?;
        frontiers.slow_anchor_timestamp = Some(timestamp_dt);
        storage_guard.save_frontiers_state(&frontiers).await?;

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

// Helper function to run the scrubber loop with a shared instance
async fn run_scrubber_with_shared_instance(
    mut state: Signal<AppState>,
    scrubber: Arc<tokio::sync::Mutex<crate::types::GlobalScrubber>>,
    sender: Arc<broadcast::Sender<ScrubberEvent>>,
    stop_notify: Arc<tokio::sync::Notify>,
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

        // Process with the shared scrubber instance
        match process_with_shared_scrubber(&scrubber, &sender, &mut state).await {
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
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                        ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                            "Error during processing: {e}"
                        )),
                    ),
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

        // Wait for the next interval, but break immediately if stop is requested.
        tokio::select! {
            _ = interval.tick() => {
                state.with_mut(|s| {
                    s.scrubber_state.status = ScrubberStatus::Running;
                    s.scrubber_state.next_cycle_timestamp = None;
                });
            }
            _ = stop_notify.notified() => {
                break;
            }
        }
    }

    // Mark as stopped when exiting the loop
    let stop_event = ScrubberEvent {
        timestamp: Utc::now(),
        event_type: ::scrobble_scrubber::events::ScrubberEventType::Stopped(
            "Scrobble scrubber stopped".to_string(),
        ),
    };

    let _ = sender.send(stop_event.clone());

    state.with_mut(|s| {
        s.scrubber_state.status = ScrubberStatus::Stopped;
        s.scrubber_state.events.push(stop_event);
        s.scrubber_state.event_sender = None;
    });
}

// Helper function to run the scrubber loop with a single instance (UNUSED - kept for reference)
#[allow(dead_code)]
async fn run_scrubber_with_instance(
    mut state: Signal<AppState>,
    mut scrubber: ::scrobble_scrubber::scrubber::ScrobbleScrubber<
        ::scrobble_scrubber::persistence::FileStorage,
        ::scrobble_scrubber::scrub_action_provider::OrScrubActionProvider,
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
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                        ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                            "Error during processing: {e}"
                        )),
                    ),
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
// Helper function to process with a shared scrubber instance
async fn process_with_shared_scrubber(
    scrubber: &Arc<tokio::sync::Mutex<crate::types::GlobalScrubber>>,
    sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut scrubber_guard = scrubber.lock().await;
    process_with_scrubber(&mut scrubber_guard, sender, state).await
}

async fn process_with_scrubber(
    scrubber: &mut ::scrobble_scrubber::scrubber::ScrobbleScrubber<
        ::scrobble_scrubber::persistence::FileStorage,
        ::scrobble_scrubber::scrub_action_provider::OrScrubActionProvider,
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
            "Starting scrubber processing cycle...".to_string(),
        ),
    };
    let _ = sender.send(start_event.clone());
    state.with_mut(|s| s.scrubber_state.events.push(start_event));

    // Run the processing cycle while concurrently draining/forwarding events so the UI doesn't
    // appear "hung" during long waits (rate limiting, backoff, anchor search, etc.).
    let mut processing_future = Box::pin(scrubber.trigger_run());
    let mut processing_result: Option<lastfm_edit::Result<()>> = None;
    let mut events_closed = false;

    while processing_result.is_none() {
        tokio::select! {
            res = &mut processing_future => {
                processing_result = Some(res);
            }
            recv_res = event_receiver.recv(), if !events_closed => {
                match recv_res {
                    Ok(lib_event) => {
                        let _ = sender.send(lib_event.clone());
                        handle_scrubber_event(lib_event, *state).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Ignore: we missed some events; keep processing.
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        events_closed = true;
                    }
                }
            }
        }
    }

    // Best-effort: drain any remaining events emitted right after the processing future resolves.
    if !events_closed {
        let timeout = tokio::time::sleep(tokio::time::Duration::from_millis(500));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                recv_res = event_receiver.recv() => {
                    match recv_res {
                        Ok(lib_event) => {
                            let _ = sender.send(lib_event.clone());
                            handle_scrubber_event(lib_event, *state).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Ignore.
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = &mut timeout => break,
            }
        }
    }

    let processing_result = processing_result.expect("processing_result must be set");

    match processing_result {
        Ok(()) => {
            let success_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                    "Scrubber processing cycle completed successfully".to_string(),
                ),
            };
            let _ = sender.send(success_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(success_event));
        }
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                    ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                        "Scrubber processing failed: {e}"
                    )),
                ),
            };
            let _ = sender.send(error_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(error_event));
            return Err(e.into());
        }
    }

    Ok(())
}

pub async fn trigger_artist_processing_with_status(
    state: Signal<AppState>,
    artist_name: String,
    mut status: Signal<Option<String>>,
) {
    // Set initial processing status
    status.set(Some(format!(
        "Starting artist processing for '{artist_name}'..."
    )));

    // Use the existing logic but with status updates
    trigger_artist_processing_internal(state, artist_name, Some(status)).await;
}

async fn trigger_artist_processing_internal(
    mut state: Signal<AppState>,
    artist_name: String,
    mut status: Option<Signal<Option<String>>>,
) {
    // Update status to processing if provided
    if let Some(ref mut status_signal) = status {
        status_signal.set(Some(format!("Processing tracks for '{artist_name}'...")));
    }

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

    // Use the global scrubber instance for artist processing
    match get_or_create_scrubber(state).await {
        Ok(scrubber) => {
            // Use the same event processing pattern as process_with_scrubber
            match process_artist_with_shared_scrubber(&scrubber, &sender, &mut state, &artist_name)
                .await
            {
                Ok(()) => {
                    let success_event = ScrubberEvent {
                        timestamp: Utc::now(),
                        event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(format!(
                            "Artist processing completed successfully for '{artist_name}'"
                        )),
                    };
                    let _ = sender.send(success_event.clone());
                    state.with_mut(|s| s.scrubber_state.events.push(success_event));

                    // Update status to success
                    if let Some(ref mut status_signal) = status {
                        status_signal.set(Some(format!(
                            "Successfully processed artist '{artist_name}'"
                        )));
                    }
                }
                Err(e) => {
                    let error_event = ScrubberEvent {
                        timestamp: Utc::now(),
                        event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                            ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                                "Artist processing failed for '{artist_name}': {e}"
                            )),
                        ),
                    };
                    let _ = sender.send(error_event.clone());
                    state.with_mut(|s| s.scrubber_state.events.push(error_event));

                    // Update status to error
                    if let Some(ref mut status_signal) = status {
                        status_signal.set(Some(format!(
                            "Error processing artist '{artist_name}': {e}"
                        )));
                    }
                }
            }
        }
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(
                    ::scrobble_scrubber::events::ScrubberError::Unknown(format!(
                        "Failed to get scrubber for artist processing: {e}"
                    )),
                ),
            };
            let _ = sender.send(error_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(error_event));

            // Update status to error
            if let Some(ref mut status_signal) = status {
                status_signal.set(Some(format!("Failed to initialize scrubber: {e}")));
            }
        }
    }
}

// Process artist with detailed event handling similar to process_search_with_events
async fn process_artist_with_shared_scrubber(
    scrubber: &Arc<tokio::sync::Mutex<crate::types::GlobalScrubber>>,
    _sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
    artist_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Subscribe to events before starting processing
    let mut event_receiver = {
        let scrubber_guard = scrubber.lock().await;
        scrubber_guard.subscribe_events()
    };

    // Create a future that will run the processing when awaited
    let processing_future = async {
        let mut scrubber_guard = scrubber.lock().await;
        scrubber_guard.process_artist(artist_name).await
    };

    let mut processing_future = Box::pin(processing_future);
    let mut processing_result: Option<lastfm_edit::Result<()>> = None;
    let mut events_closed = false;

    while processing_result.is_none() {
        tokio::select! {
            res = &mut processing_future => {
                processing_result = Some(res);
            }
            recv_res = event_receiver.recv(), if !events_closed => {
                match recv_res {
                    Ok(lib_event) => {
                        handle_scrubber_event(lib_event, *state).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Ignore: we missed some events; keep processing.
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        events_closed = true;
                    }
                }
            }
        }
    }

    // Process any remaining events after processing completes
    if !events_closed {
        let timeout = tokio::time::sleep(tokio::time::Duration::from_millis(500));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                event_result = event_receiver.recv() => {
                    match event_result {
                        Ok(lib_event) => {
                            handle_scrubber_event(lib_event, *state).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Ignore.
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = &mut timeout => break,
            }
        }
    }

    let processing_result = processing_result.expect("processing_result must be set");
    processing_result.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}
