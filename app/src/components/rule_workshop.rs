use crate::components::{RuleEditor, RulePreview, TrackSourcesSection};
use crate::types::{AppState, PreviewType};
use dioxus::prelude::*;

#[component]
pub fn RuleWorkshop(mut state: Signal<AppState>) -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 1.5rem;",
            TrackSourcesSection { state }

            // Responsive container for rule editor and preview
            div {
                style: "display: flex; flex-wrap: wrap; gap: 1.5rem;",

                // Rule editor - takes up left column on large screens
                div {
                    style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem; flex: 1; min-width: 400px;",
                    h2 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Rule Editor" }
                    RuleEditor { state }
                }

                // Live preview - takes up right column on large screens
                div {
                    style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem; flex: 1; min-width: 400px; max-height: 80vh; overflow-y: auto;",

                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                        h2 { style: "font-size: 1.25rem; font-weight: bold;", "Live Preview" }

                        div { style: "display: flex; align-items: center; gap: 1rem;",
                            // Toggle for showing all tracks vs only matching
                            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                input {
                                    r#type: "checkbox",
                                    id: "show-all-tracks",
                                    checked: "{state.read().show_all_tracks}",
                                    onchange: move |e| {
                                        state.with_mut(|s| s.show_all_tracks = e.checked());
                                    }
                                }
                                label {
                                    r#for: "show-all-tracks",
                                    style: "font-size: 0.875rem; font-weight: 500; color: #374151; cursor: pointer;",
                                    "Show all tracks"
                                }
                            }
                        }
                    }

                    RulePreview { state, rules_type: PreviewType::CurrentRule }
                }
            }
        }
    }
}
