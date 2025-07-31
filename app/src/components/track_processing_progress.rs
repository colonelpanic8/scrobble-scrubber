use crate::types::AppState;
use dioxus::prelude::*;
use lastfm_edit::Track;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum TrackStatus {
    Pending,
    Processing,
    Completed { success: bool, summary: String },
}

#[derive(Clone, Debug)]
pub struct TrackProgressState {
    pub tracks: Vec<Track>,
    pub track_statuses: HashMap<usize, TrackStatus>,
    pub current_processing_index: Option<usize>,
    pub processing_type: String,
    pub is_visible: bool,
    pub auto_scroll: bool,
}

impl Default for TrackProgressState {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            track_statuses: HashMap::new(),
            current_processing_index: None,
            processing_type: String::new(),
            is_visible: false,
            auto_scroll: true,
        }
    }
}

#[component]
pub fn TrackProcessingProgressView(mut state: Signal<AppState>) -> Element {
    let progress_state = state.read().track_progress_state.clone();

    // Auto-scroll effect - using a more compatible approach
    let current_index = progress_state.current_processing_index;

    use_effect(move || {
        if progress_state.auto_scroll && progress_state.is_visible {
            if let Some(_index) = current_index {
                // Auto-scroll functionality would be handled by CSS scroll-behavior
                // and the key updates that trigger re-renders
            }
        }
    });

    if !progress_state.is_visible || progress_state.tracks.is_empty() {
        return rsx! {
            div { style: "display: none;" }
        };
    }

    rsx! {
        div {
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); margin-bottom: 1.5rem;",

            // Header
            div {
                style: "padding: 1rem 1.5rem; border-bottom: 1px solid #e5e7eb; display: flex; justify-content: space-between; align-items: center;",

                div {
                    h3 {
                        style: "font-size: 1.125rem; font-weight: 600; margin: 0; color: #1f2937;",
                        "Processing Progress: {progress_state.processing_type}"
                    }
                    p {
                        style: "color: #6b7280; margin: 0.25rem 0 0 0; font-size: 0.875rem;",
                        "Processing {progress_state.tracks.len()} tracks"
                    }
                }

                div { style: "display: flex; align-items: center; gap: 1rem;",
                    // Progress indicator
                    div { style: "display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem;",
                        {
                            let completed_count = progress_state.track_statuses.values()
                                .filter(|status| matches!(status, TrackStatus::Completed { .. }))
                                .count();
                            let processing_count = progress_state.track_statuses.values()
                                .filter(|status| matches!(status, TrackStatus::Processing))
                                .count();
                            let total_count = progress_state.tracks.len();

                            rsx! {
                                ProgressIndicator {
                                    completed_count,
                                    processing_count,
                                    total_count
                                }
                            }
                        }
                    }

                    // Auto-scroll toggle
                    div { style: "display: flex; align-items: center; gap: 0.5rem;",
                        input {
                            r#type: "checkbox",
                            id: "auto-scroll-toggle",
                            checked: progress_state.auto_scroll,
                            onchange: move |e| {
                                state.with_mut(|s| s.track_progress_state.auto_scroll = e.checked());
                            },
                        }
                        label {
                            r#for: "auto-scroll-toggle",
                            style: "font-size: 0.875rem; color: #6b7280; cursor: pointer;",
                            "Auto-scroll"
                        }
                    }

                    // Close button
                    button {
                        style: "background: #f3f4f6; color: #6b7280; border: none; border-radius: 0.375rem; padding: 0.5rem; cursor: pointer; font-size: 0.875rem; hover:background-color: #e5e7eb;",
                        onclick: move |_| {
                            state.with_mut(|s| {
                                s.track_progress_state.is_visible = false;
                                s.track_progress_state.tracks.clear();
                                s.track_progress_state.track_statuses.clear();
                                s.track_progress_state.current_processing_index = None;
                            });
                        },
                        "✕"
                    }
                }
            }

            // Track list
            div {
                style: "max-height: 400px; overflow-y: auto; padding: 0; scroll-behavior: smooth;",

                for (index, track) in progress_state.tracks.iter().enumerate() {
                    TrackProgressItem {
                        key: "{index}",
                        track_name: track.name.clone(),
                        track_artist: track.artist.clone(),
                        track_album: track.album.clone(),
                        index,
                        status: progress_state.track_statuses.get(&index).cloned().unwrap_or(TrackStatus::Pending),
                        is_current: progress_state.current_processing_index == Some(index),
                    }
                }
            }
        }
    }
}

