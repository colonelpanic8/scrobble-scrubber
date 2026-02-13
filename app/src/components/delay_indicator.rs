use crate::types::AppState;
use chrono::Utc;
use dioxus::prelude::*;

#[component]
pub fn DelayIndicator(state: Signal<AppState>) -> Element {
    let delay_state = state.read().scrubber_state.delay_state.clone();
    let mut timer_tick = use_signal(Utc::now);

    // Update countdown every second.
    use_future(move || async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            timer_tick.set(Utc::now());
        }
    });

    let Some(delay) = delay_state else {
        return rsx! { div { style: "display: none;" } };
    };

    if !delay.is_delaying {
        return rsx! { div { style: "display: none;" } };
    }

    let _tick = timer_tick.read();
    let now = Utc::now();
    let remaining_seconds = (delay.until - now).num_seconds().max(0);

    // If already expired, don't display (next client activity should clear it).
    if remaining_seconds == 0 {
        return rsx! { div { style: "display: none;" } };
    }

    let remaining_text = if remaining_seconds < 60 {
        format!("{remaining_seconds}s")
    } else {
        let minutes = remaining_seconds / 60;
        let seconds = remaining_seconds % 60;
        format!("{minutes}m {seconds}s")
    };

    let reason = delay.reason.clone().unwrap_or_else(|| "Delay".to_string());

    rsx! {
        div {
            style: "
                background: linear-gradient(135deg, #0f766e, #14b8a6);
                color: white;
                padding: 0.75rem 1rem;
                border-radius: 0.5rem;
                box-shadow: 0 4px 12px rgba(20, 184, 166, 0.25);
                margin-bottom: 1rem;
                border-left: 4px solid #115e59;
            ",

            div { style: "display: flex; align-items: center; gap: 0.75rem;",
                div { style: "font-size: 1.5rem;", "⏳" }
                div { style: "flex: 1;",
                    div { style: "font-weight: 700; font-size: 1rem; margin-bottom: 0.25rem;",
                        "WAITING ({remaining_text})"
                    }
                    div { style: "opacity: 0.95; font-size: 0.95rem;",
                        "{delay.message}"
                    }
                    div { style: "opacity: 0.8; font-size: 0.8rem; font-family: monospace; margin-top: 0.25rem;",
                        "Reason: {reason}"
                    }
                }
            }
        }
    }
}
