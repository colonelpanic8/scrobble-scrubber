use crate::types::AppState;
use crate::utils::{copy_to_clipboard, save_current_rule};
use ::scrobble_scrubber::rewrite::{RewriteRule, SdRule};
use dioxus::prelude::*;

#[component]
pub fn RuleEditor(mut state: Signal<AppState>) -> Element {
    // Separate state for each field
    let mut track_find = use_signal(String::new);
    let mut track_replace = use_signal(String::new);
    let mut artist_find = use_signal(String::new);
    let mut artist_replace = use_signal(String::new);
    let mut album_find = use_signal(String::new);
    let mut album_replace = use_signal(String::new);
    let mut album_artist_find = use_signal(String::new);
    let mut album_artist_replace = use_signal(String::new);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",

            // Track Name
            div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem;",
                h3 { style: "font-weight: 600; margin-bottom: 1rem; color: #374151;", "Track Name" }
                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;",
                    div {
                        label { style: "display: block; font-size: 0.875rem; font-weight: 500; color: #6b7280; margin-bottom: 0.25rem;", "Find" }
                        input {
                            style: "width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 0.375rem;",
                            placeholder: "Regex pattern",
                            value: "{track_find}",
                            oninput: move |e| {
                                track_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
                            placeholder: "Regex pattern",
                            value: "{artist_find}",
                            oninput: move |e| {
                                artist_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
                            placeholder: "Regex pattern",
                            value: "{album_find}",
                            oninput: move |e| {
                                album_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
                            placeholder: "Regex pattern",
                            value: "{album_artist_find}",
                            oninput: move |e| {
                                album_artist_find.set(e.value());
                                update_all_rules(state, &track_find, &track_replace, &artist_find, &artist_replace,
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
                                               &album_find, &album_replace, &album_artist_find, &album_artist_replace);
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
) {
    let mut rule = RewriteRule::new();

    // Add track name rule if provided
    let track_find_str = track_find.read();
    let track_replace_str = track_replace.read();
    if !track_find_str.is_empty() {
        let sd_rule = SdRule::new(&track_find_str, &track_replace_str);
        rule = rule.with_track_name(sd_rule);
    }

    // Add artist name rule if provided
    let artist_find_str = artist_find.read();
    let artist_replace_str = artist_replace.read();
    if !artist_find_str.is_empty() {
        let sd_rule = SdRule::new(&artist_find_str, &artist_replace_str);
        rule = rule.with_artist_name(sd_rule);
    }

    // Add album name rule if provided
    let album_find_str = album_find.read();
    let album_replace_str = album_replace.read();
    if !album_find_str.is_empty() {
        let sd_rule = SdRule::new(&album_find_str, &album_replace_str);
        rule = rule.with_album_name(sd_rule);
    }

    // Add album artist rule if provided
    let album_artist_find_str = album_artist_find.read();
    let album_artist_replace_str = album_artist_replace.read();
    if !album_artist_find_str.is_empty() {
        let sd_rule = SdRule::new(&album_artist_find_str, &album_artist_replace_str);
        rule = rule.with_album_artist_name(sd_rule);
    }

    state.with_mut(|s| s.current_rule = rule);
}
