use crate::types::AppState;
use chrono::Utc;
use dioxus::prelude::*;

#[component]
pub fn RateLimitIndicator(mut state: Signal<AppState>) -> Element {
    let rate_limit_state = state.read().scrubber_state.rate_limit_state.clone();
    let mut timer_tick = use_signal(Utc::now);

    // Set up a timer to update the countdown every second
    use_future(move || async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            timer_tick.set(Utc::now());
        }
    });

    if let Some(rate_limit) = rate_limit_state {
        if rate_limit.is_rate_limited {
            let _tick = timer_tick.read(); // Subscribe to timer updates
            let now = Utc::now();
            let seconds_ago = (now - rate_limit.detected_at).num_seconds();

            // Calculate remaining time and expiry time if we have retry_after
            let (remaining_time, countdown_text, expiry_time) =
                if let Some(retry_after) = rate_limit.retry_after {
                    let remaining_seconds = (retry_after - now).num_seconds().max(0);
                    let remaining_time = if remaining_seconds > 0 {
                        if remaining_seconds < 60 {
                            format!("{remaining_seconds}s")
                        } else {
                            let minutes = remaining_seconds / 60;
                            let seconds = remaining_seconds % 60;
                            format!("{minutes}m {seconds}s")
                        }
                    } else {
                        "Should end soon".to_string()
                    };

                    let countdown = if remaining_seconds > 0 {
                        format!(" ({remaining_time})")
                    } else {
                        " (should end soon)".to_string()
                    };

                    // Format the expiry time in local timezone
                    let local_expiry = retry_after.format("%H:%M:%S");
                    let expiry_display = format!("Expires at {local_expiry}");

                    (Some(remaining_time), countdown, Some(expiry_display))
                } else {
                    (None, String::new(), None)
                };

            rsx! {
                div {
                    style: "
                        background: linear-gradient(135deg, #dc2626, #ef4444);
                        color: white;
                        padding: 1rem 1.5rem;
                        border-radius: 0.5rem;
                        box-shadow: 0 4px 12px rgba(220, 38, 38, 0.3);
                        margin-bottom: 1.5rem;
                        border-left: 4px solid #b91c1c;
                        animation: pulse 2s infinite;
                    ",

                    div {
                        style: "display: flex; align-items: center; gap: 1rem;",

                        // Warning icon
                        div {
                            style: "
                                font-size: 2rem;
                                animation: bounce 1s infinite;
                            ",
                            "⚠️"
                        }

                        div {
                            style: "flex: 1;",

                            h3 {
                                style: "
                                    font-size: 1.25rem;
                                    font-weight: 700;
                                    margin: 0 0 0.5rem 0;
                                    text-shadow: 0 1px 2px rgba(0,0,0,0.1);
                                ",
                                "🚫 RATE LIMITED{countdown_text}"
                            }

                            p {
                                style: "
                                    margin: 0 0 0.5rem 0;
                                    font-size: 1rem;
                                    opacity: 0.95;
                                ",
                                "{rate_limit.message}"
                            }

                            if let Some(remaining) = remaining_time {
                                p {
                                    style: "
                                        margin: 0 0 0.5rem 0;
                                        font-size: 1.1rem;
                                        font-weight: 600;
                                        color: #fbbf24;
                                    ",
                                    "⏱️ Time remaining: {remaining}"
                                }
                            }

                            if let Some(expiry) = expiry_time {
                                p {
                                    style: "
                                        margin: 0 0 0.5rem 0;
                                        font-size: 1rem;
                                        font-weight: 500;
                                        color: #60a5fa;
                                        display: flex;
                                        align-items: center;
                                        gap: 0.5rem;
                                    ",
                                    "🕒 {expiry}"
                                }
                            }

                            p {
                                style: "
                                    margin: 0;
                                    font-size: 0.875rem;
                                    opacity: 0.8;
                                ",
                                {
                                    if rate_limit.retry_after.is_some() {
                                        "Processing will resume automatically when the countdown ends.".to_string()
                                    } else {
                                        format!("Detected {seconds_ago} seconds ago. Processing will resume automatically when rate limit expires.")
                                    }
                                }
                            }

                            if let Some(rate_type) = &rate_limit.rate_limit_type {
                                p {
                                    style: "
                                        margin: 0.25rem 0 0 0;
                                        font-size: 0.75rem;
                                        opacity: 0.7;
                                        font-family: monospace;
                                    ",
                                    "Type: {rate_type}"
                                }
                            }
                        }

                        // Dismiss button
                        button {
                            style: "
                                background: rgba(255, 255, 255, 0.2);
                                color: white;
                                border: 1px solid rgba(255, 255, 255, 0.3);
                                border-radius: 0.375rem;
                                padding: 0.5rem 1rem;
                                cursor: pointer;
                                font-size: 0.875rem;
                                font-weight: 500;
                                transition: background-color 0.2s;
                                hover:background-color: rgba(255, 255, 255, 0.3);
                            ",
                            onclick: move |_| {
                                state.with_mut(|s| s.scrubber_state.rate_limit_state = None);
                            },
                            "Dismiss"
                        }
                    }
                }

                // Add keyframe animations via CSS
                style {
                    r#"
                    @keyframes pulse {{
                        0%, 100% {{ transform: scale(1); }}
                        50% {{ transform: scale(1.02); }}
                    }}
                    
                    @keyframes bounce {{
                        0%, 20%, 50%, 80%, 100% {{ transform: translateY(0); }}
                        40% {{ transform: translateY(-5px); }}
                        60% {{ transform: translateY(-3px); }}
                    }}
                    "#
                }
            }
        } else {
            rsx! { div { style: "display: none;" } }
        }
    } else {
        rsx! { div { style: "display: none;" } }
    }
}
