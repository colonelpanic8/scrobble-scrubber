use crate::components::scrobble_scrubber::{
    start_scrubber, stop_scrubber, trigger_manual_processing,
};
use crate::scrubber_manager::get_or_create_scrubber;
use crate::types::{AppState, GlobalScrubber, ScrubberStatus};
use chrono::Utc;
use dioxus::prelude::*;

#[component]
pub fn ScrubberControlsSection(mut state: Signal<AppState>) -> Element {
    let scrubber_state = state.read().scrubber_state.clone();
    let mut timer_tick = use_signal(chrono::Utc::now);

    use_future(move || async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            timer_tick.set(chrono::Utc::now());
        }
    });

    rsx! {
        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
            div { style: "display: flex; justify-content: flex-end; align-items: center; margin-bottom: 1rem;",
                div { style: "display: flex; align-items: center; gap: 1rem;",
                    ScrubberStatusIndicator {
                        status: scrubber_state.status.clone(),
                        timer_tick
                    }
                    ScrubberControlButtons {
                        status: scrubber_state.status.clone(),
                        state
                    }
                }
            }

            p { style: "color: #6b7280; margin: 0;",
                "Monitor and control the scrobble scrubber. The scrubber processes your scrobbles and applies rewrite rules automatically."
            }

            SearchControlsSection { state }
        }
    }
}

#[component]
fn ScrubberStatusIndicator(
    status: ScrubberStatus,
    timer_tick: Signal<chrono::DateTime<Utc>>,
) -> Element {
    let (style, text) = match &status {
        ScrubberStatus::Stopped => (
            "background: #fee2e2; color: #991b1b;",
            "Stopped".to_string(),
        ),
        ScrubberStatus::Starting => (
            "background: #fef3c7; color: #92400e;",
            "Starting...".to_string(),
        ),
        ScrubberStatus::Running => (
            "background: #dcfce7; color: #166534;",
            "Running".to_string(),
        ),
        ScrubberStatus::Sleeping { until_timestamp } => {
            let _tick = timer_tick.read();
            let now = chrono::Utc::now();
            let remaining_seconds = (*until_timestamp - now).num_seconds().max(0);

            let text = if remaining_seconds > 0 {
                format!("💤 Sleeping ({remaining_seconds}s)")
            } else {
                "💤 Sleeping".to_string()
            };
            ("background: #e0e7ff; color: #3730a3;", text)
        }
        ScrubberStatus::Stopping => (
            "background: #fef3c7; color: #92400e;",
            "Stopping...".to_string(),
        ),
        ScrubberStatus::Error(err) => (
            "background: #fecaca; color: #dc2626;",
            format!("Error: {err}"),
        ),
    };

    rsx! {
        div {
            style: format!(
                "padding: 0.5rem 1rem; border-radius: 0.375rem; font-size: 0.875rem; font-weight: 500; {}",
                style
            ),
            "{text}"
        }
    }
}

