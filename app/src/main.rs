use dioxus::document::eval;
use dioxus::prelude::*;
use lastfm_edit::Track;
use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::persistence::{FileStorage, RewriteRulesState, StateStorage};
use scrobble_scrubber::rewrite::{apply_all_rules, create_no_op_edit, RewriteRule, SdRule};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableTrack {
    name: String,
    artist: String,
    album: Option<String>,
    timestamp: Option<u64>,
    playcount: u32,
}

impl From<Track> for SerializableTrack {
    fn from(track: Track) -> Self {
        Self {
            name: track.name,
            artist: track.artist,
            album: track.album,
            timestamp: track.timestamp,
            playcount: track.playcount,
        }
    }
}

impl From<SerializableTrack> for Track {
    fn from(strack: SerializableTrack) -> Self {
        Self {
            name: strack.name,
            artist: strack.artist,
            album: strack.album,
            timestamp: strack.timestamp,
            playcount: strack.playcount,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Page {
    RuleWorkshop,
    RewriteRules,
}

#[derive(Clone, Debug, PartialEq)]
enum TrackSource {
    Recent,
    Artist(String),
}

#[derive(Clone, Debug, PartialEq)]
enum PreviewType {
    CurrentRule,   // Only apply the rule being edited
    AllSavedRules, // Apply all saved rules collectively
}

#[derive(Clone)]
struct AppState {
    logged_in: bool,
    session: Option<String>,               // Serialized LastFmEditSession
    recent_tracks: Vec<SerializableTrack>, // Recent tracks from pagination
    artist_tracks: Vec<SerializableTrack>, // All tracks for specific artist
    current_rule: RewriteRule,
    show_all_tracks: bool, // Toggle to show all tracks or only matching ones
    current_page: u32,     // Current page for pagination (for recent tracks)
    active_page: Page,     // Current active page
    track_source: TrackSource, // What tracks are currently being viewed
    config: Option<ScrobbleScrubberConfig>, // Loaded configuration
    storage: Option<Arc<Mutex<FileStorage>>>, // Persistence storage
    saved_rules: Vec<RewriteRule>, // Rules loaded from storage
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            logged_in: false,
            session: None,
            recent_tracks: Vec::new(),
            artist_tracks: Vec::new(),
            current_rule: RewriteRule::new(),
            show_all_tracks: true, // Default to showing all tracks
            current_page: 1,       // Start at page 1
            active_page: Page::RuleWorkshop,
            track_source: TrackSource::Recent,
            config: None,
            storage: None,
            saved_rules: Vec::new(),
        }
    }
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut state = use_signal(AppState::default);

    // Initialize config and storage
    use_effect(move || {
        spawn(async move {
            // Try to load config
            match ScrobbleScrubberConfig::load() {
                Ok(config) => {
                    let state_file = config.storage.state_file.clone();

                    // Try to initialize storage
                    match FileStorage::new(&state_file) {
                        Ok(storage) => {
                            // Try to load existing rewrite rules
                            let saved_rules = match storage.load_rewrite_rules_state().await {
                                Ok(rules_state) => rules_state.rewrite_rules,
                                Err(_) => Vec::new(),
                            };

                            state.with_mut(|s| {
                                s.config = Some(config);
                                s.storage = Some(Arc::new(Mutex::new(storage)));
                                s.saved_rules = saved_rules;
                            });
                        }
                        Err(e) => {
                            eprintln!("Failed to initialize storage: {e}");
                            // Still set config without storage
                            state.with_mut(|s| s.config = Some(config));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load config: {e}");
                }
            }
        });
    });

    // Try auto-login with environment variables or config
    use_effect(move || {
        spawn(async move {
            if !state.read().logged_in {
                let (username, password) = {
                    let s = state.read();
                    if let Some(config) = &s.config {
                        (
                            config.lastfm.username.clone(),
                            config.lastfm.password.clone(),
                        )
                    } else {
                        // Fallback to environment variables
                        (
                            std::env::var("SCROBBLE_SCRUBBER_LASTFM_USERNAME").unwrap_or_default(),
                            std::env::var("SCROBBLE_SCRUBBER_LASTFM_PASSWORD").unwrap_or_default(),
                        )
                    }
                };

                if !username.trim().is_empty() && !password.trim().is_empty() {
                    match login_to_lastfm(username.trim().to_string(), password.trim().to_string())
                        .await
                    {
                        Ok(session_str) => {
                            state.with_mut(|s| {
                                s.logged_in = true;
                                s.session = Some(session_str.clone());
                            });

                            // Load recent tracks using the session
                            if let Ok(tracks) = load_recent_tracks_from_page(session_str, 1).await {
                                state.with_mut(|s| {
                                    s.recent_tracks = tracks;
                                    s.current_page = 1;
                                });
                            }
                        }
                        Err(e) => {
                            eprintln!("Auto-login failed: {e}");
                        }
                    }
                }
            }
        });
    });

    rsx! {
        div {
            style: "min-height: 100vh; background: #f5f5f5; padding: 20px; font-family: Arial, sans-serif;",
            div {
                style: "max-width: 1200px; margin: 0 auto;",
                h1 {
                    style: "font-size: 2.5rem; font-weight: bold; text-align: center; margin-bottom: 2rem; color: #333;",
                    "Scrobble Scrubber"
                }

                if !state.read().logged_in {
                    LoginPage { state }
                } else {
                    div {
                        Navigation { state }
                        MainContent { state }
                    }
                }
            }
        }
    }
}

#[component]
fn Navigation(mut state: Signal<AppState>) -> Element {
    let active_page = state.read().active_page.clone();

    rsx! {
        nav {
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1rem; margin-bottom: 1.5rem;",
            ul {
                style: "display: flex; list-style: none; margin: 0; padding: 0; gap: 1rem;",
                li {
                    button {
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; {}",
                            if active_page == Page::RuleWorkshop {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151; hover:background: #e5e7eb;"
                            }
                        ),
                        onclick: move |_| {
                            state.with_mut(|s| s.active_page = Page::RuleWorkshop);
                        },
                        "Rule Workshop"
                    }
                }
                li {
                    button {
                        style: format!(
                            "padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; font-weight: 500; transition: all 0.2s; {}",
                            if active_page == Page::RewriteRules {
                                "background: #2563eb; color: white;"
                            } else {
                                "background: #f3f4f6; color: #374151; hover:background: #e5e7eb;"
                            }
                        ),
                        onclick: move |_| {
                            state.with_mut(|s| s.active_page = Page::RewriteRules);
                        },
                        "Rewrite Rules"
                    }
                }
            }
        }
    }
}

#[component]
fn MainContent(state: Signal<AppState>) -> Element {
    let active_page = state.read().active_page.clone();

    rsx! {
        match active_page {
            Page::RuleWorkshop => rsx! { RuleWorkshop { state } },
            Page::RewriteRules => rsx! { RewriteRulesPage { state } },
        }
    }
}

#[component]
fn LoginPage(mut state: Signal<AppState>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);

    rsx! {
        div {
            style: "max-width: 400px; margin: 0 auto; background: white; border-radius: 8px; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 2rem;",
            h2 {
                style: "font-size: 1.5rem; font-weight: bold; margin-bottom: 1.5rem; text-align: center;",
                "Login to Last.fm"
            }

            if !error.read().is_empty() {
                div {
                    style: "background: #fee; border: 1px solid #fcc; color: #c33; padding: 0.75rem 1rem; border-radius: 4px; margin-bottom: 1rem;",
                    "{error}"
                }
            }

            div { style: "display: flex; flex-direction: column; gap: 1rem;",
                div {
                    label {
                        style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.5rem;",
                        "Username"
                    }
                    input {
                        style: "width: 100%; padding: 0.5rem 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; outline: none;",
                        r#type: "text",
                        placeholder: "Your Last.fm username",
                        value: "{username}",
                        oninput: move |e| username.set(e.value())
                    }
                }

                div {
                    label {
                        style: "display: block; font-size: 0.875rem; font-weight: 500; color: #374151; margin-bottom: 0.5rem;",
                        "Password"
                    }
                    input {
                        style: "width: 100%; padding: 0.5rem 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; outline: none;",
                        r#type: "password",
                        placeholder: "Your Last.fm password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value())
                    }
                }

                button {
                    style: format!("width: 100%; background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {};",
                        "#2563eb",
                        if *loading.read() { "0.5" } else { "1" }
                    ),
                    disabled: *loading.read(),
                    onclick: move |_| async move {
                        loading.set(true);
                        error.set(String::new());

                        let username_val = username.read().clone();
                        let password_val = password.read().clone();
                        match login_to_lastfm(username_val, password_val).await {
                            Ok(session_str) => {
                                state.with_mut(|s| {
                                    s.logged_in = true;
                                    s.session = Some(session_str.clone());
                                });

                                // Load recent tracks using the session
                                if let Ok(tracks) = load_recent_tracks_from_page(session_str, 1).await {
                                    state.with_mut(|s| {
                                        s.recent_tracks = tracks;
                                        s.current_page = 1;
                                    });
                                }
                            }
                            Err(e) => {
                                error.set(format!("Login failed: {e}"));
                            }
                        }
                        loading.set(false);
                    },
                    if *loading.read() {
                        "Logging in..."
                    } else {
                        "Login"
                    }
                }
            }
        }
    }
}

#[component]
fn RewriteRulesPage(mut state: Signal<AppState>) -> Element {
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

#[component]
fn RuleWorkshop(mut state: Signal<AppState>) -> Element {
    let mut loading_tracks = use_signal(|| false);
    let mut loading_artist_tracks = use_signal(|| false);
    let mut artist_name = use_signal(String::new);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            // Rule editor
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                h2 { style: "font-size: 1.25rem; font-weight: bold; margin-bottom: 1rem;", "Rule Workshop" }
                RuleEditor { state }
            }

            // Load tracks section and Live preview
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
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
                                if let Ok(mut new_tracks) = load_recent_tracks_from_page(session_str, next_page).await {
                                    state.with_mut(|s| {
                                        s.recent_tracks.append(&mut new_tracks);
                                        s.current_page = next_page;
                                        s.track_source = TrackSource::Recent;
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

                // Artist loading section
                div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; margin-bottom: 1rem;",
                    h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Load All Tracks for Artist" }
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
                                            Ok(tracks) => {
                                                state.with_mut(|s| {
                                                    s.artist_tracks = tracks;
                                                    s.track_source = TrackSource::Artist(artist);
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

                    // Current track source indicator
                    {
                        let state_read = state.read();
                        let track_source = &state_read.track_source;
                        let tracks = get_current_tracks(&state_read);
                        rsx! {
                            div { style: "margin-top: 0.5rem; font-size: 0.875rem; color: #6b7280;",
                                match track_source {
                                    TrackSource::Recent => format!("Currently showing {} recent tracks", tracks.len()),
                                    TrackSource::Artist(name) => format!("Currently showing {} tracks by {}", tracks.len(), name),
                                }
                            }
                        }
                    }
                }

                RulePreview { state, rules_type: PreviewType::CurrentRule }
            }
        }
    }
}

#[component]
fn RuleEditor(mut state: Signal<AppState>) -> Element {
    // Separate state for each field
    let mut track_find = use_signal(String::new);
    let mut track_replace = use_signal(String::new);
    let mut artist_find = use_signal(String::new);
    let mut artist_replace = use_signal(String::new);
    let mut album_find = use_signal(String::new);
    let mut album_replace = use_signal(String::new);
    let mut album_artist_find = use_signal(String::new);
    let mut album_artist_replace = use_signal(String::new);
    let mut is_regex = use_signal(|| true);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            // Regex toggle
            div { style: "display: flex; align-items: center; gap: 0.5rem;",
                input {
                    r#type: "checkbox",
                    checked: "{is_regex}",
                    onchange: move |e| {
                        is_regex.set(e.checked());
                        update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                       &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                    }
                }
                label { style: "font-weight: 500;", "Use Regular Expression" }
            }

            // Track Name
            div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem;",
                h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Track Name" }
                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;",
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Find" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: if *is_regex.read() { "Regex pattern" } else { "Text to find" },
                            value: "{track_find}",
                            oninput: move |e| {
                                track_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Replace" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: "Replacement text",
                            value: "{track_replace}",
                            oninput: move |e| {
                                track_replace.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                }
            }

            // Artist Name
            div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem;",
                h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Artist Name" }
                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;",
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Find" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: if *is_regex.read() { "Regex pattern" } else { "Text to find" },
                            value: "{artist_find}",
                            oninput: move |e| {
                                artist_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Replace" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: "Replacement text",
                            value: "{artist_replace}",
                            oninput: move |e| {
                                artist_replace.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                }
            }

            // Album Name
            div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem;",
                h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Album Name" }
                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;",
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Find" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: if *is_regex.read() { "Regex pattern" } else { "Text to find" },
                            value: "{album_find}",
                            oninput: move |e| {
                                album_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Replace" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: "Replacement text",
                            value: "{album_replace}",
                            oninput: move |e| {
                                album_replace.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                }
            }

            // Album Artist Name
            div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem;",
                h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Album Artist Name" }
                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;",
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Find" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: if *is_regex.read() { "Regex pattern" } else { "Text to find" },
                            value: "{album_artist_find}",
                            oninput: move |e| {
                                album_artist_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Replace" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: "Replacement text",
                            value: "{album_artist_replace}",
                            oninput: move |e| {
                                album_artist_replace.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace, *is_regex.read());
                            }
                        }
                    }
                }
            }

            // Action buttons
            div { style: "display: flex; gap: 1rem; align-self: flex-start;",
                button {
                    style: "background: #059669; color: white; padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer;",
                    onclick: move |_| {
                        let rule = state.read().current_rule.clone();
                        if let Ok(json_str) = serde_json::to_string_pretty(&rule) {
                            copy_to_clipboard(json_str);
                        }
                    },
                    "Copy as JSON"
                }

                button {
                    style: "background: #2563eb; color: white; padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer;",
                    onclick: move |_| {
                        spawn(async move {
                            let rule = state.read().current_rule.clone();
                            if let Err(e) = save_current_rule(state, rule).await {
                                eprintln!("Failed to save rule: {e}");
                            }
                        });
                    },
                    "Save Rule"
                }

                button {
                    style: "background: #dc2626; color: white; padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer;",
                    onclick: move |_| {
                        track_find.set(String::new());
                        track_replace.set(String::new());
                        artist_find.set(String::new());
                        artist_replace.set(String::new());
                        album_find.set(String::new());
                        album_replace.set(String::new());
                        album_artist_find.set(String::new());
                        album_artist_replace.set(String::new());
                        state.with_mut(|s| s.current_rule = RewriteRule::new());
                    },
                    "Clear All Rules"
                }
            }
        }
    }
}

