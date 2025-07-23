use crate::server_functions::{
    approve_pending_edit, approve_pending_rewrite_rule, load_pending_edits_from_page,
    load_pending_rewrite_rules_from_page, reject_pending_edit, reject_pending_rewrite_rule,
};
use crate::types::AppState;
use dioxus::prelude::*;
use scrobble_scrubber::persistence::{PendingEdit, PendingRewriteRule};

// Helper to create async operation handlers that manage error/success state
fn create_operation_handler<F, Fut>(
    operation: F,
    mut success_message: Signal<String>,
    mut error_message: Signal<String>,
    reload_data: impl Fn() + Copy + 'static,
) -> impl Fn() + 'static
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = Result<String, dioxus::prelude::ServerFnError>> + 'static,
{
    move || {
        let operation = operation.clone();
        spawn(async move {
            success_message.set(String::new());
            error_message.set(String::new());

            match operation().await {
                Ok(msg) => {
                    success_message.set(msg);
                    reload_data();
                }
                Err(e) => error_message.set(e.to_string()),
            }
        });
    }
}

// Helper for loading more items with pagination
fn create_load_more_handler<T, F, Fut>(
    load_function: F,
    mut items_signal: Signal<Vec<T>>,
    mut current_page: Signal<u32>,
    mut loading_signal: Signal<bool>,
    mut error_message: Signal<String>,
) -> impl Fn() + 'static
where
    T: 'static,
    F: Fn(u32) -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = Result<Vec<T>, dioxus::prelude::ServerFnError>> + 'static,
{
    move || {
        let load_function = load_function.clone();
        spawn(async move {
            loading_signal.set(true);
            let next_page = *current_page.read() + 1;

            match load_function(next_page).await {
                Ok(mut new_items) => {
                    if !new_items.is_empty() {
                        items_signal.with_mut(|items| {
                            items.append(&mut new_items);
                        });
                        current_page.set(next_page);
                    }
                }
                Err(e) => error_message.set(format!("Failed to load more items: {e}")),
            }

            loading_signal.set(false);
        });
    }
}

fn get_rule_description(rule: &scrobble_scrubber::rewrite::RewriteRule) -> String {
    let rule_name = rule.name.as_deref().unwrap_or("Unnamed Rule");

    let rule_detail = if let Some(track_rule) = &rule.track_name {
        format!("Track name: {} → {}", track_rule.find, track_rule.replace)
    } else if let Some(artist_rule) = &rule.artist_name {
        format!(
            "Artist name: {} → {}",
            artist_rule.find, artist_rule.replace
        )
    } else if let Some(album_rule) = &rule.album_name {
        format!("Album name: {} → {}", album_rule.find, album_rule.replace)
    } else if let Some(album_artist_rule) = &rule.album_artist_name {
        format!(
            "Album artist: {} → {}",
            album_artist_rule.find, album_artist_rule.replace
        )
    } else {
        "No active rules".to_string()
    };

    format!("{rule_name}: {rule_detail}")
}

