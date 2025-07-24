use crate::server_functions::{approve_pending_edit, load_pending_edits, reject_pending_edit};
use crate::types::AppState;
use dioxus::prelude::*;
use scrobble_scrubber::persistence::PendingEdit;

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

#[component]
pub fn PendingEditsPage(_state: Signal<AppState>) -> Element {
    let mut pending_edits = use_signal(Vec::<PendingEdit>::new);
    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(String::new);
    let success_message = use_signal(String::new);

    // Load all pending edits on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error_message.set(String::new());

            match load_pending_edits().await {
                Ok(edits) => {
                    pending_edits.set(edits);
                }
                Err(e) => error_message.set(format!("Failed to load pending edits: {e}")),
            }

            loading.set(false);
        });
    });

    let reload_data = move || {
        spawn(async move {
            match load_pending_edits().await {
                Ok(edits) => {
                    pending_edits.set(edits);
                    error_message.set(String::new()); // Clear any previous errors
                }
                Err(e) => {
                    error_message.set(format!("Failed to reload pending edits: {e}"));
                }
            }
        });
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
            h1 { style: "font-size: 2rem; font-weight: bold; margin-bottom: 1rem;", "Pending Edits" }

            if *loading.read() && pending_edits.read().is_empty() {
                div { style: "text-align: center; padding: 2rem;",
                    p { "Loading pending edits..." }
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
                    h2 { style: "font-size: 1.5rem; font-weight: bold;", "All Pending Edits" }
                    span { style: "background: #e5e7eb; color: #374151; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.875rem;",
                        "{pending_edits.read().len()} items"
                    }
                }

                if pending_edits.read().is_empty() && !*loading.read() {
                    div { style: "text-align: center; color: #6b7280; padding: 2rem;",
                        p { "No pending edits found." }
                        p { style: "font-size: 0.875rem; margin-top: 0.5rem;",
                            "Pending edits will appear here when the scrubber suggests track modifications."
                        }
                    }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: 1rem;",
                        for edit in pending_edits.read().iter() {
                            PendingEditCard {
                                edit_id: edit.id.clone(),
                                original_track_name: edit.original_track_name.clone(),
                                original_artist_name: edit.original_artist_name.clone(),
                                original_album_name: edit.original_album_name.clone(),
                                new_track_name: edit.new_track_name.clone(),
                                new_artist_name: edit.new_artist_name.clone(),
                                new_album_name: edit.new_album_name.clone(),
                                on_approve: {
                                    let edit_id = edit.id.clone();
                                    let handler = create_operation_handler(
                                        move || approve_pending_edit(edit_id.clone()),
                                        success_message,
                                        error_message,
                                        reload_data,
                                    );
                                    move |_| handler()
                                },
                                on_reject: {
                                    let edit_id = edit.id.clone();
                                    let handler = create_operation_handler(
                                        move || reject_pending_edit(edit_id.clone()),
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
fn PendingEditCard(
    edit_id: String,
    original_track_name: String,
    original_artist_name: String,
    original_album_name: Option<String>,
    new_track_name: Option<String>,
    new_artist_name: Option<String>,
    new_album_name: Option<String>,
    on_approve: EventHandler<()>,
    on_reject: EventHandler<()>,
) -> Element {
    rsx! {
        div { style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; background: #f9fafb;",
            div { style: "display: flex; justify-content: between; align-items: start; gap: 1rem;",
                div { style: "flex: 1;",
                    h3 { style: "font-weight: bold; margin-bottom: 0.5rem; color: #1f2937;", "Track Edit" }

                    div { style: "display: grid; grid-template-columns: auto 1fr; gap: 0.5rem 1rem; font-size: 0.875rem;",
                        span { style: "font-weight: 500; color: #374151;", "Track:" }
                        div {
                            span { style: "color: #dc2626;", "{original_track_name}" }
                            " → "
                            span { style: "color: #059669;", "{new_track_name.as_deref().unwrap_or(&original_track_name)}" }
                        }

                        span { style: "font-weight: 500; color: #374151;", "Artist:" }
                        div {
                            span { style: "color: #dc2626;", "{original_artist_name}" }
                            " → "
                            span { style: "color: #059669;", "{new_artist_name.as_deref().unwrap_or(&original_artist_name)}" }
                        }

                        if let Some(original_album) = &original_album_name {
                            span { style: "font-weight: 500; color: #374151;", "Album:" }
                            div {
                                span { style: "color: #dc2626;", "{original_album}" }
                                " → "
                                span {
                                    style: "color: #059669;",
                                    {new_album_name.as_deref().unwrap_or("(empty)")}
                                }
                            }
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