#[component]
fn ScrubberControlButtons(status: ScrubberStatus, mut state: Signal<AppState>) -> Element {
    match status {
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

#[component]
fn SearchControlsSection(mut state: Signal<AppState>) -> Element {
    let mut track_search_query = use_signal(String::new);
    let mut track_search_limit = use_signal(|| 50u32);
    let mut track_unlimited = use_signal(|| false);
    let mut album_search_query = use_signal(String::new);
    let mut album_search_limit = use_signal(|| 10u32);
    let mut album_unlimited = use_signal(|| false);
    let search_status = use_signal(|| None::<String>);

    rsx! {
        div {
            style: "margin-top: 1.5rem; padding-top: 1.5rem; border-top: 1px solid #e5e7eb;",
            h3 {
                style: "font-size: 1.125rem; font-weight: 600; margin-bottom: 1rem; color: #1f2937;",
                "Search-Based Processing"
            }
            p {
                style: "color: #6b7280; margin-bottom: 1rem; font-size: 0.875rem;",
                "Process tracks or albums matching specific search queries instead of your recent scrobbles."
            }

            if let Some(status) = search_status.read().as_ref() {
                div {
                    style: format!(
                        "padding: 0.75rem; border-radius: 0.375rem; margin-bottom: 1rem; {}",
                        if status.contains("Error") || status.contains("Failed") {
                            "background-color: #fee2e2; color: #991b1b; border: 1px solid #ef4444;"
                        } else if status.contains("Processing") {
                            "background-color: #fef3c7; color: #92400e; border: 1px solid #f59e0b;"
                        } else {
                            "background-color: #d1fae5; color: #065f46; border: 1px solid #10b981;"
                        }
                    ),
                    {status.clone()}
                }
            }

            div {
                style: "display: grid; gap: 1rem; grid-template-columns: 1fr 1fr;",

                // Track Search Section
                div {
                    style: "background: #f9fafb; padding: 1rem; border-radius: 0.5rem;",
                    h4 {
                        style: "font-weight: 600; margin-bottom: 0.75rem; color: #374151;",
                        "Search Tracks"
                    }
                    div {
                        style: "margin-bottom: 0.75rem;",
                        label {
                            style: "display: block; font-weight: 500; margin-bottom: 0.25rem; color: #374151; font-size: 0.875rem;",
                            "Search Query"
                        }
                        input {
                            r#type: "text",
                            placeholder: "e.g., \"artist:Beatles\" or \"Bohemian Rhapsody\"",
                            value: "{track_search_query}",
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem;",
                            oninput: move |e| track_search_query.set(e.value()),
                        }
                    }
                    div {
                        style: "margin-bottom: 0.75rem;",
                        label {
                            style: "display: block; font-weight: 500; margin-bottom: 0.25rem; color: #374151; font-size: 0.875rem;",
                            "Max Tracks"
                        }
                        div {
                            style: "display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem;",
                            input {
                                r#type: "checkbox",
                                checked: *track_unlimited.read(),
                                onchange: move |e| {
                                    track_unlimited.set(e.checked());
                                },
                            }
                            label {
                                style: "font-size: 0.875rem; color: #6b7280;",
                                "No limit (process all results)"
                            }
                        }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "500",
                            value: "{track_search_limit}",
                            disabled: *track_unlimited.read(),
                            style: format!(
                                "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem; {}",
                                if *track_unlimited.read() { "background-color: #f3f4f6; color: #9ca3af;" } else { "" }
                            ),
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u32>() {
                                    track_search_limit.set(val.clamp(1, 500));
                                }
                            },
                        }
                    }
                    button {
                        style: "width: 100%; background: #2563eb; color: white; padding: 0.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; font-weight: 500;",
                        disabled: track_search_query.read().trim().is_empty(),
                        onclick: move |_| {
                            let query = track_search_query.read().clone();
                            let limit = if *track_unlimited.read() { None } else { Some(*track_search_limit.read()) };
                            spawn(async move {
                                trigger_track_search(state, query, limit, search_status).await;
                            });
                        },
                        "Search & Process Tracks"
                    }
                }

                // Album Search Section
                div {
                    style: "background: #f9fafb; padding: 1rem; border-radius: 0.5rem;",
                    h4 {
                        style: "font-weight: 600; margin-bottom: 0.75rem; color: #374151;",
                        "Search Albums"
                    }
                    div {
                        style: "margin-bottom: 0.75rem;",
                        label {
                            style: "display: block; font-weight: 500; margin-bottom: 0.25rem; color: #374151; font-size: 0.875rem;",
                            "Album Search Query"
                        }
                        input {
                            r#type: "text",
                            placeholder: "e.g., \"Abbey Road\" or \"artist:Beatles album:Abbey\"",
                            value: "{album_search_query}",
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem;",
                            oninput: move |e| album_search_query.set(e.value()),
                        }
                    }
                    div {
                        style: "margin-bottom: 0.75rem;",
                        label {
                            style: "display: block; font-weight: 500; margin-bottom: 0.25rem; color: #374151; font-size: 0.875rem;",
                            "Max Albums"
                        }
                        div {
                            style: "display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem;",
                            input {
                                r#type: "checkbox",
                                checked: *album_unlimited.read(),
                                onchange: move |e| {
                                    album_unlimited.set(e.checked());
                                },
                            }
                            label {
                                style: "font-size: 0.875rem; color: #6b7280;",
                                "No limit (process all results)"
                            }
                        }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "50",
                            value: "{album_search_limit}",
                            disabled: *album_unlimited.read(),
                            style: format!(
                                "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem; {}",
                                if *album_unlimited.read() { "background-color: #f3f4f6; color: #9ca3af;" } else { "" }
                            ),
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u32>() {
                                    album_search_limit.set(val.clamp(1, 50));
                                }
                            },
                        }
                    }
                    button {
                        style: "width: 100%; background: #7c3aed; color: white; padding: 0.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; font-weight: 500;",
                        disabled: album_search_query.read().trim().is_empty(),
                        onclick: move |_| {
                            let query = album_search_query.read().clone();
                            let limit = if *album_unlimited.read() { None } else { Some(*album_search_limit.read()) };
                            spawn(async move {
                                trigger_album_search(state, query, limit, search_status).await;
                            });
                        },
                        "Search & Process Albums"
                    }
                }
            }
        }
    }
}