#[component]
fn RulePreview(state: Signal<AppState>, rules_type: PreviewType) -> Element {
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
fn TracksPreview(state: Signal<AppState>) -> Element {
    rsx! {
        RulePreview { state, rules_type: PreviewType::CurrentRule }
    }
}

// Helper functions

fn get_current_tracks(state: &AppState) -> &Vec<SerializableTrack> {
    match &state.track_source {
        TrackSource::Recent => &state.recent_tracks,
        TrackSource::Artist(_) => &state.artist_tracks,
    }
}

#[allow(clippy::too_many_arguments)]
fn update_all_rules(
    mut state: Signal<AppState>,
    track_find: &Signal<String>,
    track_replace: &Signal<String>,
    artist_find: &Signal<String>,
    artist_replace: &Signal<String>,
    album_find: &Signal<String>,
    album_replace: &Signal<String>,
    album_artist_find: &Signal<String>,
    album_artist_replace: &Signal<String>,
    is_regex: bool,
) {
    let mut rule = RewriteRule::new();

    // Add track name rule if provided
    let track_find_str = track_find.read();
    let track_replace_str = track_replace.read();
    if !track_find_str.is_empty() {
        let sd_rule = if is_regex {
            SdRule::new_regex(&track_find_str, &track_replace_str)
        } else {
            SdRule::new_literal(&track_find_str, &track_replace_str)
        };
        rule = rule.with_track_name(sd_rule);
    }

    // Add artist name rule if provided
    let artist_find_str = artist_find.read();
    let artist_replace_str = artist_replace.read();
    if !artist_find_str.is_empty() {
        let sd_rule = if is_regex {
            SdRule::new_regex(&artist_find_str, &artist_replace_str)
        } else {
            SdRule::new_literal(&artist_find_str, &artist_replace_str)
        };
        rule = rule.with_artist_name(sd_rule);
    }

    // Add album name rule if provided
    let album_find_str = album_find.read();
    let album_replace_str = album_replace.read();
    if !album_find_str.is_empty() {
        let sd_rule = if is_regex {
            SdRule::new_regex(&album_find_str, &album_replace_str)
        } else {
            SdRule::new_literal(&album_find_str, &album_replace_str)
        };
        rule = rule.with_album_name(sd_rule);
    }

    // Add album artist rule if provided
    let album_artist_find_str = album_artist_find.read();
    let album_artist_replace_str = album_artist_replace.read();
    if !album_artist_find_str.is_empty() {
        let sd_rule = if is_regex {
            SdRule::new_regex(&album_artist_find_str, &album_artist_replace_str)
        } else {
            SdRule::new_literal(&album_artist_find_str, &album_artist_replace_str)
        };
        rule = rule.with_album_artist_name(sd_rule);
    }

    state.with_mut(|s| s.current_rule = rule);
}

async fn save_current_rule(
    mut state: Signal<AppState>,
    rule: RewriteRule,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if rule has any content
    if rule.track_name.is_none()
        && rule.artist_name.is_none()
        && rule.album_name.is_none()
        && rule.album_artist_name.is_none()
    {
        return Err("Cannot save empty rule".into());
    }

    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Load current rules
        let mut rules_state = storage_lock
            .load_rewrite_rules_state()
            .await
            .unwrap_or_default();

        // Add new rule
        rules_state.rewrite_rules.push(rule);

        // Save updated rules
        storage_lock.save_rewrite_rules_state(&rules_state).await?;

        // Update local state
        let saved_rules = rules_state.rewrite_rules;
        drop(storage_lock);
        state.with_mut(|s| s.saved_rules = saved_rules);
    }

    Ok(())
}

