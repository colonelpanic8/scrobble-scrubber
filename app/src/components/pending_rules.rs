use crate::api::{
    approve_pending_rewrite_rule, load_pending_rewrite_rules, reject_pending_rewrite_rule,
};
use crate::types::AppState;
use dioxus::prelude::*;
use scrobble_scrubber::persistence::PendingRewriteRule;

// Helper to create async operation handlers that manage error/success state
fn create_operation_handler<F, Fut>(
    operation: F,
    mut success_message: Signal<String>,
    mut error_message: Signal<String>,
    reload_data: impl Fn() + Copy + 'static,
) -> impl Fn() + 'static
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>>
        + 'static,
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
pub fn PendingRulesPage(_state: Signal<AppState>) -> Element {
    let mut pending_rules = use_signal(Vec::<PendingRewriteRule>::new);
    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(String::new);
    let success_message = use_signal(String::new);

    // Load all pending rules on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error_message.set(String::new());

            match load_pending_rewrite_rules().await {
                Ok(rules) => {
                    pending_rules.set(rules);
                }
                Err(e) => error_message.set(format!("Failed to load pending rules: {e}")),
            }

            loading.set(false);
        });
    });

    let reload_data = move || {
        spawn(async move {
            match load_pending_rewrite_rules().await {
                Ok(rules) => {
                    pending_rules.set(rules);
                    error_message.set(String::new()); // Clear any previous errors
                }
                Err(e) => {
                    error_message.set(format!("Failed to reload pending rules: {e}"));
                }
            }
        });
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            h1 { style: "font-size: 2rem; font-weight: bold; margin-bottom: 1rem;", "Pending Rules" }

            if *loading.read() && pending_rules.read().is_empty() {
                div { style: "text-align: center; padding: 2rem;",
                    p { "Loading pending rules..." }
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

            // Pending Rules Section
            div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;",
                    h2 { style: "font-size: 1.5rem; font-weight: bold;", "All Pending Rules" }
                    span { style: "background: #e5e7eb; color: #374151; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.875rem;",
                        "{pending_rules.read().len()} items"
                    }
                }

                if pending_rules.read().is_empty() && !*loading.read() {
                    div { style: "text-align: center; color: #6b7280; padding: 2rem;",
                        p { "No pending rewrite rules found." }
                        p { style: "font-size: 0.875rem; margin-top: 0.5rem;",
                            "Pending rules will appear here when the scrubber suggests new rewrite rules based on track patterns."
                        }
                    }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: 1rem;",
                        for rule in pending_rules.read().iter() {
                            PendingRuleCard {
                                rule_id: rule.id.clone(),
                                reason: rule.reason.clone(),
                                example_track_name: rule.example_track_name.clone(),
                                example_artist_name: rule.example_artist_name.clone(),
                                example_album_name: rule.example_album_name.clone(),
                                rule_description: get_rule_description(&rule.rule),
                                on_approve: {
                                    let rule_id = rule.id.clone();
                                    let handler = create_operation_handler(
                                        move || approve_pending_rewrite_rule(rule_id.clone()),
                                        success_message,
                                        error_message,
                                        reload_data,
                                    );
                                    move |_| handler()
                                },
                                on_reject: {
                                    let rule_id = rule.id.clone();
                                    let handler = create_operation_handler(
                                        move || reject_pending_rewrite_rule(rule_id.clone()),
                                        success_message,
                                        error_message,
                                        reload_data,
                                    );
                                    move |_| handler()
                                },
                            }
                        }
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
    on_approve: EventHandler<()>,
    on_reject: EventHandler<()>,
) -> Element {
    rsx! {
        div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; background: #f9fafb;",
            div { style: "display: flex; justify-content: between; align-items: start; gap: 1rem;",
                div { style: "flex: 1;",
                    h3 { style: "font-weight: bold; margin-bottom: 0.5rem; color: #1f2937;", "Rewrite Rule" }

                    div { style: "margin-bottom: 0.75rem;",
                        div { style: "font-size: 0.875rem; color: #374151; margin-bottom: 0.25rem;",
                            strong { "Rule: " }
                            "{rule_description}"
                        }
                    }

                    div { style: "margin-bottom: 0.75rem; padding: 0.5rem; background: #f3f4f6; border-radius: 0.25rem; font-size: 0.875rem; color: #6b7280;",
                        strong { "Reason: " }
                        "{reason}"
                    }

                    div { style: "padding: 0.5rem; background: #eff6ff; border-radius: 0.25rem; font-size: 0.875rem;",
                        strong { style: "color: #1e40af;", "Example Track: " }
                        span { style: "color: #374151;", "'{example_track_name}' by '{example_artist_name}'" }
                        if let Some(album) = &example_album_name {
                            span { style: "color: #6b7280;", " from '{album}'" }
                        }
                    }
                }

                div { style: "display: flex; gap: 0.5rem;",
                    button {
                        style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| on_approve.call(()),
                        "Approve"
                    }
                    button {
                        style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |_| on_reject.call(()),
                        "Reject"
                    }
                }
            }
        }
    }
}
