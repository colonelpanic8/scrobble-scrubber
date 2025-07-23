use crate::types::{AppState, PreviewType};
use crate::utils::get_current_tracks;
use dioxus::prelude::*;
use lastfm_edit::{ScrobbleEdit, Track};
use scrobble_scrubber::rewrite::{any_rules_match, apply_all_rules, create_no_op_edit};

// Pagination constants
const INITIAL_LOAD_SIZE: usize = 50; // Number of tracks to show initially
const LOAD_MORE_SIZE: usize = 25; // Number of tracks to load when clicking "Load More"

#[component]
pub fn RulePreview(state: Signal<AppState>, rules_type: PreviewType) -> Element {
    let mut visible_count = use_signal(|| INITIAL_LOAD_SIZE);

    // Use regular signals for track processing to avoid PartialEq issues
    let mut processed_tracks = use_signal(Vec::<(usize, Track, ScrobbleEdit, bool)>::new);
    let mut matching_count = use_signal(|| 0);
    let mut total_tracks = use_signal(|| 0);
    let mut show_all_tracks = use_signal(|| true);

    // Calculate preview text before the closure
    let preview_text = match rules_type {
        PreviewType::CurrentRule => "Current rule",
        PreviewType::AllSavedRules => "All saved rules",
    };

    // Process tracks when component loads or state changes
    use_effect(move || {
        let state_read = state.read();
        let tracks = get_current_tracks(&state_read);

        // Get the rules to apply based on the preview type
        let rules_to_apply = match rules_type {
            PreviewType::CurrentRule => vec![state_read.current_rule.clone()],
            PreviewType::AllSavedRules => state_read.saved_rules.clone(),
        };

        // Process all tracks but don't render them yet
        let mut tracks_to_display = Vec::new();
        let mut match_count = 0;
        let track_count = tracks.len();

        for (idx, strack) in tracks.iter().enumerate() {
            let track: Track = strack.clone().into();

            // Use pattern matching to check if rules match
            let rules_match = any_rules_match(&rules_to_apply, &track).unwrap_or(false);

            // Create edit to show the result
            let mut edit = create_no_op_edit(&track);
            if rules_match {
                let _rule_applied = apply_all_rules(&rules_to_apply, &mut edit).unwrap_or_default();
            }

            let has_changes = rules_match;
            if has_changes {
                match_count += 1;
            }

            // Add to display list based on toggle setting
            if state_read.show_all_tracks || has_changes {
                tracks_to_display.push((idx, track, edit, has_changes));
            }
        }

        processed_tracks.set(tracks_to_display);
        matching_count.set(match_count);
        total_tracks.set(track_count);
        show_all_tracks.set(state_read.show_all_tracks);
    });

    let tracks_to_display = processed_tracks.read();
    let matching_count_val = *matching_count.read();
    let total_tracks_val = *total_tracks.read();
    let show_all_tracks_val = *show_all_tracks.read();
    let visible_count_val = *visible_count.read();

    // Limit tracks to visible count for pagination
    let visible_tracks: Vec<_> = tracks_to_display.iter().take(visible_count_val).collect();

    let has_more_tracks = tracks_to_display.len() > visible_count_val;

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1rem;",
            // Count display
            div { style: "padding: 0.75rem 1rem; background: #f3f4f6; border-radius: 0.5rem; font-weight: 500; color: #374151;",
                if total_tracks_val == 0 {
                    "No tracks loaded"
                } else if show_all_tracks_val {
                    "{preview_text} matches: {matching_count_val}/{total_tracks_val} tracks (showing all)"
                } else {
                    "{preview_text} matches: {matching_count_val}/{total_tracks_val} tracks (showing {matching_count_val} matches only)"
                }
            }

            if tracks_to_display.is_empty() {
                div { style: "color: #6b7280; text-align: center; padding: 2rem;",
                    "No tracks loaded. The preview will show here once tracks are fetched."
                }
            } else {
                div {
                    // Track list with pagination
                    div { style: "display: flex; flex-direction: column; gap: 0.75rem; max-height: 600px; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 0.75rem;",
                        for (idx, track, edit, has_changes) in visible_tracks.iter() {
                            div {
                                key: "{idx}",
                                style: format!(
                                    "border: 1px solid {}; background: {}; border-radius: 0.5rem; padding: 1rem;",
                                    if *has_changes { "#86efac" } else { "#e5e7eb" },
                                    if *has_changes { "#f0fdf4" } else { "#f9fafb" }
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
                                    if *has_changes {
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

                    // Load More button
                    if has_more_tracks {
                        div { style: "margin-top: 1rem; text-align: center;",
                            button {
                                style: "background: #2563eb; color: white; padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500;",
                                onclick: move |_| {
                                    visible_count.with_mut(|count| *count += LOAD_MORE_SIZE);
                                },
                                {format!("Load More Tracks ({} of {} shown)",
                                    visible_count_val.min(tracks_to_display.len()),
                                    tracks_to_display.len())}
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