async fn remove_rule_at_index(
    mut state: Signal<AppState>,
    index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Load current rules
        let mut rules_state = storage_lock
            .load_rewrite_rules_state()
            .await
            .unwrap_or_default();

        // Remove rule at index
        if index < rules_state.rewrite_rules.len() {
            rules_state.rewrite_rules.remove(index);

            // Save updated rules
            storage_lock.save_rewrite_rules_state(&rules_state).await?;

            // Update local state
            let saved_rules = rules_state.rewrite_rules;
            drop(storage_lock);
            state.with_mut(|s| s.saved_rules = saved_rules);
        }
    }

    Ok(())
}

async fn clear_all_rules(mut state: Signal<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Clear all rules
        let empty_rules_state = RewriteRulesState::default();
        storage_lock
            .save_rewrite_rules_state(&empty_rules_state)
            .await?;

        // Update local state
        drop(storage_lock);
        state.with_mut(|s| s.saved_rules = Vec::new());
    }

    Ok(())
}

#[server(LoginToLastfm)]
async fn login_to_lastfm(username: String, password: String) -> Result<String, ServerFnError> {
    use lastfm_edit::LastFmEditClient;

    if username.is_empty() || password.is_empty() {
        return Err(ServerFnError::new("Username and password are required"));
    }

    // Create HTTP client and LastFM client
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClient::new(Box::new(http_client));

    match client.login(&username, &password).await {
        Ok(_) => {
            // Get the session and serialize it
            let session = client.get_session();
            match serde_json::to_string(&session) {
                Ok(session_str) => Ok(session_str),
                Err(e) => Err(ServerFnError::new(format!(
                    "Failed to serialize session: {}",
                    e
                ))),
            }
        }
        Err(e) => Err(ServerFnError::new(format!("Login failed: {}", e))),
    }
}

