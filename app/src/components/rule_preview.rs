use crate::types::{AppState, PreviewType};
use crate::utils::get_current_tracks;
use ::scrobble_scrubber::rewrite::{apply_all_rules, create_no_op_edit};
use dioxus::prelude::*;
use lastfm_edit::Track;

#[component]
pub fn RulePreview(state: Signal<AppState>, rules_type: PreviewType) -> Element {
    let state_read = state.read();
    let tracks = get_current_tracks(&state_read);

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
        let track: Track = strack.clone().into();
        let mut edit = create_no_op_edit(&track);
        let _rule_applied = apply_all_rules(&rules_to_apply, &mut edit).unwrap_or_default();

        let has_changes = edit.track_name != track.name
            || edit.artist_name != track.artist
            || edit.album_name != track.album.clone().unwrap_or_default();

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

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1rem;",
            // Count display
            div { style: "padding: 0.75rem 1rem; background: #f3f4f6; border-radius: 0.5rem; font-weight: 500; color: #374151;",
                if total_tracks == 0 {
                    "No tracks loaded"
                } else if state_read.show_all_tracks {
                    "{preview_text} matches: {matching_count}/{total_tracks} tracks (showing all)"
                } else {
                    "{preview_text} matches: {matching_count}/{total_tracks} tracks (showing {matching_count} matches only)"
                }
            }

            if tracks.is_empty() {
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
                                            h4 { style: "font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.5rem;", "Original" }
                                            div { style: "display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.875rem;",
                                                div { style: "font-weight: 500;", "{track.name}" }
                                                div { style: "color: #4b5563;", "by {track.artist}" }
                                                if let Some(album) = &track.album {
                                                    div { style: "color: #6b7280;", "from {album}" }
                                                }
                                            }
                                        }

                                        // After
                                        if has_changes {
                                            div {
                                                h4 { style: "font-size: 0.875rem; font-weight: 500; color: #059669; margin-bottom: 0.5rem;", "After Rule Applied ✓" }
                                                div { style: "display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.875rem;",
                                                    div { style: "font-weight: 500;", "{edit.track_name}" }
                                                    div { style: "color: #4b5563;", "by {edit.artist_name}" }
                                                    if !edit.album_name.is_empty() {
                                                        div { style: "color: #6b7280;", "from {edit.album_name}" }
                                                    }
                                                    if !edit.album_artist_name.is_empty() {
                                                        div { style: "color: #6b7280;", "album artist: {edit.album_artist_name}" }
                                                    }
                                                }
                                            }
                                        } else {
                                            div {
                                                h4 { style: "font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.5rem;", "No Changes" }
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
