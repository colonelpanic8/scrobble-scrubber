use dioxus::prelude::*;
use lastfm_edit::Track;
use scrobble_scrubber::rewrite::{apply_all_rules, create_no_op_edit, RewriteRule, SdRule};
use serde::{Deserialize, Serialize};

// Temporarily comment out assets to debug blank page
// const FAVICON: Asset = asset!("/assets/favicon.ico");
// const MAIN_CSS: Asset = asset!("/assets/main.css");
// const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

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

#[derive(Clone, Debug)]
struct AppState {
    logged_in: bool,
    session: Option<String>,        // Serialized LastFmEditSession
    tracks: Vec<SerializableTrack>, // Only mutated when loading from client
    current_rule: RewriteRule,
    show_all_tracks: bool, // Toggle to show all tracks or only matching ones
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            logged_in: false,
            session: None,
            tracks: Vec::new(),
            current_rule: RewriteRule::new(),
            show_all_tracks: true, // Default to showing all tracks
        }
    }
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut state = use_signal(AppState::default);

    // Try auto-login with environment variables
    use_effect(move || {
        spawn(async move {
            if !state.read().logged_in {
                if let (Ok(username), Ok(password)) = (
                    std::env::var("SCROBBLE_SCRUBBER_LASTFM_USERNAME"),
                    std::env::var("SCROBBLE_SCRUBBER_LASTFM_PASSWORD"),
                ) {
                    if !username.trim().is_empty() && !password.trim().is_empty() {
                        match login_to_lastfm(
                            username.trim().to_string(),
                            password.trim().to_string(),
                        )
                        .await
                        {
                            Ok(session_str) => {
                                state.with_mut(|s| {
                                    s.logged_in = true;
                                    s.session = Some(session_str.clone());
                                });

                                // Load recent tracks using the session
                                if let Ok(tracks) = load_recent_tracks(session_str).await {
                                    state.with_mut(|s| s.tracks = tracks);
                                }
                            }
                            Err(e) => {
                                eprintln!("Auto-login failed: {e}");
                            }
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
                    "Scrobble Rule Editor"
                }

                if !state.read().logged_in {
                    LoginPage { state }
                } else {
                    RuleWorkshop { state }
                }
            }
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
                                if let Ok(tracks) = load_recent_tracks(session_str).await {
                                    state.with_mut(|s| s.tracks = tracks);
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
fn RuleWorkshop(mut state: Signal<AppState>) -> Element {
    let mut loading_tracks = use_signal(|| false);

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
                    h2 { style: "font-size: 1.25rem; font-weight: bold;", "Live Preview on Recent Tracks" }

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
                            let session_str = state.read().session.clone();
                            if let Some(session_str) = session_str {
                                loading_tracks.set(true);
                                if let Ok(tracks) = load_recent_tracks(session_str).await {
                                    state.with_mut(|s| s.tracks = tracks);
                                }
                                loading_tracks.set(false);
                            }
                        },
                        if *loading_tracks.read() {
                            "Loading..."
                        } else {
                            "Load Recent Tracks"
                        }
                        }
                    }
                }
                TracksPreview { state }
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

            // Clear button
            button {
                style: "background: #dc2626; color: white; padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: pointer; align-self: flex-start;",
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

#[component]
fn TracksPreview(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    // Compute matches and prepare tracks for display based on toggle
    let mut tracks_to_display = Vec::new();
    let mut matching_count = 0;
    let total_tracks = state_read.tracks.len();

    for (idx, strack) in state_read.tracks.iter().enumerate() {
        let track: Track = strack.clone().into();
        let mut edit = create_no_op_edit(&track);
        let _rule_applied =
            apply_all_rules(&[state_read.current_rule.clone()], &mut edit).unwrap_or_default();

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

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1rem;",
            // Count display
            div { style: "padding: 0.75rem 1rem; background: #f3f4f6; border-radius: 0.5rem; font-weight: 500; color: #374151;",
                if total_tracks == 0 {
                    "No tracks loaded"
                } else if state_read.show_all_tracks {
                    "Rule matches: {matching_count}/{total_tracks} tracks (showing all)"
                } else {
                    "Rule matches: {matching_count}/{total_tracks} tracks (showing {matching_count} matches only)"
                }
            }

            if state_read.tracks.is_empty() {
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

// Helper functions

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

    // Try to fetch real recent tracks
    let mut tracks = Vec::new();
    let mut recent_iterator = client.recent_tracks();
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
