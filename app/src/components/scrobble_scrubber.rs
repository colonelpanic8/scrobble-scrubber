// Temporarily simplified version to fix compilation
// TODO: Properly implement using ScrobbleScrubber library

use crate::types::{event_formatting, AppState, ScrubberStatus};
use dioxus::prelude::*;

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

                        // Control buttons - simplified
                        match scrubber_state.status {
                            ScrubberStatus::Stopped | ScrubberStatus::Error(_) => rsx! {
                                button {
                                    style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        // TODO: Implement proper scrubber start
                                    },
                                    "Start Scrubber"
                                }
                            },
                            ScrubberStatus::Running => rsx! {
                                button {
                                    style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        // TODO: Implement proper scrubber stop
                                    },
                                    "Stop Scrubber"
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

            // Event log
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: between; align-items: center; margin-bottom: 1rem;",
                    h3 { style: "font-size: 1.25rem; font-weight: bold; margin: 0;", "Activity Log" }
                    div { style: "display: flex; align-items: center; gap: 1rem;",
                        div { style: "font-size: 1.5rem; font-weight: bold; color: #dc2626;", "{scrubber_state.events.len()}" }
                        button {
                            style: "background: #6b7280; color: white; padding: 0.25rem 0.5rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem;",
                            onclick: move |_| {
                                state.with_mut(|s| s.scrubber_state.events.clear());
                            },
                            "Clear"
                        }
                    }
                }

                div { style: "max-height: 24rem; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem; padding: 0.5rem;",
                    if scrubber_state.events.is_empty() {
                        div { style: "text-align: center; color: #6b7280; padding: 2rem;",
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
        }
    }
}