#[component]
fn ProgressIndicator(
    completed_count: usize,
    processing_count: usize,
    total_count: usize,
) -> Element {
    let pending_count = total_count - completed_count - processing_count;

    let percentage = if total_count > 0 {
        (completed_count as f32 / total_count as f32 * 100.0) as u32
    } else {
        0
    };

    rsx! {
        div { style: "display: flex; align-items: center; gap: 0.75rem;",
            // Progress bar
            div { style: "display: flex; flex-direction: column; gap: 0.25rem;",
                div { style: "width: 200px; height: 8px; background-color: #e5e7eb; border-radius: 4px; overflow: hidden;",
                    div {
                        style: format!(
                            "height: 100%; background-color: #10b981; width: {}%; transition: width 0.3s ease;",
                            percentage
                        )
                    }
                }
                div { style: "font-size: 0.75rem; color: #6b7280; text-align: center;",
                    "{completed_count}/{total_count} completed ({percentage}%)"
                }
            }

            // Status counts
            if processing_count > 0 {
                div { style: "display: flex; align-items: center; gap: 0.25rem; font-size: 0.75rem; color: #2563eb;",
                    div { style: "width: 8px; height: 8px; background-color: #2563eb; border-radius: 50%;" }
                    "{processing_count} processing"
                }
            }

            if pending_count > 0 {
                div { style: "display: flex; align-items: center; gap: 0.25rem; font-size: 0.75rem; color: #6b7280;",
                    div { style: "width: 8px; height: 8px; background-color: #6b7280; border-radius: 50%;" }
                    "{pending_count} pending"
                }
            }
        }
    }
}

#[component]
fn TrackProgressItem(
    track_name: String,
    track_artist: String,
    track_album: Option<String>,
    index: usize,
    status: TrackStatus,
    is_current: bool,
) -> Element {
    let (status_color, status_icon, status_text) = match &status {
        TrackStatus::Pending => ("#6b7280", "○", "Waiting"),
        TrackStatus::Processing => ("#2563eb", "⟳", "Processing..."),
        TrackStatus::Completed {
            success: true,
            summary,
        } => ("#10b981", "✓", summary.as_str()),
        TrackStatus::Completed {
            success: false,
            summary,
        } => ("#dc2626", "✗", summary.as_str()),
    };

    let background_color = if is_current {
        "#eff6ff" // Light blue background for current track
    } else {
        match &status {
            TrackStatus::Completed { success: true, .. } => "#f0fdf4", // Light green for completed
            TrackStatus::Completed { success: false, .. } => "#fef2f2", // Light red for errors
            _ => "#ffffff",                                            // White for others
        }
    };

    let border_left = if is_current {
        "4px solid #2563eb"
    } else {
        "4px solid transparent"
    };

    rsx! {
        div {
            id: "track-{index}",
            style: format!(
                "padding: 0.75rem 1.5rem; border-bottom: 1px solid #f3f4f6; display: flex; align-items: center; gap: 1rem; background-color: {}; border-left: {}; transition: all 0.2s ease;",
                background_color, border_left
            ),

            // Status indicator
            div {
                style: format!(
                    "display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; border-radius: 50%; font-weight: bold; color: {}; font-size: 0.875rem;",
                    status_color
                ),
                "{status_icon}"
            }

            // Track index
            div {
                style: "font-size: 0.75rem; color: #9ca3af; font-weight: 500; min-width: 32px;",
                "#{index + 1}"
            }

            // Track info
            div { style: "flex: 1; min-width: 0;", // min-width: 0 allows text to truncate
                div { style: "font-weight: 500; color: #1f2937; truncate;",
                    "{track_name}"
                }
                div { style: "font-size: 0.875rem; color: #6b7280; truncate;",
                    "by {track_artist}"
                    if let Some(album) = &track_album {
                        " • {album}"
                    }
                }
            }

            // Status text
            div {
                style: format!(
                    "font-size: 0.875rem; color: {}; font-weight: 500; text-align: right; min-width: 120px;",
                    status_color
                ),
                "{status_text}"
            }
        }
    }
}

// Helper functions for managing track progress state
impl TrackProgressState {
    pub fn start_batch(&mut self, tracks: Vec<Track>, processing_type: String) {
        self.tracks = tracks;
        self.processing_type = processing_type;
        self.track_statuses.clear();
        self.current_processing_index = None;
        self.is_visible = true;

        // Initialize all tracks as pending
        for i in 0..self.tracks.len() {
            self.track_statuses.insert(i, TrackStatus::Pending);
        }
    }

    pub fn start_track_processing(&mut self, track_index: usize) {
        self.current_processing_index = Some(track_index);
        self.track_statuses
            .insert(track_index, TrackStatus::Processing);
    }

    pub fn complete_track_processing(
        &mut self,
        track_index: usize,
        success: bool,
        summary: String,
    ) {
        self.track_statuses
            .insert(track_index, TrackStatus::Completed { success, summary });

        // Move to next track if current
        if self.current_processing_index == Some(track_index) {
            // Find next pending track or set to None if all done
            let next_index = (track_index + 1..self.tracks.len()).find(|&i| {
                matches!(
                    self.track_statuses.get(&i),
                    Some(TrackStatus::Pending) | None
                )
            });
            self.current_processing_index = next_index;
        }
    }
}
