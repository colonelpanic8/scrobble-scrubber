use crate::components::{
    ActivityLogSection, ArtistProcessingSection, ScrubberControlsSection,
    ScrubberStatisticsSection, TimestampManagementSection,
};
use crate::scrubber_manager::get_or_create_scrubber;
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
            ScrubberControlsSection { state }
            ScrubberStatisticsSection { state }

            ArtistProcessingSection { state }

            ActivityLogSection { state }

            TimestampManagementSection { state }
        }
    }
}

pub async fn start_scrubber(mut state: Signal<AppState>) {
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

            // Start the scrubber background task with the shared instance
            spawn(run_scrubber_with_shared_instance(
                state, scrubber, sender_arc,
            ));
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
}

pub async fn stop_scrubber(mut state: Signal<AppState>) {
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
                    event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                        "Manual processing failed: {e}"
                    )),
                };
                let _ = sender.send(error_event.clone());
                state.with_mut(|s| s.scrubber_state.events.push(error_event));
            }
        },
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                    "Failed to get scrubber: {e}"
                )),
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

// Helper function to run the scrubber loop with a shared instance
async fn run_scrubber_with_shared_instance(
    mut state: Signal<AppState>,
    scrubber: Arc<tokio::sync::Mutex<crate::types::GlobalScrubber>>,
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
        });

        // Wait for the next interval
        interval.tick().await;
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

    // Process events after scrubbing completes - forward ALL events without compression
    let mut has_cycle_completed = false;

    while !has_cycle_completed {
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(500),
            event_receiver.recv(),
        )
        .await
        {
            Ok(Ok(lib_event)) => {
                // Forward ALL events to both broadcast channel and state without any filtering
                let _ = sender.send(lib_event.clone());

                // Handle special state updates for certain event types
                match &lib_event.event_type {
                    ::scrobble_scrubber::events::ScrubberEventType::AnchorUpdated {
                        anchor_timestamp,
                        ..
                    } => {
                        state.with_mut(|s| {
                            s.scrubber_state.events.push(lib_event.clone());
                            s.scrubber_state.current_anchor_timestamp = Some(*anchor_timestamp);
                        });
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::CycleCompleted { .. } => {
                        has_cycle_completed = true;
                        state.with_mut(|s| s.scrubber_state.events.push(lib_event.clone()));
                    }
                    _ => {
                        // Add all other events to state
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

    match processing_result {
        Ok(()) => {
            let success_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                    "Cache-based processing completed successfully".to_string(),
                ),
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

pub async fn trigger_artist_processing(mut state: Signal<AppState>, artist_name: String) {
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
                }
            }
        }
        Err(e) => {
            let error_event = ScrubberEvent {
                timestamp: Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Error(format!(
                    "Failed to get scrubber for artist processing: {e}"
                )),
            };
            let _ = sender.send(error_event.clone());
            state.with_mut(|s| s.scrubber_state.events.push(error_event));
        }
    }
}

// Process artist with detailed event handling similar to process_with_scrubber
// Helper function to process artist with a shared scrubber instance
async fn process_artist_with_shared_scrubber(
    scrubber: &Arc<tokio::sync::Mutex<crate::types::GlobalScrubber>>,
    sender: &Arc<broadcast::Sender<ScrubberEvent>>,
    state: &mut Signal<AppState>,
    artist_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut scrubber_guard = scrubber.lock().await;
    process_artist_with_events(&mut scrubber_guard, sender, state, artist_name).await
}

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
    let mut track_events: HashMap<String, (lastfm_edit::Track, Vec<String>, bool)> = HashMap::new(); // (track, rule_descriptions, had_errors)

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
                        track_events
                            .entry(track_key)
                            .or_insert((track.clone(), Vec::new(), false));
                    }
                    ::scrobble_scrubber::events::ScrubberEventType::RuleApplied {
                        track,
                        description,
                        ..
                    } => {
                        rules_applied += 1;
                        let track_key = format!("{}:{}", track.artist, track.name);
                        track_events
                            .entry(track_key.clone())
                            .or_insert((track.clone(), Vec::new(), false))
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
