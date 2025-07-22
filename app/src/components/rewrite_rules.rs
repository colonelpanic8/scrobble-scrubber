use crate::components::RulePreview;
use crate::types::{AppState, PreviewType};
use crate::utils::{clear_all_rules, get_current_tracks, remove_rule_at_index};
use dioxus::prelude::*;
use scrobble_scrubber::rewrite::RewriteRule;

#[component]
pub fn RewriteRulesPage(mut state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let saved_rules = state_read.saved_rules.clone();
    let tracks = get_current_tracks(&state_read);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            // Header and stats
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h2 { style: "font-size: 1.5rem; font-weight: bold; margin: 0;", "Rewrite Rules Management" }
                    div { style: "display: flex; align-items: center; gap: 1rem;",
                        div { style: "text-sm: color: #6b7280;",
                            "{saved_rules.len()} rules saved"
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
                            "Clear All Rules"
                        }
                    }
                }

                p { style: "color: #6b7280; margin: 0;",
                    "Manage your saved rewrite rules and see how they would apply to your recent tracks."
                }
            }

            // Rules list
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Saved Rewrite Rules" }

                if saved_rules.is_empty() {
                    div { style: "text-center; color: #6b7280; padding: 2rem;",
                        p { "No rewrite rules saved yet." }
                        p { style: "font-size: 0.875rem;", "Create and save rules in the Rule Workshop to see them here." }
                    }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: 1rem;",
                        for (idx, rule) in saved_rules.iter().enumerate() {
                            {rule_card(rule.clone(), state, idx)}
                        }
                    }
                }
            }

            // Rules preview on tracks
            if !saved_rules.is_empty() && !tracks.is_empty() {
                div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                    h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Rules Preview on Recent Tracks" }
                    RulePreview { state, rules_type: PreviewType::AllSavedRules }
                }
            }
        }
    }
}

fn rule_card(rule: RewriteRule, state: Signal<AppState>, index: usize) -> Element {
    rsx! {
        div {
            style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem;",
            div { style: "display: flex; justify-content: between; align-items: start; margin-bottom: 0.75rem;",
                div { style: "flex: 1;",
                    h4 { style: "font-weight: 600; margin-bottom: 0.5rem; color: #374151;", "Rule #{index + 1}" }

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
                    style: "background: #dc2626; color: white; padding: 0.375rem 0.75rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.875rem;",
                    onclick: move |_| {
                        spawn(async move {
                            if let Err(e) = remove_rule_at_index(state, index).await {
                                eprintln!("Failed to remove rule: {e}");
                            }
                        });
                    },
                    "Remove"
                }
            }
        }
    }
}