async fn trigger_track_search(
    state: Signal<AppState>,
    query: String,
    limit: Option<u32>,
    mut search_status: Signal<Option<String>>,
) {
    if query.is_empty() {
        search_status.set(Some("Please enter a search query".to_string()));
        return;
    }

    let limit_info = match limit {
        Some(l) => format!("limit: {l} tracks"),
        None => "no limit".to_string(),
    };
    search_status.set(Some(format!(
        "Searching for tracks matching '{query}' ({limit_info})..."
    )));

    // Get or create scrubber instance
    let scrubber_result = get_or_create_scrubber(state).await;

    match scrubber_result {
        Ok(scrubber) => match process_search_with_events(scrubber, &query, limit, state).await {
            Ok(()) => {
                search_status.set(Some(format!(
                    "Successfully processed tracks matching '{query}'"
                )));
            }
            Err(e) => {
                search_status.set(Some(format!("Failed to process tracks: {e}")));
                log::error!("Track search processing failed: {e}");
            }
        },
        Err(e) => {
            search_status.set(Some(format!("Failed to create scrubber: {e}")));
            log::error!("Failed to create scrubber instance: {e}");
        }
    }
}

async fn trigger_album_search(
    state: Signal<AppState>,
    query: String,
    limit: Option<u32>,
    mut search_status: Signal<Option<String>>,
) {
    if query.is_empty() {
        search_status.set(Some("Please enter a search query".to_string()));
        return;
    }

    let limit_info = match limit {
        Some(l) => format!("limit: {l} albums"),
        None => "no limit".to_string(),
    };
    search_status.set(Some(format!(
        "Searching for albums matching '{query}' ({limit_info})..."
    )));

    // Get or create scrubber instance
    let scrubber_result = get_or_create_scrubber(state).await;

    match scrubber_result {
        Ok(scrubber) => {
            match process_album_search_with_events(scrubber, &query, limit, state).await {
                Ok(()) => {
                    search_status.set(Some(format!(
                        "Successfully processed albums matching '{query}'"
                    )));
                }
                Err(e) => {
                    search_status.set(Some(format!("Failed to process albums: {e}")));
                    log::error!("Album search processing failed: {e}");
                }
            }
        }
        Err(e) => {
            search_status.set(Some(format!("Failed to create scrubber: {e}")));
            log::error!("Failed to create scrubber instance: {e}");
        }
    }
}

