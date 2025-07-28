use crate::types::{event_formatting, AppState};
use dioxus::prelude::*;

#[component]
pub fn ActivityLogSection(mut state: Signal<AppState>) -> Element {
    let scrubber_state = state.read().scrubber_state.clone();

    rsx! {
        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
            ActivityLogHeader { state }
            ActivityLogContent {
                events: scrubber_state.events
            }
        }
    }
}

#[component]
fn ActivityLogHeader(mut state: Signal<AppState>) -> Element {
    rsx! {
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
    }
}

#[derive(Props, Clone)]
struct ActivityLogContentProps {
    events: Vec<::scrobble_scrubber::events::ScrubberEvent>,
}

impl PartialEq for ActivityLogContentProps {
    fn eq(&self, other: &Self) -> bool {
        self.events.len() == other.events.len()
    }
}

#[component]
fn ActivityLogContent(props: ActivityLogContentProps) -> Element {
    rsx! {
        div { style: "max-height: 400px; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem; padding: 0.5rem;",
            if props.events.is_empty() {
                div { style: "text-center; color: #6b7280; padding: 2rem;",
                    "No events yet. Start the scrubber to see activity."
                }
            } else {
                div { style: "display: flex; flex-direction: column-reverse; gap: 0.25rem;",
                    for (index, event) in props.events.iter().rev().enumerate() {
                        div {
                            key: "{index}",
                            style: "display: flex; align-items: center; gap: 0.75rem; padding: 0.5rem; border-radius: 0.25rem; font-size: 0.875rem; hover:background: #f9fafb;",
                            {
                                let event_category = event_formatting::get_event_category(event);
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
                                let message = event_formatting::format_event_message(event);

                                rsx! {
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
}
