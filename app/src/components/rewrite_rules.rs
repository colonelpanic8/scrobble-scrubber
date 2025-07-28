use crate::components::{RulePreview, TrackSourcesSection};
use crate::types::{AppState, PreviewType};
use crate::utils::{clear_all_rules, get_current_tracks, remove_rule_at_index};
use dioxus::prelude::*;

#[component]
pub fn RewriteRulesPage(mut state: Signal<AppState>) -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 1.5rem;",
            TrackSourcesSection { state }

            // Responsive container for rules management and preview
            div {
                style: "display: flex; flex-wrap: wrap; gap: 1.5rem;",

                // Rules management - takes up left column on large screens
                div {
                    style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem; flex: 1; min-width: 400px;",

                    // Header and stats
                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                        h2 { style: "font-size: 1.25rem; font-weight: bold; margin: 0;", "Saved Rewrite Rules" }
                        div { style: "display: flex; align-items: center; gap: 1rem;",
                            div { style: "font-size: 0.875rem; color: #6b7280;",
                                {
                                    let saved_rules = state.read().saved_rules.clone();
                                    format!("{} rules saved", saved_rules.len())
                                }
                            }
                            button {
                                style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                onclick: move |_| {
                                    spawn(async move {
                                        if let Err(e) = clear_all_rules(state).await {
                                            eprintln!("Failed to clear rules: {e}");
                                        }
                                    });
                                },
                                "Clear All"
                            }
                        }
                    }

                    // Rules list
                    {
                        let saved_rules = state.read().saved_rules.clone();
                        if saved_rules.is_empty() {
                            rsx! {
                                div { style: "text-center; color: #6b7280; padding: 2rem;",
                                    p { "No rewrite rules saved yet." }
                                    p { style: "font-size: 0.875rem; margin-top: 0.5rem;", "Create and save rules in the Rule Workshop to see them here." }
                                }
                            }
                        } else {
                            rsx! {
                                div { style: "display: flex; flex-direction: column; gap: 1rem; max-height: 60vh; overflow-y: auto;",
                                    for (idx, rule) in saved_rules.iter().enumerate() {
                                        div {
                                            key: "{idx}",
                                            style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem;",
                                            div { style: "display: flex; justify-content: space-between; align-items: start; margin-bottom: 0.75rem;",
                                                div { style: "flex: 1;",
                                                    h4 { style: "font-weight: 600; margin-bottom: 0.5rem; color: #374151;",
                                                        {rule.name.as_deref().unwrap_or(&format!("Rule #{}", idx + 1))}
                                                    }

                                                    if let Some(track_rule) = rule.track_name.as_ref() {
                                                        div { style: "margin-bottom: 0.5rem;",
                                                            strong { "Track: " }
                                                            code { style: "background: #f3f4f6; padding: 0.25rem; border-radius: 0.25rem; font-size: 0.875rem;",
                                                                "\"{track_rule.find}\" → \"{track_rule.replace}\""
                                                            }
                                                        }
                                                    }

                                                    if let Some(artist_rule) = rule.artist_name.as_ref() {
                                                        div { style: "margin-bottom: 0.5rem;",
                                                            strong { "Artist: " }
                                                            code { style: "background: #f3f4f6; padding: 0.25rem; border-radius: 0.25rem; font-size: 0.875rem;",
                                                                "\"{artist_rule.find}\" → \"{artist_rule.replace}\""
                                                            }
                                                        }
                                                    }

                                                    if let Some(album_rule) = rule.album_name.as_ref() {
                                                        div { style: "margin-bottom: 0.5rem;",
                                                            strong { "Album: " }
                                                            code { style: "background: #f3f4f6; padding: 0.25rem; border-radius: 0.25rem; font-size: 0.875rem;",
                                                                "\"{album_rule.find}\" → \"{album_rule.replace}\""
                                                            }
                                                        }
                                                    }

                                                    if let Some(album_artist_rule) = rule.album_artist_name.as_ref() {
                                                        div { style: "margin-bottom: 0.5rem;",
                                                            strong { "Album Artist: " }
                                                            code { style: "background: #f3f4f6; padding: 0.25rem; border-radius: 0.25rem; font-size: 0.875rem;",
                                                                "\"{album_artist_rule.find}\" → \"{album_artist_rule.replace}\""
                                                            }
                                                        }
                                                    }
                                                }
                                                button {
                                                    style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; margin-top: 0.5rem;",
                                                    onclick: {
                                                        let rule_index = idx;
                                                        move |_| {
                                                            spawn(async move {
                                                                if let Err(e) = remove_rule_at_index(state, rule_index).await {
                                                                    eprintln!("Failed to remove rule: {e}");
                                                                }
                                                            });
                                                        }
                                                    },
                                                    "Remove Rule"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
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

                    // Rules preview
                    {
                        let saved_rules = state.read().saved_rules.clone();
                        let tracks = get_current_tracks(&state.read());
                        if !saved_rules.is_empty() && !tracks.is_empty() {
                            rsx! {
                                RulePreview { state, rules_type: PreviewType::AllSavedRules }
                            }
                        } else if saved_rules.is_empty() {
                            rsx! {
                                div { style: "text-center; color: #6b7280; padding: 2rem;",
                                    p { "No rules to preview." }
                                    p { style: "font-size: 0.875rem; margin-top: 0.5rem;", "Save some rules first to see their preview here." }
                                }
                            }
                        } else {
                            rsx! {
                                div { style: "text-center; color: #6b7280; padding: 2rem;",
                                    p { "No tracks loaded for preview." }
                                    p { style: "font-size: 0.875rem; margin-top: 0.5rem;", "Load some tracks using the controls above to see how your rules would apply." }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