#[component]
pub fn PendingItemsPage(state: Signal<AppState>) -> Element {
    let mut pending_edits = use_signal(Vec::<PendingEdit>::new);
    let mut pending_rules = use_signal(Vec::<PendingRewriteRule>::new);
    let mut loading_edits = use_signal(|| false);
    let mut loading_rules = use_signal(|| false);
    let mut error_message = use_signal(String::new);
    let success_message = use_signal(String::new);
    let mut current_edits_page = use_signal(|| 0u32);
    let mut current_rules_page = use_signal(|| 0u32);

    // Load initial pending items on mount
    use_effect(move || {
        spawn(async move {
            loading_edits.set(true);
            loading_rules.set(true);
            error_message.set(String::new());

            // Load first page of pending edits
            match load_pending_edits_from_page(1).await {
                Ok(edits) => {
                    pending_edits.set(edits);
                    current_edits_page.set(1);
                }
                Err(e) => error_message.set(format!("Failed to load pending edits: {e}")),
            }

            // Load first page of pending rules
            match load_pending_rewrite_rules_from_page(1).await {
                Ok(rules) => {
                    pending_rules.set(rules);
                    current_rules_page.set(1);
                }
                Err(e) => {
                    let current_error = error_message.read().clone();
                    if current_error.is_empty() {
                        error_message.set(format!("Failed to load pending rules: {e}"));
                    } else {
                        error_message.set(format!(
                            "{current_error}; Failed to load pending rules: {e}"
                        ));
                    }
                }
            }

            loading_edits.set(false);
            loading_rules.set(false);
        });
    });

    let reload_data = move || {
        spawn(async move {
            // Reload from page 1 and reset pagination
            if let Ok(edits) = load_pending_edits_from_page(1).await {
                pending_edits.set(edits);
                current_edits_page.set(1);
            }
            if let Ok(rules) = load_pending_rewrite_rules_from_page(1).await {
                pending_rules.set(rules);
                current_rules_page.set(1);
            }
        });
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            h1 { style: "font-size: 2rem; font-weight: bold; margin-bottom: 1rem;", "Pending Items" }

            if *loading_edits.read() && pending_edits.read().is_empty() {
                div { style: "text-align: center; padding: 2rem;",
                    p { "Loading pending items..." }
                }
            }

            if !error_message.read().is_empty() {
                div { style: "background: #fee2e2; border: 1px solid #fca5a5; color: #dc2626; padding: 1rem; border-radius: 0.5rem;",
                    p { "Error: {error_message}" }
                }
            }

            if !success_message.read().is_empty() {
                div { style: "background: #d1fae5; border: 1px solid #6ee7b7; color: #059669; padding: 1rem; border-radius: 0.5rem;",
                    p { "{success_message}" }
                }
            }

            // Pending Edits Section
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h2 { style: "font-size: 1.5rem; font-weight: bold;", "Pending Edits" }
                    div { style: "display: flex; align-items: center; gap: 1rem;",
                        span { style: "background: #e5e7eb; color: #374151; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.875rem;",
                            "{pending_edits.read().len()} items"
                        }
                        button {
                            style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {}; font-size: 0.875rem;",
                                "#059669",
                                if *loading_edits.read() { "0.5" } else { "1" }
                            ),
                            disabled: *loading_edits.read(),
                            onclick: move |_| {
                                let handler = create_load_more_handler(
                                    load_pending_edits_from_page,
                                    pending_edits,
                                    current_edits_page,
                                    loading_edits,
                                    error_message,
                                );
                                handler();
                            },
                            if *loading_edits.read() {
                                "Loading..."
                            } else {
                                "Load More Edits"
                            }
                        }
                    }
                }

                if pending_edits.read().is_empty() && !*loading_edits.read() {
                    div { style: "text-align: center; color: #6b7280; padding: 2rem;",
                        p { "No pending edits" }
                    }
                } else {
                    div { style: "space-y: 1rem;",
                        for edit in pending_edits.read().iter() {
                            PendingEditCard {
                                key: "{edit.id}",
                                edit_id: edit.id.clone(),
                                original_track_name: edit.original_track_name.clone(),
                                original_artist_name: edit.original_artist_name.clone(),
                                original_album_name: edit.original_album_name.clone(),
                                new_track_name: edit.new_track_name.clone(),
                                new_artist_name: edit.new_artist_name.clone(),
                                new_album_name: edit.new_album_name.clone(),
                                on_approve: move |edit_id: String| {
                                    let handler = create_operation_handler(
                                        move || approve_pending_edit(edit_id.clone()),
                                        success_message,
                                        error_message,
                                        reload_data,
                                    );
                                    handler();
                                },
                                on_reject: move |edit_id: String| {
                                    let handler = create_operation_handler(
                                        move || reject_pending_edit(edit_id.clone()),
                                        success_message,
                                        error_message,
                                        reload_data,
                                    );
                                    handler();
                                }
                            }
                        }
                    }
                }
            }

            // Pending Rewrite Rules Section
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h2 { style: "font-size: 1.5rem; font-weight: bold;", "Pending Rewrite Rules" }
                    div { style: "display: flex; align-items: center; gap: 1rem;",
                        span { style: "background: #e5e7eb; color: #374151; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.875rem;",
                            "{pending_rules.read().len()} items"
                        }
                        button {
                            style: format!("background: {}; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; opacity: {}; font-size: 0.875rem;",
                                "#059669",
                                if *loading_rules.read() { "0.5" } else { "1" }
                            ),
                            disabled: *loading_rules.read(),
                            onclick: move |_| {
                                let handler = create_load_more_handler(
                                    load_pending_rewrite_rules_from_page,
                                    pending_rules,
                                    current_rules_page,
                                    loading_rules,
                                    error_message,
                                );
                                handler();
                            },
                            if *loading_rules.read() {
                                "Loading..."
                            } else {
                                "Load More Rules"
                            }
                        }
                    }
                }

                if pending_rules.read().is_empty() && !*loading_rules.read() {
                    div { style: "text-align: center; color: #6b7280; padding: 2rem;",
                        p { "No pending rewrite rules" }
                    }
                } else {
                    div { style: "space-y: 1rem;",
                        for rule in pending_rules.read().iter() {
                            PendingRuleCard {
                                key: "{rule.id}",
                                rule_id: rule.id.clone(),
                                reason: rule.reason.clone(),
                                example_track_name: rule.example_track_name.clone(),
                                example_artist_name: rule.example_artist_name.clone(),
                                example_album_name: rule.example_album_name.clone(),
                                rule_description: get_rule_description(&rule.rule),
                                on_approve: move |rule_id: String| {
                                    let handler = create_operation_handler(
                                        move || approve_pending_rewrite_rule(rule_id.clone()),
                                        success_message,
                                        error_message,
                                        reload_data,
                                    );
                                    handler();
                                },
                                on_reject: move |rule_id: String| {
                                    let handler = create_operation_handler(
                                        move || reject_pending_rewrite_rule(rule_id.clone()),
                                        success_message,
                                        error_message,
                                        reload_data,
                                    );
                                    handler();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PendingEditCard(
    edit_id: String,
    original_track_name: String,
    original_artist_name: String,
    original_album_name: Option<String>,
    new_track_name: Option<String>,
    new_artist_name: Option<String>,
    new_album_name: Option<String>,
    on_approve: EventHandler<String>,
    on_reject: EventHandler<String>,
) -> Element {
    rsx! {
        div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; margin-bottom: 1rem;",
            div { style: "display: flex; justify-content: between; align-items: start; margin-bottom: 1rem;",
                div { style: "flex: 1;",
                    h3 { style: "font-weight: 600; margin-bottom: 0.5rem;", "Track Edit" }
                    div { style: "grid-template-columns: auto 1fr; gap: 0.5rem; font-size: 0.875rem;",
                        div { style: "display: grid; grid-template-columns: auto 1fr; gap: 0.5rem 1rem;",
                            span { style: "font-weight: 500; color: #374151;", "Track:" }
                            span { style: "color: #6b7280;", "{original_track_name}" }
                            if let Some(new_name) = &new_track_name {
                                span { style: "font-weight: 500; color: #374151;", "→" }
                                span { style: "color: #059669; font-weight: 500;", "{new_name}" }
                            }

                            span { style: "font-weight: 500; color: #374151;", "Artist:" }
                            span { style: "color: #6b7280;", "{original_artist_name}" }
                            if let Some(new_artist) = &new_artist_name {
                                span { style: "font-weight: 500; color: #374151;", "→" }
                                span { style: "color: #059669; font-weight: 500;", "{new_artist}" }
                            }

                            if let Some(album) = &original_album_name {
                                span { style: "font-weight: 500; color: #374151;", "Album:" }
                                span { style: "color: #6b7280;", "{album}" }
                                if let Some(new_album) = &new_album_name {
                                    span { style: "font-weight: 500; color: #374151;", "→" }
                                    span { style: "color: #059669; font-weight: 500;", "{new_album}" }
                                }
                            } else if let Some(new_album) = &new_album_name {
                                span { style: "font-weight: 500; color: #374151;", "Album:" }
                                span { style: "color: #6b7280;", "(none)" }
                                span { style: "font-weight: 500; color: #374151;", "→" }
                                span { style: "color: #059669; font-weight: 500;", "{new_album}" }
                            }
                        }
                    }
                }
                div { style: "display: flex; gap: 0.5rem;",
                    button {
                        style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: {
                            let edit_id = edit_id.clone();
                            move |_| on_approve.call(edit_id.clone())
                        },
                        "Approve"
                    }
                    button {
                        style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: {
                            let edit_id = edit_id.clone();
                            move |_| on_reject.call(edit_id.clone())
                        },
                        "Reject"
                    }
                }
            }
        }
    }
}

#[component]
fn PendingRuleCard(
    rule_id: String,
    reason: String,
    example_track_name: String,
    example_artist_name: String,
    example_album_name: Option<String>,
    rule_description: String,
    on_approve: EventHandler<String>,
    on_reject: EventHandler<String>,
) -> Element {
    rsx! {
        div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; margin-bottom: 1rem;",
            div { style: "display: flex; justify-content: between; align-items: start; margin-bottom: 1rem;",
                div { style: "flex: 1;",
                    h3 { style: "font-weight: 600; margin-bottom: 0.5rem;", "Rewrite Rule" }
                    div { style: "margin-bottom: 1rem;",
                        div { style: "display: grid; grid-template-columns: auto 1fr; gap: 0.5rem 1rem; font-size: 0.875rem;",
                            span { style: "font-weight: 500; color: #374151;", "Rule:" }
                            span { style: "color: #6b7280; font-family: monospace; background: #f3f4f6; padding: 0.25rem; border-radius: 0.25rem;", "{rule_description}" }
                        }
                    }

                    div { style: "margin-bottom: 1rem;",
                        h4 { style: "font-weight: 500; margin-bottom: 0.5rem; color: #374151;", "Reason:" }
                        p { style: "color: #6b7280; font-size: 0.875rem;", "{reason}" }
                    }

                    div { style: "margin-bottom: 1rem;",
                        h4 { style: "font-weight: 500; margin-bottom: 0.5rem; color: #374151;", "Example:" }
                        div { style: "background: #f9fafb; padding: 0.75rem; border-radius: 0.375rem; font-size: 0.875rem;",
                            div { style: "display: grid; grid-template-columns: auto 1fr; gap: 0.5rem 1rem;",
                                span { style: "font-weight: 500; color: #374151;", "Track:" }
                                span { style: "color: #6b7280;", "{example_track_name}" }

                                span { style: "font-weight: 500; color: #374151;", "Artist:" }
                                span { style: "color: #6b7280;", "{example_artist_name}" }

                                if let Some(album) = &example_album_name {
                                    span { style: "font-weight: 500; color: #374151;", "Album:" }
                                    span { style: "color: #6b7280;", "{album}" }
                                }
                            }
                        }
                    }
                }
                div { style: "display: flex; gap: 0.5rem;",
                    button {
                        style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: {
                            let rule_id = rule_id.clone();
                            move |_| on_approve.call(rule_id.clone())
                        },
                        "Approve"
                    }
                    button {
                        style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: {
                            let rule_id = rule_id.clone();
                            move |_| on_reject.call(rule_id.clone())
                        },
                        "Reject"
                    }
                }
            }
        }
    }
}
