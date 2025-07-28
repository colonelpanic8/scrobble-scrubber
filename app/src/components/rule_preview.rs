use crate::api::suggest_rule_for_track;
use crate::types::{AppState, PreviewType};
use crate::utils::get_current_tracks;
use dioxus::prelude::*;
use lastfm_edit::{ScrobbleEdit, Track};
use scrobble_scrubber::rewrite::{any_rules_match, apply_all_rules, create_no_op_edit};
use std::collections::HashMap;

#[component]
pub fn RulePreview(state: Signal<AppState>, rules_type: PreviewType) -> Element {
    let state_read = state.read();
    let tracks = get_current_tracks(&state_read);

    // State for tracking loading status of suggest buttons (track index -> loading state)
    let loading_states = use_signal(HashMap::<usize, bool>::new);

    // State for feedback messages (track index -> message)
    let feedback_messages = use_signal(HashMap::<usize, String>::new);

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

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1rem;",
            PreviewHeader {
                preview_text,
                matching_count,
                total_tracks,
                show_all_tracks: state_read.show_all_tracks
            }

            if tracks.is_empty() {
                EmptyTracksMessage {}
            } else {
                div { style: "display: flex; flex-direction: column; gap: 0.75rem;",
                    for (idx, _strack, track, edit, has_changes) in tracks_to_display {
                        TrackPreviewCard {
                            key: "{idx}",
                            track_index: idx,
                            track: track.clone(),
                            edit,
                            has_changes,
                            loading_states,
                            feedback_messages,
                            session_str: state_read.session.clone().unwrap_or_default()
                        }
                    }
                }
            }
        }
    }
}

/// Header showing rule match statistics
#[component]
fn PreviewHeader(
    preview_text: &'static str,
    matching_count: usize,
    total_tracks: usize,
    show_all_tracks: bool,
) -> Element {
    rsx! {
        div { style: "padding: 0.75rem 1rem; background: #f3f4f6; border-radius: 0.5rem; font-weight: 500; color: #374151;",
            if total_tracks == 0 {
                "No tracks loaded"
            } else if show_all_tracks {
                "{preview_text} matches: {matching_count}/{total_tracks} tracks (showing all)"
            } else {
                "{preview_text} matches: {matching_count}/{total_tracks} tracks (showing {matching_count} matches only)"
            }
        }
    }
}

/// Message shown when no tracks are loaded
#[component]
fn EmptyTracksMessage() -> Element {
    rsx! {
        div { style: "color: #6b7280; text-align: center; padding: 2rem;",
            "No tracks loaded. The preview will show here once tracks are fetched."
        }
    }
}

/// Individual track preview card with before/after comparison and suggest button
#[derive(Props, Clone)]
struct TrackPreviewCardProps {
    track_index: usize,
    track: Track,
    edit: ScrobbleEdit,
    has_changes: bool,
    loading_states: Signal<HashMap<usize, bool>>,
    feedback_messages: Signal<HashMap<usize, String>>,
    session_str: String,
}

impl PartialEq for TrackPreviewCardProps {
    fn eq(&self, other: &Self) -> bool {
        self.track_index == other.track_index
            && self.has_changes == other.has_changes
            && self.session_str == other.session_str
        // Skip comparing Track and ScrobbleEdit as they don't implement PartialEq
    }
}

#[component]
fn TrackPreviewCard(props: TrackPreviewCardProps) -> Element {
    let TrackPreviewCardProps {
        track_index,
        track,
        edit,
        has_changes,
        loading_states,
        feedback_messages,
        session_str,
    } = props;

    rsx! {
        div {
            style: format!(
                "border: 1px solid {}; background: {}; border-radius: 0.5rem; padding: 1rem;",
                if has_changes { "#86efac" } else { "#e5e7eb" },
                if has_changes { "#f0fdf4" } else { "#f9fafb" }
            ),

            div { style: "display: flex; flex-direction: column; gap: 1rem;",
                TrackComparisonGrid { track: track.clone(), edit, has_changes }
                SuggestRuleSection {
                    track_index,
                    track: track.clone(),
                    loading_states,
                    feedback_messages,
                    session_str
                }
            }
        }
    }
}

/// Grid showing original vs modified track metadata
#[derive(Props, Clone)]
struct TrackComparisonGridProps {
    track: Track,
    edit: ScrobbleEdit,
    has_changes: bool,
}

impl PartialEq for TrackComparisonGridProps {
    fn eq(&self, other: &Self) -> bool {
        self.has_changes == other.has_changes
        // Skip comparing Track and ScrobbleEdit as they don't implement PartialEq
    }
}

#[component]
fn TrackComparisonGrid(props: TrackComparisonGridProps) -> Element {
    let TrackComparisonGridProps {
        track,
        edit,
        has_changes,
    } = props;

    rsx! {
        div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;",
            OriginalTrackInfo { track: track.clone() }
            if has_changes {
                ModifiedTrackInfo { edit }
            } else {
                NoChangesInfo {}
            }
        }
    }
}

/// Original track metadata display
#[derive(Props, Clone)]
struct OriginalTrackInfoProps {
    track: Track,
}

impl PartialEq for OriginalTrackInfoProps {
    fn eq(&self, _other: &Self) -> bool {
        true // Skip comparing Track as it doesn't implement PartialEq
    }
}

