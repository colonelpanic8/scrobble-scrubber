use crate::api::{search_musicbrainz_for_track, MusicBrainzResult};
use crate::error_utils::{apply_edit_with_timeout, deserialize_session};
use crate::types::AppState;
use dioxus::prelude::*;
use lastfm_edit::{ScrobbleEdit, Track};

#[component]
pub fn MusicBrainzPage(mut state: Signal<AppState>) -> Element {
    let mut search_artist = use_signal(String::new);
    let mut search_title = use_signal(String::new);
    let mut search_album = use_signal(String::new);
    let mut search_results = use_signal(Vec::<MusicBrainzResult>::new);
    let mut is_searching = use_signal(|| false);
    let mut selected_track = use_signal(|| Option::<Track>::None);
    let mut is_applying_edit = use_signal(|| false);

    // Check for URL parameters to auto-populate the form
    use_effect(move || {
        // Check if we have URL parameters (in a real implementation, you'd parse them from location)
        // For now, this is just a placeholder for the URL parameter functionality
    });

    // Function to perform MusicBrainz search
    let perform_search = move || async move {
        if search_artist.read().trim().is_empty() || search_title.read().trim().is_empty() {
            return;
        }

        is_searching.set(true);

        let artist = search_artist.read().clone();
        let title = search_title.read().clone();
        let album = if search_album.read().trim().is_empty() {
            None
        } else {
            Some(search_album.read().clone())
        };

        match search_musicbrainz_for_track(artist, title, album).await {
            Ok(results) => {
                search_results.set(results);
            }
            Err(e) => {
                log::error!("MusicBrainz search failed: {e}");
                search_results.set(vec![]);
            }
        }

        is_searching.set(false);
    };

    // Function to populate search from a cached track and immediately search
    let mut populate_from_track = move |track: Track| {
        search_artist.set(track.artist.clone());
        search_title.set(track.name.clone());
        search_album.set(track.album.clone().unwrap_or_default());
        selected_track.set(Some(track));
        // Immediately perform the search
        spawn(perform_search());
    };

    // Function to apply track edit to Last.fm
    let apply_track_edit = move |new_track: Track| {
        spawn(async move {
            is_applying_edit.set(true);
            let original_track = selected_track.read().as_ref().cloned();
            let session_str = state.read().session.clone();
            if let Some(original_track) = original_track {
                if let Some(session_str) = session_str {
                    match deserialize_session(&session_str) {
                        Ok(session) => {
                            let edit = ScrobbleEdit {
                                track_name_original: Some(original_track.name.clone()),
                                album_name_original: original_track.album.clone(),
                                artist_name_original: original_track.artist.clone(),
                                album_artist_name_original: None,
                                track_name: Some(new_track.name),
                                album_name: new_track.album,
                                artist_name: new_track.artist,
                                album_artist_name: None,
                                timestamp: original_track.timestamp,
                                edit_all: false,
                            };

                            match apply_edit_with_timeout(session, edit).await {
                                Ok(_) => {
                                    log::info!("Successfully applied track edit to Last.fm");
                                    // Clear the form after successful edit
                                    search_artist.set(String::new());
                                    search_title.set(String::new());
                                    search_album.set(String::new());
                                    selected_track.set(None);
                                    search_results.set(vec![]);
                                }
                                Err(e) => {
                                    log::error!("Failed to apply track edit: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to deserialize session: {e}");
                        }
                    }
                } else {
                    log::error!("No session available for track edit");
                }
            }
            is_applying_edit.set(false);
        });
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",

            // Header
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h2 { style: "font-size: 1.5rem; font-weight: bold; margin-bottom: 1rem; color: #1f2937;", "MusicBrainz Lookup" }
                p { style: "color: #6b7280; margin: 0;",
                    "Search MusicBrainz database for track information. Select a track from your cached history below to automatically search, or enter details manually. From the results, you can apply edits directly to Last.fm."
                }
            }

            // Search Form
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Search" }

                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; margin-bottom: 1rem;",
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.25rem;",
                            "Artist Name *"
                        }
                        input {
                            r#type: "text",
                            placeholder: "Enter artist name...",
                            value: "{search_artist.read()}",
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem;",
                            oninput: move |event| {
                                search_artist.set(event.value());
                            }
                        }
                    }
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.25rem;",
                            "Track Title *"
                        }
                        input {
                            r#type: "text",
                            placeholder: "Enter track title...",
                            value: "{search_title.read()}",
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem;",
                            oninput: move |event| {
                                search_title.set(event.value());
                            }
                        }
                    }
                }

                div { style: "margin-bottom: 1rem;",
                    label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.25rem;",
                        "Album (optional)"
                    }
                    input {
                        r#type: "text",
                        placeholder: "Enter album name...",
                        value: "{search_album.read()}",
                        style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem;",
                        oninput: move |event| {
                            search_album.set(event.value());
                        }
                    }
                }

                div { style: "display: flex; gap: 0.75rem; align-items: center;",
                    button {
                        style: format!(
                            "padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; font-size: 0.875rem; font-weight: 500; {}",
                            if search_artist.read().trim().is_empty() || search_title.read().trim().is_empty() || *is_searching.read() {
                                "background: #9ca3af; color: white; cursor: not-allowed;"
                            } else {
                                "background: #2563eb; color: white; cursor: pointer; hover:background: #1d4ed8;"
                            }
                        ),
                        disabled: search_artist.read().trim().is_empty() || search_title.read().trim().is_empty() || *is_searching.read(),
                        onclick: move |_| {
                            spawn(perform_search());
                        },
                        if *is_searching.read() {
                            "Searching..."
                        } else {
                            "Search MusicBrainz"
                        }
                    }

                    if selected_track.read().is_some() {
                        button {
                            style: "background: #6b7280; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                            onclick: move |_| {
                                search_artist.set(String::new());
                                search_title.set(String::new());
                                search_album.set(String::new());
                                selected_track.set(None);
                                search_results.set(vec![]);
                            },
                            "Clear"
                        }
                    }
                }
            }

            // Search Results
            {
                let results = search_results.read().clone();
                if !results.is_empty() {
                    rsx! {
                        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                            h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;",
                                "MusicBrainz Results ({results.len()} found)"
                            }

                            div { style: "display: flex; flex-direction: column; gap: 0.75rem;",
                                for (index, result) in results.iter().enumerate() {
                                    {
                                        let confidence_color = if result.confidence >= 0.8 {
                                    "#059669" // Green for high confidence
                                } else if result.confidence >= 0.6 {
                                    "#f59e0b" // Yellow for medium confidence
                                } else {
                                    "#dc2626" // Red for low confidence
                                };

                                rsx! {
                                    div {
                                        key: "{index}",
                                        style: "border: 1px solid #e5e7eb; border-radius: 0.375rem; padding: 1rem; hover:background: #f9fafb;",
                                        div { style: "display: flex; justify-content: space-between; align-items: start; margin-bottom: 0.5rem;",
                                            div {
                                                h4 { style: "font-size: 1rem; font-weight: 600; margin: 0; color: #1f2937;",
                                                    "{result.title}"
                                                }
                                                p { style: "font-size: 0.875rem; color: #6b7280; margin: 0.25rem 0;",
                                                    "by {result.artist}"
                                                }
                                                if let Some(album) = &result.album {
                                                    p { style: "font-size: 0.75rem; color: #9ca3af; margin: 0;",
                                                        "Album: {album}"
                                                    }
                                                }
                                            }
                                            div { style: "text-align: right; display: flex; flex-direction: column; gap: 0.5rem;",
                                                div {
                                                    style: format!("display: inline-block; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.75rem; font-weight: 500; color: white; background: {};", confidence_color),
                                                    "{(result.confidence * 100.0) as u32}% match"
                                                }
                                                if selected_track.read().is_some() {
                                                    button {
                                                        style: format!(
                                                            "padding: 0.25rem 0.75rem; border: none; border-radius: 0.25rem; font-size: 0.75rem; font-weight: 500; {}",
                                                            if *is_applying_edit.read() {
                                                                "background: #9ca3af; color: white; cursor: not-allowed;"
                                                            } else {
                                                                "background: #059669; color: white; cursor: pointer;"
                                                            }
                                                        ),
                                                        disabled: *is_applying_edit.read(),
                                                        onclick: {
                                                            let result_artist = result.artist.clone();
                                                            let result_title = result.title.clone();
                                                            let result_album = result.album.clone();
                                                            move |_| {
                                                                if !*is_applying_edit.read() {
                                                                    if let Some(selected) = selected_track.read().as_ref() {
                                                                        let new_track = Track {
                                                                            artist: result_artist.clone(),
                                                                            name: result_title.clone(),
                                                                            album: result_album.clone(),
                                                                            timestamp: selected.timestamp,
                                                                            playcount: selected.playcount,
                                                                            album_artist: None,
                                                                        };
                                                                        apply_track_edit(new_track);
                                                                    }
                                                                }
                                                            }
                                                        },
                                                        if *is_applying_edit.read() {
                                                            "Applying..."
                                                        } else {
                                                            "Apply Edit"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        div { style: "display: flex; justify-content: space-between; align-items: center; margin-top: 0.5rem; padding-top: 0.5rem; border-top: 1px solid #f3f4f6;",
                                            p { style: "font-size: 0.75rem; color: #9ca3af; margin: 0; font-family: monospace;",
                                                "MBID: {result.mbid}"
                                            }
                                            a {
                                                href: "https://musicbrainz.org/recording/{result.mbid}",
                                                target: "_blank",
                                                style: "font-size: 0.75rem; color: #2563eb; text-decoration: none; hover:underline;",
                                                "View on MusicBrainz ↗"
                                            }
                                        }
                                    }
                                }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    rsx! { div {} }
                }
            }

            // Cached Tracks for Quick Lookup
            {
                let state_read = state.read();
                let cached_tracks = &state_read.track_cache.recent_tracks;

                if !cached_tracks.is_empty() {
                    rsx! {
                        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                            h3 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Quick Lookup from Cached Tracks" }
                            p { style: "color: #6b7280; margin-bottom: 1rem; font-size: 0.875rem;",
                                "Click on any track below to automatically search MusicBrainz. From the results, you can apply edits directly to Last.fm."
                            }

                            div { style: "max-height: 400px; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                for (index, track) in cached_tracks.iter().take(50).enumerate() {
                                    {
                                        let track_clone = track.clone();
                                        rsx! {
                                            div {
                                                key: "{index}",
                                                style: "display: flex; justify-content: space-between; align-items: center; padding: 0.75rem; border-bottom: 1px solid #f3f4f6; hover:background: #f9fafb; cursor: pointer;",
                                                onclick: move |_| {
                                                    populate_from_track(track_clone.clone());
                                                },
                                                div { style: "flex-grow: 1;",
                                                    div { style: "font-weight: 500; color: #1f2937;", "{track.name}" }
                                                    div { style: "font-size: 0.875rem; color: #6b7280;", "{track.artist}" }
                                                    if let Some(album) = &track.album {
                                                        div { style: "font-size: 0.75rem; color: #9ca3af;", "{album}" }
                                                    }
                                                }
                                                div { style: "color: #2563eb; font-size: 0.875rem;", "Search →" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    rsx! { div {} }
                }
            }
        }
    }
}
