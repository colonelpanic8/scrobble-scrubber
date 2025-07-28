use crate::types::AppState;
use dioxus::prelude::*;

#[component]
pub fn ScrubberStatisticsSection(state: Signal<AppState>) -> Element {
    let scrubber_state = state.read().scrubber_state.clone();

    rsx! {
        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
            h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Statistics" }

            div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 1rem;",
                StatCard {
                    value: scrubber_state.processed_count,
                    label: "Tracks Processed",
                    color: "#2563eb"
                }
                StatCard {
                    value: scrubber_state.rules_applied_count,
                    label: "Rules Applied",
                    color: "#059669"
                }
                StatCard {
                    value: scrubber_state.events.len(),
                    label: "Total Events",
                    color: "#dc2626"
                }
            }
        }
    }
}

#[component]
fn StatCard(value: usize, label: &'static str, color: &'static str) -> Element {
    rsx! {
        div { style: "padding: 1rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
            div { style: "font-size: 1.5rem; font-weight: bold; color: {color};", "{value}" }
            div { style: "font-size: 0.875rem; color: #6b7280;", "{label}" }
        }
    }
}
