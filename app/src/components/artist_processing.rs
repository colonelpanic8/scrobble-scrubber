use crate::components::scrobble_scrubber::trigger_artist_processing_with_status;
use crate::types::AppState;
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;

#[component]
pub fn ArtistProcessingSection(mut state: Signal<AppState>) -> Element {
    let artist_input = use_signal(String::new);
    let artist_processing_status = use_signal(|| None::<String>);

    rsx! {
        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
            h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Process Specific Artist" }

            p { style: "color: #6b7280; margin-bottom: 1rem; font-size: 0.875rem;",
                "Enter an artist name to process only tracks by that artist. This will apply your saved rules to all tracks by the specified artist in your cache."
            }

            // Status display similar to search handling
            if let Some(status) = artist_processing_status.read().as_ref() {
                div {
                    style: format!(
                        "padding: 0.75rem; border-radius: 0.375rem; margin-bottom: 1rem; {}",
                        if status.contains("Error") || status.contains("Failed") {
                            "background-color: #fee2e2; color: #991b1b; border: 1px solid #ef4444;"
                        } else if status.contains("Processing") || status.contains("Starting") {
                            "background-color: #fef3c7; color: #92400e; border: 1px solid #f59e0b;"
                        } else {
                            "background-color: #d1fae5; color: #065f46; border: 1px solid #10b981;"
                        }
                    ),
                    {status.clone()}
                }
            }

            div { style: "display: flex; gap: 0.75rem; align-items: flex-end;",
                ArtistInputField { artist_input, state, artist_processing_status }
                ProcessArtistButton { artist_input, state, artist_processing_status }
            }

            TopArtistsList { artist_input, state }
        }
    }
}

#[component]
fn ArtistInputField(
    mut artist_input: Signal<String>,
    mut state: Signal<AppState>,
    mut artist_processing_status: Signal<Option<String>>,
) -> Element {
    let is_processing = artist_processing_status
        .read()
        .as_ref()
        .is_some_and(|status| {
            status.contains("Processing") || status.contains("Starting")
        });

    rsx! {
        div { style: "flex: 1;",
            label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.25rem;",
                "Artist Name"
            }
            input {
                r#type: "text",
                placeholder: "Enter artist name...",
                value: "{artist_input.read()}",
                disabled: is_processing,
                style: format!(
                    "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem; focus:outline-none; focus:ring-2; focus:ring-blue-500; focus:border-transparent; {}",
                    if is_processing { "background-color: #f9fafb; color: #6b7280;" } else { "" }
                ),
                oninput: move |event| {
                    if !is_processing {
                        artist_input.set(event.value());
                    }
                },
                onkeypress: move |event| {
                    if event.key() == Key::Enter && !is_processing {
                        let artist_name = artist_input.read().trim().to_string();
                        if !artist_name.is_empty() {
                            spawn(async move {
                                trigger_artist_processing_with_status(state, artist_name, artist_processing_status).await;
                            });
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProcessArtistButton(
    artist_input: Signal<String>,
    mut state: Signal<AppState>,
    mut artist_processing_status: Signal<Option<String>>,
) -> Element {
    let is_processing = artist_processing_status
        .read()
        .as_ref()
        .is_some_and(|status| {
            status.contains("Processing") || status.contains("Starting")
        });
    let is_disabled = artist_input.read().trim().is_empty() || is_processing;

    rsx! {
        button {
            style: format!(
                "padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; font-size: 0.875rem; font-weight: 500; {}",
                if is_disabled {
                    "background: #9ca3af; color: white; cursor: not-allowed;"
                } else {
                    "background: #7c3aed; color: white; cursor: pointer; hover:background: #6d28d9;"
                }
            ),
            disabled: is_disabled,
            onclick: move |_| {
                let artist_name = artist_input.read().trim().to_string();
                if !artist_name.is_empty() && !is_processing {
                    spawn(async move {
                        trigger_artist_processing_with_status(state, artist_name, artist_processing_status).await;
                    });
                }
            },
            if is_processing { "Processing..." } else { "Process Artist" }
        }
    }
}

#[component]
fn TopArtistsList(mut artist_input: Signal<String>, state: Signal<AppState>) -> Element {
    let state_read = state.read();
    let artist_tracks = &state_read.track_cache.recent_tracks;
    let mut artist_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for track in artist_tracks {
        *artist_counts.entry(track.artist.clone()).or_insert(0) += 1;
    }

    let mut sorted_artists: Vec<_> = artist_counts.into_iter().collect();
    sorted_artists.sort_by(|a, b| b.1.cmp(&a.1));

    if !sorted_artists.is_empty() {
        rsx! {
            div { style: "margin-top: 1rem; border-top: 1px solid #e5e7eb; padding-top: 1rem;",
                p { style: "font-size: 0.75rem; color: #6b7280; margin-bottom: 0.5rem;",
                    "Top artists in cache (click to auto-fill):"
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 0.5rem;",
                    for (artist, count) in sorted_artists.iter().take(8) {
                        ArtistButton {
                            key: "{artist}",
                            artist_name: artist.clone(),
                            count: *count,
                            artist_input
                        }
                    }
                }
            }
        }
    } else {
        rsx! { div {} }
    }
}

#[component]
fn ArtistButton(artist_name: String, count: usize, mut artist_input: Signal<String>) -> Element {
    rsx! {
        button {
            style: "background: #f3f4f6; color: #374151; padding: 0.25rem 0.5rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem; hover:background: #e5e7eb;",
            onclick: move |_| {
                artist_input.set(artist_name.clone());
            },
            "{artist_name} ({count})"
        }
    }
}
