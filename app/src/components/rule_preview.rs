use crate::server_functions::generate_llm_rule_for_track;
use crate::types::{AppState, PreviewType};
use crate::utils::get_current_tracks;
use dioxus::prelude::*;
use lastfm_edit::Track;
use scrobble_scrubber::rewrite::{any_rules_match, apply_all_rules, create_no_op_edit};

#[component]
pub fn RulePreview(mut state: Signal<AppState>, rules_type: PreviewType) -> Element {
    let mut generation_error = use_signal(|| Option::<String>::None);
    let mut generating_for_track = use_signal(|| Option::<usize>::None);

    let state_read = state.read();
    let tracks = get_current_tracks(&state_read);

    // Create clones for different usages
    let tracks_for_per_track_generation = tracks.clone();
    let tracks_for_display_check = tracks.clone();

    // Get the rules to apply based on the preview type
    let rules_to_apply = match rules_type {
        PreviewType::CurrentRule => vec![state_read.current_rule.clone()],
        PreviewType::AllSavedRules => state_read.saved_rules.clone(),
    };

    // Compute matches and prepare tracks for display based on toggle
    let mut tracks_to_display = Vec::new();
    let mut matching_count = 0;
    let total_tracks = tracks.len();

    for (idx, strack) in tracks.iter().enumerate() {
        let track: Track = strack.clone();

        // Use pattern matching to check if rules match (regardless of whether they would change anything)
        let rules_match = any_rules_match(&rules_to_apply, &track).unwrap_or(false);

        // Create edit to show the result (whether changed or not)
        let mut edit = create_no_op_edit(&track);
        if rules_match {
            let _rule_applied = apply_all_rules(&rules_to_apply, &mut edit).unwrap_or_default();
        }

        // Show as matching if patterns match, regardless of whether output changes
        let has_changes = rules_match;

        if has_changes {
            matching_count += 1;
        }

        // Add to display list based on toggle setting
        if state_read.show_all_tracks || has_changes {
            tracks_to_display.push((idx, strack, track, edit, has_changes));
        }
    }

    let preview_text = match rules_type {
        PreviewType::CurrentRule => "Current rule",
        PreviewType::AllSavedRules => "All saved rules",
    };

    let can_generate_rule =
        !state_read.llm_settings.api_key.trim().is_empty() && !tracks_for_display_check.is_empty();

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1rem;",
            // Count display and controls
            div { style: "display: flex; justify-content: space-between; align-items: center; padding: 0.75rem 1rem; background: #f3f4f6; border-radius: 0.5rem;",
                div { style: "font-weight: 500; color: #374151;",
                    if total_tracks == 0 {
                        "No tracks loaded"
                    } else if state_read.show_all_tracks {
                        "{preview_text} matches: {matching_count}/{total_tracks} tracks (showing all)"
                    } else {
                        "{preview_text} matches: {matching_count}/{total_tracks} tracks (showing {matching_count} matches only)"
                    }
                }

            }

            // Error display
            if let Some(error) = generation_error.read().as_ref() {
                div { style: "padding: 0.75rem; background: #fef2f2; border: 1px solid #fecaca; border-radius: 0.375rem; color: #dc2626;",
                    "{error}"
                }
            }

            if tracks_for_display_check.is_empty() {
                div { style: "color: #6b7280; text-align: center; padding: 2rem;",
                    "No tracks loaded. The preview will show here once tracks are fetched."
                }
            } else {
                div { style: "display: flex; flex-direction: column; gap: 0.75rem;",
                    for (idx, _strack, track, edit, has_changes) in tracks_to_display {
                        {
                            rsx! {
                                div {
                                    key: "{idx}",
                                    style: format!(
                                        "border: 1px solid {}; background: {}; border-radius: 0.5rem; padding: 1rem;",
                                        if has_changes { "#86efac" } else { "#e5e7eb" },
                                        if has_changes { "#f0fdf4" } else { "#f9fafb" }
                                    ),


                                    div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;",
                                        // Before
                                        div {
                                            div { style: "display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.25rem;",
                                                h4 { style: "font-size: 0.875rem; font-weight: 500; color: #374151; margin: 0;", "Original" }
                                                if matches!(rules_type, PreviewType::CurrentRule) && can_generate_rule {
                                                    button {
                                                        style: format!(
                                                            "padding: 0.25rem; border: none; border-radius: 0.25rem; cursor: pointer; transition: all 0.2s; display: flex; align-items: center; justify-content: center; {}",
                                                            if generating_for_track.read().as_ref() == Some(&idx) {
                                                                "background: #9ca3af; color: white; cursor: not-allowed;"
                                                            } else {
                                                                "background: transparent; color: #7c3aed; hover:background: #f3f4f6;"
                                                            }
                                                        ),
                                                        disabled: generating_for_track.read().is_some(),
                                                        title: "Generate rule for this track",
                                                        onclick: {
                                                            let track_clone = track.clone();
                                                            let all_tracks = tracks_for_per_track_generation.clone();
                                                            let track_idx = idx;
                                                            move |_| {
                                                                let track_for_rule = track_clone.clone();
                                                                let context_tracks = all_tracks.clone();
                                                                let llm_settings = state.read().llm_settings.clone();

                                                                spawn(async move {
                                                                    generation_error.set(None);
                                                                    generating_for_track.set(Some(track_idx));

                                                                    // Remove the target track from context
                                                                    let context_tracks: Vec<Track> = context_tracks
                                                                        .into_iter()
                                                                        .enumerate()
                                                                        .filter(|(i, _)| *i != track_idx)
                                                                        .map(|(_, track)| track)
                                                                        .collect();

                                                                    match generate_llm_rule_for_track(
                                                                        llm_settings.api_key.clone(),
                                                                        llm_settings.model.clone(),
                                                                        llm_settings.system_prompt.clone(),
                                                                        track_for_rule,
                                                                        context_tracks
                                                                    ).await {
                                                                        Ok(rule) => {
                                                                            state.with_mut(|s| {
                                                                                s.current_rule = rule;
                                                                            });
                                                                        }
                                                                        Err(e) => {
                                                                            generation_error.set(Some(format!("Failed to generate rule for this track: {e}")));
                                                                        }
                                                                    }

                                                                    generating_for_track.set(None);
                                                                });
                                                            }
                                                        },
                                                        if generating_for_track.read().as_ref() == Some(&idx) {
                                                            "⏳"
                                                        } else {
                                                            "✨"
                                                        }
                                                    }
                                                }
                                            }
                                            div { style: "display: flex; flex-direction: column; gap: 0.125rem; font-size: 0.875rem;",
                                                div { style: "font-weight: 500;", "{track.name}" }
                                                div { style: "color: #4b5563;", "{track.artist}" }
                                                if let Some(album) = &track.album {
                                                    div { style: "color: #6b7280;", "{album}" }
                                                }
                                            }
                                        }

                                        // After
                                        if has_changes {
                                            div {
                                                h4 { style: "font-size: 0.875rem; font-weight: 500; color: #059669; margin-bottom: 0.25rem;", "After Rule Applied ✓" }
                                                div { style: "display: flex; flex-direction: column; gap: 0.125rem; font-size: 0.875rem;",
                                                    div { style: "font-weight: 500;", "{edit.track_name}" }
                                                    div { style: "color: #4b5563;", "{edit.artist_name}" }
                                                    if !edit.album_name.is_empty() {
                                                        div { style: "color: #6b7280;", "{edit.album_name}" }
                                                    }
                                                    if !edit.album_artist_name.is_empty() {
                                                        div { style: "color: #6b7280;", "{edit.album_artist_name}" }
                                                    }
                                                }
                                            }
                                        } else {
                                            div {
                                                h4 { style: "font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "No Changes" }
                                                div { style: "font-size: 0.875rem; color: #9ca3af;", "Rule does not apply to this track" }
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
}

// Legacy component for backwards compatibility - now uses RulePreview
#[component]
pub fn TracksPreview(state: Signal<AppState>) -> Element {
    rsx! {
        RulePreview { state, rules_type: PreviewType::CurrentRule }
    }
}
