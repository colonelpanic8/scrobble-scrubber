use crate::components::scrobble_scrubber::{
    start_scrubber, stop_scrubber, trigger_manual_processing,
};
use crate::types::{AppState, ScrubberStatus};
use chrono::Utc;
use dioxus::prelude::*;

#[component]
pub fn ScrubberControlsSection(mut state: Signal<AppState>) -> Element {
    let scrubber_state = state.read().scrubber_state.clone();
    let mut timer_tick = use_signal(chrono::Utc::now);

    use_future(move || async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            timer_tick.set(chrono::Utc::now());
        }
    });

    rsx! {
        div { style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 1.5rem;",
            div { style: "display: flex; justify-content: flex-end; align-items: center; margin-bottom: 1rem;",
                div { style: "display: flex; align-items: center; gap: 1rem;",
                    ScrubberStatusIndicator {
                        status: scrubber_state.status.clone(),
                        timer_tick
                    }
                    ScrubberControlButtons {
                        status: scrubber_state.status.clone(),
                        state
                    }
                }
            }

            p { style: "color: #6b7280; margin: 0;",
                "Monitor and control the scrobble scrubber. The scrubber processes your scrobbles and applies rewrite rules automatically."
            }
        }
    }
}

#[component]
fn ScrubberStatusIndicator(
    status: ScrubberStatus,
    timer_tick: Signal<chrono::DateTime<Utc>>,
) -> Element {
    let (style, text) = match &status {
        ScrubberStatus::Stopped => (
            "background: #fee2e2; color: #991b1b;",
            "Stopped".to_string(),
        ),
        ScrubberStatus::Starting => (
            "background: #fef3c7; color: #92400e;",
            "Starting...".to_string(),
        ),
        ScrubberStatus::Running => (
            "background: #dcfce7; color: #166534;",
            "Running".to_string(),
        ),
        ScrubberStatus::Sleeping { until_timestamp } => {
            let _tick = timer_tick.read();
            let now = chrono::Utc::now();
            let remaining_seconds = (*until_timestamp - now).num_seconds().max(0);

            let text = if remaining_seconds > 0 {
                format!("💤 Sleeping ({remaining_seconds}s)")
            } else {
                "💤 Sleeping".to_string()
            };
            ("background: #e0e7ff; color: #3730a3;", text)
        }
        ScrubberStatus::Stopping => (
            "background: #fef3c7; color: #92400e;",
            "Stopping...".to_string(),
        ),
        ScrubberStatus::Error(err) => (
            "background: #fecaca; color: #dc2626;",
            format!("Error: {err}"),
        ),
    };

    rsx! {
        div {
            style: format!(
                "padding: 0.5rem 1rem; border-radius: 0.375rem; font-size: 0.875rem; font-weight: 500; {}",
                style
            ),
            "{text}"
        }
    }
}

#[component]
fn ScrubberControlButtons(status: ScrubberStatus, mut state: Signal<AppState>) -> Element {
    match status {
        ScrubberStatus::Stopped | ScrubberStatus::Error(_) => rsx! {
            button {
                style: "background: #059669; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; margin-right: 0.5rem;",
                onclick: move |_| {
                    spawn(async move {
                        start_scrubber(state).await;
                    });
                },
                "Start Scrubber"
            }
            button {
                style: "background: #2563eb; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                onclick: move |_| {
                    spawn(async move {
                        trigger_manual_processing(state).await;
                    });
                },
                "Process Now"
            }
        },
        ScrubberStatus::Running | ScrubberStatus::Sleeping { .. } => rsx! {
            button {
                style: "background: #dc2626; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; margin-right: 0.5rem;",
                onclick: move |_| {
                    spawn(async move {
                        stop_scrubber(state).await;
                    });
                },
                "Stop Scrubber"
            }
            button {
                style: "background: #7c3aed; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                onclick: move |_| {
                    spawn(async move {
                        trigger_manual_processing(state).await;
                    });
                },
                "Process Now"
            }
        },
        ScrubberStatus::Starting | ScrubberStatus::Stopping => rsx! {
            button {
                style: "background: #6b7280; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: not-allowed; font-size: 0.875rem;",
                disabled: true,
                "Please wait..."
            }
        },
    }
}