#[server(LoadRecentTracks)]
async fn load_recent_tracks(session_str: String) -> Result<Vec<SerializableTrack>, ServerFnError> {
    load_recent_tracks_from_page(session_str, 1).await
}

#[server(LoadArtistTracks)]
async fn load_artist_tracks(
    session_str: String,
    artist_name: String,
) -> Result<Vec<SerializableTrack>, ServerFnError> {
    use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = match serde_json::from_str(&session_str) {
        Ok(s) => s,
        Err(e) => {
            return Err(ServerFnError::new(format!(
                "Failed to deserialize session: {e}"
            )))
        }
    };

    // Create HTTP client and LastFM client from session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClient::from_session(Box::new(http_client), session);

    // Try to fetch all tracks for the artist
    let mut tracks = Vec::new();
    const MAX_TRACKS: usize = 1000; // Limit to prevent excessive loading

    match tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut artist_iterator = client.artist_tracks(&artist_name);
        let mut count = 0;

        while let Some(track) = artist_iterator.next().await? {
            if count >= MAX_TRACKS {
                break; // Safety limit
            }
            tracks.push(SerializableTrack::from(track));
            count += 1;
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await
    {
        Ok(Ok(_)) => {
            // Success - tracks were loaded
        }
        Ok(Err(e)) => {
            eprintln!("Error fetching artist tracks: {e}");
        }
        Err(_) => {
            eprintln!("Timeout fetching artist tracks");
        }
    }

    if tracks.is_empty() {
        return Err(ServerFnError::new(format!(
            "No tracks found for artist '{artist_name}'"
        )));
    }

    Ok(tracks)
}

#[server(LoadRecentTracksFromPage)]
async fn load_recent_tracks_from_page(
    session_str: String,
    page: u32,
) -> Result<Vec<SerializableTrack>, ServerFnError> {
    use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = match serde_json::from_str(&session_str) {
        Ok(s) => s,
        Err(e) => {
            return Err(ServerFnError::new(format!(
                "Failed to deserialize session: {}",
                e
            )))
        }
    };

    // Create HTTP client and LastFM client from session
    let http_client = http_client::native::NativeClient::new();
    let mut client = LastFmEditClient::from_session(Box::new(http_client), session);

    // Try to fetch real recent tracks from specific page
    let mut tracks = Vec::new();
    let mut recent_iterator = client.recent_tracks_from_page(page);
    let mut count = 0;
    const LIMIT: u32 = 50;

    match tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(track) = recent_iterator.next().await? {
            if count >= LIMIT {
                break;
            }
            tracks.push(SerializableTrack::from(track));
            count += 1;
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await
    {
        Ok(Ok(_)) => {
            if !tracks.is_empty() {
                return Ok(tracks);
            }
        }
        Ok(Err(e)) => {
            eprintln!("Error fetching tracks: {}", e);
        }
        Err(_) => {
            eprintln!("Timeout fetching tracks");
        }
    }

    // Fall back to mock data if real fetch fails
    let mock_tracks = vec![
        SerializableTrack {
            name: "Bohemian Rhapsody - 2011 Remaster".to_string(),
            artist: "Queen ft. Someone".to_string(),
            album: Some("A Night at the Opera (Deluxe Edition)".to_string()),
            timestamp: Some(1234567890),
            playcount: 150,
        },
        SerializableTrack {
            name: "Stairway to Heaven (Remaster)".to_string(),
            artist: "Led Zeppelin featuring Guest".to_string(),
            album: Some("Led Zeppelin IV".to_string()),
            timestamp: Some(1234567800),
            playcount: 75,
        },
        SerializableTrack {
            name: "Hotel California - Live".to_string(),
            artist: "Eagles".to_string(),
            album: Some("Hotel California (40th Anniversary)".to_string()),
            timestamp: Some(1234567700),
            playcount: 42,
        },
    ];

    Ok(mock_tracks)
}

// Helper function to copy text to clipboard
fn copy_to_clipboard(text: String) {
    spawn(async move {
        let _ = eval(&format!(
            "navigator.clipboard.writeText(`{}`)",
            text.replace('`', "\\`")
        ));
    });
}