/// Process search with events to properly surface tracks to UI
async fn process_search_with_events(
    scrubber: std::sync::Arc<tokio::sync::Mutex<GlobalScrubber>>,
    query: &str,
    limit: Option<u32>,
    state: Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Subscribe to events before starting processing
    let mut event_receiver = {
        let scrubber_guard = scrubber.lock().await;
        scrubber_guard.subscribe_events()
    };

    // Create a future that will run the processing when awaited
    let processing_future = async {
        let mut scrubber_guard = scrubber.lock().await;
        scrubber_guard.process_search_with_limit(query, limit).await
    };

    // Create a future that processes events concurrently
    let event_processing_future = async {
        while let Ok(lib_event) = event_receiver.recv().await {
            handle_scrubber_event(lib_event, state).await;
        }
    };

    // Run both futures concurrently - the processing will yield and allow events to be processed
    let search_result = tokio::select! {
        result = processing_future => {
            result.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }
        _ = event_processing_future => {
            log::warn!("Event processing completed before search - this shouldn't happen");
            Ok(())
        }
    };

    // Process any remaining events after search completes
    let timeout = tokio::time::sleep(tokio::time::Duration::from_millis(500));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            event_result = event_receiver.recv() => {
                match event_result {
                    Ok(lib_event) => {
                        handle_scrubber_event(lib_event, state).await;
                    }
                    Err(_) => {
                        // Channel closed
                        break;
                    }
                }
            }
            _ = &mut timeout => {
                // Timeout to catch any final events
                break;
            }
        }
    }

    search_result
}

/// Process album search with events to properly surface tracks to UI
async fn process_album_search_with_events(
    scrubber: std::sync::Arc<tokio::sync::Mutex<GlobalScrubber>>,
    query: &str,
    limit: Option<u32>,
    state: Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Subscribe to events before starting processing
    let mut event_receiver = {
        let scrubber_guard = scrubber.lock().await;
        scrubber_guard.subscribe_events()
    };

    // Create a future that will run the processing when awaited
    let processing_future = async {
        let mut scrubber_guard = scrubber.lock().await;
        scrubber_guard.process_search_albums(query, limit).await
    };

    // Create a future that processes events concurrently
    let event_processing_future = async {
        while let Ok(lib_event) = event_receiver.recv().await {
            handle_scrubber_event(lib_event, state).await;
        }
    };

    // Run both futures concurrently - the processing will yield and allow events to be processed
    let search_result = tokio::select! {
        result = processing_future => {
            result.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }
        _ = event_processing_future => {
            log::warn!("Event processing completed before album search - this shouldn't happen");
            Ok(())
        }
    };

    // Process any remaining events after search completes
    let timeout = tokio::time::sleep(tokio::time::Duration::from_millis(500));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            event_result = event_receiver.recv() => {
                match event_result {
                    Ok(lib_event) => {
                        handle_scrubber_event(lib_event, state).await;
                    }
                    Err(_) => {
                        // Channel closed
                        break;
                    }
                }
            }
            _ = &mut timeout => {
                // Timeout to catch any final events
                break;
            }
        }
    }

    search_result
}

/// Handle scrubber events from search processing
pub async fn handle_scrubber_event(
    lib_event: ::scrobble_scrubber::events::ScrubberEvent,
    mut state: Signal<AppState>,
) {
    match &lib_event.event_type {
        ::scrobble_scrubber::events::ScrubberEventType::ProcessingBatchStarted {
            tracks,
            processing_type,
        } => {
            state.with_mut(|s| {
                s.scrubber_state.events.push(lib_event.clone());
                s.track_progress_state
                    .start_batch(tracks.clone(), processing_type.display_name().to_string());
            });
        }
        ::scrobble_scrubber::events::ScrubberEventType::TrackProcessingStarted {
            track_index,
            ..
        } => {
            state.with_mut(|s| {
                s.scrubber_state.events.push(lib_event.clone());
                s.track_progress_state.start_track_processing(*track_index);
            });
        }
        ::scrobble_scrubber::events::ScrubberEventType::TrackProcessingCompleted {
            track_index,
            success,
            result,
            ..
        } => {
            state.with_mut(|s| {
                s.scrubber_state.events.push(lib_event.clone());
                s.track_progress_state.complete_track_processing(
                    *track_index,
                    *success,
                    result.summary(),
                );
            });
        }
        _ => {
            // Store other events in the activity log
            state.with_mut(|s| {
                s.scrubber_state.events.push(lib_event.clone());
            });
        }
    }
}
