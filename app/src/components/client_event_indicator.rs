use crate::server_functions::*;
use crate::types::event_formatting::format_client_event_message;
use crate::types::*;
use dioxus::prelude::*;

#[component]
pub fn ClientEventIndicator(state: Signal<AppState>) -> Element {
    let mut error_message = use_signal(String::new);

    // Update client events periodically when logged in
    use_effect(move || {
        spawn(async move {
            let session_str = {
                let state_read = state.read();
                if !state_read.logged_in {
                    return;
                }
                match state_read.session.clone() {
                    Some(session) => session,
                    None => return,
                }
            };

            // Poll for latest client event every 2 seconds
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));

            loop {
                interval.tick().await;

                // Check if still logged in
                if !state.read().logged_in {
                    break;
                }

                match get_latest_client_event(session_str.clone()).await {
                    Ok(latest_event) => {
                        state.with_mut(|s| {
                            if let Some(event) = latest_event {
                                s.client_events.update_event(event);
                            } else if s.client_events.latest_event.is_some() {
                                // Clear old events if no current event
                                let now = chrono::Utc::now();
                                let last_updated = s.client_events.last_updated;
                                if (now - last_updated).num_seconds() > 30 {
                                    s.client_events.clear_event();
                                }
                            }
                        });
                        error_message.set(String::new());
                    }
                    Err(e) => {
                        error_message.set(format!("Failed to get client events: {e}"));
                    }
                }
            }
        });
    });

    let current_event = state.read().client_events.latest_event.clone();
    let last_updated = state.read().client_events.last_updated;
    let error = error_message.read().clone();

    rsx! {
        div {
            style: "position: fixed; top: 10px; right: 10px; background: rgba(0, 0, 0, 0.8); color: white; padding: 8px 12px; border-radius: 6px; font-size: 12px; z-index: 1000; min-width: 200px;",
            if !error.is_empty() {
                div {
                    style: "color: #ff6b6b;",
                    "⚠️ {error}"
                }
            } else {
                match current_event {
                    Some(event) => rsx! {
                        div {
                            style: "display: flex; align-items: center; gap: 8px;",
                            span {
                                { format_client_event_message(&event) }
                            }
                            span {
                                style: "font-size: 10px; opacity: 0.7;",
                                {
                                    let now = chrono::Utc::now();
                                    let seconds_ago = (now - last_updated).num_seconds();
                                    if seconds_ago < 60 {
                                        format!("{seconds_ago}s ago")
                                    } else {
                                        let minutes_ago = seconds_ago / 60;
                                        format!("{minutes_ago}m ago")
                                    }
                                }
                            }
                        }
                    },
                    None => rsx! {
                        div {
                            style: "display: flex; align-items: center; gap: 8px; opacity: 0.6;",
                            span { "🟢 Client ready" }
                        }
                    }
                }
            }
        }
    }
}
