use crate::api::{search_musicbrainz_for_track, MusicBrainzResult};
use crate::error_utils::{apply_edit_with_timeout, deserialize_session};
use crate::types::AppState;
use dioxus::prelude::*;
use lastfm_edit::{ScrobbleEdit, Track};

async fn perform_musicbrainz_search(
    artist: String,
    title: String,
    album: Option<String>,
) -> Result<Vec<MusicBrainzResult>, String> {
    search_musicbrainz_for_track(artist, title, album)
        .await
        .map_err(|e| e.to_string())
}

fn populate_search_from_track(
    track: Track,
    mut search_artist: Signal<String>,
    mut search_title: Signal<String>,
    mut search_album: Signal<String>,
    mut selected_track: Signal<Option<Track>>,
) {
    search_artist.set(track.artist.clone());
    search_title.set(track.name.clone());
    search_album.set(track.album.clone().unwrap_or_default());
    selected_track.set(Some(track));
}

fn clear_search_form(
    mut search_artist: Signal<String>,
    mut search_title: Signal<String>,
    mut search_album: Signal<String>,
    mut selected_track: Signal<Option<Track>>,
    mut search_results: Signal<Vec<MusicBrainzResult>>,
) {
    search_artist.set(String::new());
    search_title.set(String::new());
    search_album.set(String::new());
    selected_track.set(None);
    search_results.set(vec![]);
}

async fn apply_track_edit_to_lastfm(
    original_track: Track,
    new_track: Track,
    session_str: String,
) -> Result<(), String> {
    let session = deserialize_session(&session_str)
        .map_err(|e| format!("Failed to deserialize session: {e}"))?;

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
        edit_all: true,
    };

    apply_edit_with_timeout(session, edit)
        .await
        .map_err(|e| format!("Failed to apply track edit: {e}"))?;

    Ok(())
}

#[component]
fn SearchResults(
    search_results: Signal<Vec<MusicBrainzResult>>,
    selected_track: Signal<Option<Track>>,
    is_applying_edit: Signal<bool>,
    on_apply_edit: EventHandler<Track>,
) -> Element {
    let results = search_results.read().clone();
    if results.is_empty() {
        return rsx! { div {} };
    }

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
                                                        on_apply_edit.call(new_track);
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
}

#[derive(Props)]
struct CachedTracksListProps {
    cached_tracks: Vec<Track>,
    on_select_track: EventHandler<Track>,
}

impl Clone for CachedTracksListProps {
    fn clone(&self) -> Self {
        Self {
            cached_tracks: self.cached_tracks.clone(),
            on_select_track: self.on_select_track,
        }
    }
}

impl PartialEq for CachedTracksListProps {
    fn eq(&self, other: &Self) -> bool {
        self.cached_tracks.len() == other.cached_tracks.len()
            && self
                .cached_tracks
                .iter()
                .zip(other.cached_tracks.iter())
                .all(|(a, b)| a.name == b.name && a.artist == b.artist && a.album == b.album)
    }
}

#[component]
fn CachedTracksList(props: CachedTracksListProps) -> Element {
    let cached_tracks = props.cached_tracks;
    let on_select_track = props.on_select_track;
    if cached_tracks.is_empty() {
        return rsx! { div {} };
    }

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
                                    on_select_track.call(track_clone.clone());
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
}

#[component]
fn SearchForm(
    search_artist: Signal<String>,
    search_title: Signal<String>,
    search_album: Signal<String>,
    is_searching: Signal<bool>,
    selected_track: Signal<Option<Track>>,
    on_search: EventHandler<()>,
    on_clear: EventHandler<()>,
) -> Element {
    rsx! {
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
                        on_search.call(());
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
                            on_clear.call(());
                        },
                        "Clear"
                    }
                }
            }
        }
    }
}

#[component]
pub fn MusicBrainzPage(mut state: Signal<AppState>) -> Element {
    let search_artist = use_signal(String::new);
    let search_title = use_signal(String::new);
    let search_album = use_signal(String::new);
    let mut search_results = use_signal(Vec::<MusicBrainzResult>::new);
    let mut is_searching = use_signal(|| false);
    let selected_track = use_signal(|| Option::<Track>::None);
    let mut is_applying_edit = use_signal(|| false);

    // Check for URL parameters to auto-populate the form
    use_effect(move || {
        // Check if we have URL parameters (in a real implementation, you'd parse them from location)
        // For now, this is just a placeholder for the URL parameter functionality
    });

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

        match perform_musicbrainz_search(artist, title, album).await {
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

    let populate_from_track = move |track: Track| {
        populate_search_from_track(
            track.clone(),
            search_artist,
            search_title,
            search_album,
            selected_track,
        );
        spawn(perform_search());
    };

    let apply_track_edit = move |new_track: Track| {
        spawn(async move {
            is_applying_edit.set(true);
            let original_track = selected_track.read().as_ref().cloned();
            let session_str = state.read().session.clone();

            if let Some(original_track) = original_track {
                if let Some(session_str) = session_str {
                    match apply_track_edit_to_lastfm(original_track, new_track, session_str).await {
                        Ok(_) => {
                            log::info!("Successfully applied track edit to Last.fm");
                            clear_search_form(
                                search_artist,
                                search_title,
                                search_album,
                                selected_track,
                                search_results,
                            );
                        }
                        Err(e) => {
                            log::error!("{e}");
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

            SearchForm {
                search_artist,
                search_title,
                search_album,
                is_searching,
                selected_track,
                on_search: move |_| {
                    spawn(perform_search());
                },
                on_clear: move |_| {
                    clear_search_form(search_artist, search_title, search_album, selected_track, search_results);
                },
            }

            SearchResults {
                search_results,
                selected_track,
                is_applying_edit,
                on_apply_edit: move |new_track| {
                    apply_track_edit(new_track);
                },
            }

            {
                let state_read = state.read();
                let cached_tracks = state_read.track_cache.recent_tracks.clone();
                rsx! {
                    CachedTracksList {
                        cached_tracks,
                        on_select_track: move |track| {
                            populate_from_track(track);
                        },
                    }
                }
            }
        }
    }
}
