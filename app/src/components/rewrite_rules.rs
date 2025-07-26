use crate::api::{load_artist_tracks, load_recent_tracks_from_page};
use crate::components::RulePreview;
use crate::types::{AppState, PreviewType, TrackSourceState};
use crate::utils::{clear_all_rules, get_current_tracks, remove_rule_at_index};
use ::scrobble_scrubber::track_cache::TrackCache;
use dioxus::prelude::*;

#[component]
pub fn RewriteRulesPage(mut state: Signal<AppState>) -> Element {
    let mut loading_tracks = use_signal(|| false);
    let mut loading_artist_tracks = use_signal(|| false);
    let mut artist_name = use_signal(String::new);

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 1.5rem;",

            // Track controls section - placed at the top
            div {
                style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h2 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Track Sources" }

                // Artist loading section
                div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; margin-bottom: 1rem;",
                    h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Load All Tracks for Artist" }
                    {
                        let cached_artists_count = state.read().track_cache.artist_tracks.len();
                        if cached_artists_count > 0 {
                            rsx! {
                                p { style: "font-size: 0.875rem; color: #6b7280; margin-bottom: 1rem;",
                                    "📂 {cached_artists_count} artists loaded from cache (see Track Source Controls below)"
                                }
                            }
                        } else {
                            rsx! { div {} }
                        }
                    }
                    div { style: "display: flex; gap: 1rem; align-items: end;",
                        div { style: "flex: 1;",
                            label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Artist Name" }
                            input {
                                style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                                placeholder: "Enter artist name",
                                value: "{artist_name}",
                                oninput: move |e| artist_name.set(e.value())
                            }
                        }
                        button {
                            style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {};",
                                "#2563eb",
                                if *loading_artist_tracks.read() { "0.5" } else { "1" }
                            ),
                            disabled: *loading_artist_tracks.read() || artist_name.read().trim().is_empty(),
                            onclick: move |_| async move {
                                let session_str = state.read().session.clone();
                                let artist = artist_name.read().trim().to_string();

                                if let Some(session_str) = session_str {
                                    if !artist.is_empty() {
                                        loading_artist_tracks.set(true);

                                        match load_artist_tracks(session_str, artist.clone()).await {
                                            Ok(_tracks) => {
                                                state.with_mut(|s| {
                                                    s.artist_tracks.insert(artist, TrackSourceState {
                                                        enabled: true,
                                                    });
                                                    // Reload cache to get the newly cached tracks
                                                    s.track_cache = TrackCache::load();
                                                });
                                            }
                                            Err(e) => {
                                                eprintln!("Failed to load artist tracks: {e}");
                                            }
                                        }

                                        loading_artist_tracks.set(false);
                                    }
                                }
                            },
                            if *loading_artist_tracks.read() {
                                "Loading..."
                            } else {
                                "Load Artist Tracks"
                            }
                        }
                    }

                    // Track source controls and indicators
                    {
                        let state_read = state.read();
                        let all_tracks = get_current_tracks(&state_read);
                        rsx! {
                            div { style: "margin-top: 1rem; border-top: 1px solid #e5e7eb; padding-top: 1rem;",
                                h4 { style: "font-weight: 600; margin-bottom: 0.75rem; color: #374151;", "Track Source Controls" }
                                div { style: "display: flex; flex-direction: column; gap: 0.5rem;",
                                    // Recent tracks control
                                    div { style: "display: flex; align-items: center; justify-content: space-between; padding: 0.5rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                        div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                            input {
                                                r#type: "checkbox",
                                                id: "enable-recent-tracks",
                                                checked: "{state_read.recent_tracks.enabled}",
                                                onchange: move |e| {
                                                    state.with_mut(|s| s.recent_tracks.enabled = e.checked());
                                                }
                                            }
                                            label {
                                                r#for: "enable-recent-tracks",
                                                style: "font-size: 0.875rem; font-weight: 500; color: #374151; cursor: pointer;",
                                                "Recent Tracks"
                                            }
                                        }
                                        div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                            span { style: "font-size: 0.75rem; color: #6b7280;",
                                                {
                                                    let total_tracks: usize = state_read.track_cache.recent_tracks.len();
                                                    format!("{total_tracks} tracks")
                                                }
                                            }
                                            // Show cache indicator if recent tracks are cached
                                            {
                                                if !state_read.track_cache.recent_tracks.is_empty() {
                                                    rsx! {
                                                        span { style: "font-size: 0.625rem; color: #059669; background: #d1fae5; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                                                            "📂 cached"
                                                        }
                                                    }
                                                } else {
                                                    rsx! { span {} }
                                                }
                                            }
                                        }
                                    }

                                    // Artist tracks controls
                                    for (artist_name, track_state) in &state_read.artist_tracks {
                                        div {
                                            key: "{artist_name}",
                                            style: "display: flex; align-items: center; justify-content: space-between; padding: 0.5rem; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                                            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                                input {
                                                    r#type: "checkbox",
                                                    id: "enable-artist-{artist_name}",
                                                    checked: "{track_state.enabled}",
                                                    onchange: {
                                                        let artist_name = artist_name.clone();
                                                        move |e: Event<FormData>| {
                                                            let enabled = e.checked();
                                                            state.with_mut(|s| {
                                                                if let Some(track_state) = s.artist_tracks.get_mut(&artist_name) {
                                                                    track_state.enabled = enabled;
                                                                }
                                                            });
                                                        }
                                                    }
                                                }
                                                label {
                                                    r#for: "enable-artist-{artist_name}",
                                                    style: "font-size: 0.875rem; font-weight: 500; color: #374151; cursor: pointer;",
                                                    "{artist_name}"
                                                }
                                            }
                                            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                                                span { style: "font-size: 0.75rem; color: #6b7280;",
                                                    {
                                                        let track_count = state_read.track_cache.artist_tracks.get(artist_name).map(|v| v.len()).unwrap_or(0);
                                                        format!("{track_count} tracks")
                                                    }
                                                }
                                                // Show cache indicator if this artist is in the cache
                                                {
                                                    let state_read = state.read();
                                                    if state_read.track_cache.artist_tracks.contains_key(artist_name) {
                                                        rsx! {
                                                            span { style: "font-size: 0.625rem; color: #059669; background: #d1fae5; padding: 0.125rem 0.25rem; border-radius: 0.25rem;",
                                                                "📂 cached"
                                                            }
                                                        }
                                                    } else {
                                                        rsx! { span {} }
                                                    }
                                                }
                                                button {
                                                    style: "background: #dc2626; color: white; padding: 0.25rem 0.5rem; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 0.75rem;",
                                                    onclick: {
                                                        let artist_name = artist_name.clone();
                                                        move |_| {
                                                            state.with_mut(|s| {
                                                                s.artist_tracks.remove(&artist_name);
                                                            });
                                                        }
                                                    },
                                                    "Remove"
                                                }
                                            }
                                        }
                                    }
                                }

                                div { style: "margin-top: 0.5rem; font-size: 0.875rem; color: #6b7280;",
                                    "Total enabled tracks: {all_tracks.len()}"
                                }
                            }
                        }
                    }
                }
            }

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

                            button {
                                style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {};",
                                    "#059669",
                                    if *loading_tracks.read() { "0.5" } else { "1" }
                                ),
                                disabled: *loading_tracks.read(),
                                onclick: move |_| async move {
                                    let (session_str, current_page) = {
                                        let s = state.read();
                                        (s.session.clone(), s.current_page)
                                    };
                                    if let Some(session_str) = session_str {
                                        loading_tracks.set(true);
                                        let next_page = current_page + 1;
                                        if let Ok(_new_tracks) = load_recent_tracks_from_page(session_str, next_page).await {
                                            state.with_mut(|s| {
                                                s.current_page = next_page;
                                                // Reload cache to get the newly cached tracks
                                                s.track_cache = TrackCache::load();
                                            });
                                        }
                                        loading_tracks.set(false);
                                    }
                                },
                                if *loading_tracks.read() {
                                    "Loading..."
                                } else {
                                    "Load More Recent Tracks"
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