#[component]
fn OriginalTrackInfo(props: OriginalTrackInfoProps) -> Element {
    let track = props.track;

    rsx! {
        div {
            h4 { style: "font-size: 0.875rem; font-weight: 500; color: #374151; margin-top: 0; margin-bottom: 0.5rem;", "Original" }
            div { style: "display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.875rem;",
                div { style: "font-weight: 500;", "{track.name}" }
                div { style: "color: #4b5563;", "{track.artist}" }
                if let Some(album) = &track.album {
                    div { style: "color: #6b7280;", "{album}" }
                }
            }
        }
    }
}

/// Modified track metadata display
#[derive(Props, Clone)]
struct ModifiedTrackInfoProps {
    edit: ScrobbleEdit,
}

impl PartialEq for ModifiedTrackInfoProps {
    fn eq(&self, _other: &Self) -> bool {
        true // Skip comparing ScrobbleEdit as it doesn't implement PartialEq
    }
}

#[component]
fn ModifiedTrackInfo(props: ModifiedTrackInfoProps) -> Element {
    let edit = props.edit;
    rsx! {
        div {
            h4 { style: "font-size: 0.875rem; font-weight: 500; color: #059669; margin-top: 0; margin-bottom: 0.5rem;", "After Rule Applied ✓" }
            div { style: "display: flex; flex-direction: column; gap: 0.25rem; font-size: 0.875rem;",
                div { style: "font-weight: 500;", "{edit.track_name.as_deref().unwrap_or(\"unknown\")}" }
                div { style: "color: #4b5563;", "{edit.artist_name}" }
                if edit.album_name.as_ref().is_some_and(|name| !name.is_empty()) {
                    div { style: "color: #6b7280;", "{edit.album_name.as_deref().unwrap_or(\"\")}" }
                }
            }
        }
    }
}

/// Display when no changes were made
#[component]
fn NoChangesInfo() -> Element {
    rsx! {
        div {
            h4 { style: "font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-top: 0; margin-bottom: 0.5rem;", "No Changes" }
            div { style: "font-size: 0.875rem; color: #9ca3af;", "Rule does not apply to this track" }
        }
    }
}

/// Section with suggest rule button and feedback message
#[derive(Props, Clone)]
struct SuggestRuleSectionProps {
    track_index: usize,
    track: Track,
    loading_states: Signal<HashMap<usize, bool>>,
    feedback_messages: Signal<HashMap<usize, String>>,
    session_str: String,
}

impl PartialEq for SuggestRuleSectionProps {
    fn eq(&self, other: &Self) -> bool {
        self.track_index == other.track_index && self.session_str == other.session_str
        // Skip comparing Track as it doesn't implement PartialEq
    }
}

#[component]
fn SuggestRuleSection(props: SuggestRuleSectionProps) -> Element {
    let SuggestRuleSectionProps {
        track_index,
        track,
        loading_states,
        feedback_messages,
        session_str,
    } = props;
    rsx! {
        div { style: "display: flex; justify-content: space-between; align-items: center; padding-top: 0.5rem; border-top: 1px solid #e5e7eb;",
            FeedbackMessage { track_index, feedback_messages }
            SuggestRuleButton {
                track_index,
                track: track.clone(),
                loading_states,
                feedback_messages,
                session_str
            }
        }
    }
}

/// Feedback message display area
#[component]
fn FeedbackMessage(
    track_index: usize,
    feedback_messages: Signal<HashMap<usize, String>>,
) -> Element {
    rsx! {
        div { style: "flex: 1;",
            if let Some(message) = feedback_messages.read().get(&track_index) {
                div { style: "font-size: 0.75rem; color: #059669; font-weight: 500;", "{message}" }
            }
        }
    }
}

/// Suggest rule button with loading states
#[derive(Props, Clone)]
struct SuggestRuleButtonProps {
    track_index: usize,
    track: Track,
    loading_states: Signal<HashMap<usize, bool>>,
    feedback_messages: Signal<HashMap<usize, String>>,
    session_str: String,
}

impl PartialEq for SuggestRuleButtonProps {
    fn eq(&self, other: &Self) -> bool {
        self.track_index == other.track_index && self.session_str == other.session_str
        // Skip comparing Track as it doesn't implement PartialEq
    }
}

#[component]
fn SuggestRuleButton(props: SuggestRuleButtonProps) -> Element {
    let SuggestRuleButtonProps {
        track_index,
        track,
        mut loading_states,
        mut feedback_messages,
        session_str,
    } = props;
    let is_loading = *loading_states.read().get(&track_index).unwrap_or(&false);

    rsx! {
        button {
            style: format!(
                "padding: 0.5rem 1rem; font-size: 0.75rem; font-weight: 500; color: white; background: {}; border: none; border-radius: 0.375rem; cursor: {}; transition: all 0.2s;",
                if is_loading { "#9ca3af" } else { "#3b82f6" },
                if is_loading { "not-allowed" } else { "pointer" }
            ),
            disabled: is_loading,
            onclick: move |_| {
                let track = track.clone();
                let session_str = session_str.clone();
                spawn(async move {
                    // Set loading state
                    loading_states.write().insert(track_index, true);
                    feedback_messages.write().remove(&track_index);

                    // Make the API call
                    match suggest_rule_for_track(session_str, track).await {
                        Ok(message) => {
                            feedback_messages.write().insert(track_index, message);
                        }
                        Err(e) => {
                            feedback_messages.write().insert(track_index, format!("Error: {e}"));
                        }
                    }

                    // Clear loading state
                    loading_states.write().insert(track_index, false);
                });
            },
            if is_loading {
                "Suggesting..."
            } else {
                "🤖 Suggest Rule"
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
