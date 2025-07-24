use crate::types::AppState;
use dioxus::prelude::*;

#[component]
pub fn SettingsPage(mut state: Signal<AppState>) -> Element {
    rsx! {
        div {
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 2rem; margin-bottom: 1.5rem;",

            h2 {
                style: "font-size: 1.5rem; font-weight: bold; margin-bottom: 1.5rem; color: #333;",
                "Settings"
            }

            div {
                style: "display: flex; flex-direction: column; gap: 2rem;",

                // LLM Configuration Section
                div {
                    h3 {
                        style: "font-size: 1.25rem; font-weight: 600; margin-bottom: 1rem; color: #374151;",
                        "LLM Configuration"
                    }

                    div {
                        style: "display: flex; flex-direction: column; gap: 1rem; max-width: 500px;",

                        // API Key
                        div {
                            label {
                                style: "display: block; font-weight: 500; color: #374151; margin-bottom: 0.5rem;",
                                "OpenAI API Key"
                            }
                            input {
                                r#type: "password",
                                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem;",
                                placeholder: "sk-...",
                                value: state.read().llm_settings.api_key.clone(),
                                oninput: move |e| {
                                    state.with_mut(|s| {
                                        s.llm_settings.api_key = e.value();
                                    });
                                }
                            }
                        }

                        // Model Selection
                        div {
                            label {
                                style: "display: block; font-weight: 500; color: #374151; margin-bottom: 0.5rem;",
                                "Model"
                            }
                            select {
                                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem;",
                                value: state.read().llm_settings.model.clone(),
                                onchange: move |e| {
                                    state.with_mut(|s| {
                                        s.llm_settings.model = e.value();
                                    });
                                },
                                option { value: "gpt-4o", "GPT-4o" }
                                option { value: "gpt-4o-mini", "GPT-4o Mini" }
                                option { value: "gpt-4-turbo", "GPT-4 Turbo" }
                            }
                        }

                        // System Prompt (Optional)
                        div {
                            label {
                                style: "display: block; font-weight: 500; color: #374151; margin-bottom: 0.5rem;",
                                "Custom System Prompt (Optional)"
                            }
                            textarea {
                                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 0.875rem; min-height: 100px;",
                                placeholder: "Leave empty to use default prompt...",
                                value: state.read().llm_settings.system_prompt.as_deref().unwrap_or("").to_string(),
                                oninput: move |e| {
                                    state.with_mut(|s| {
                                        s.llm_settings.system_prompt = if e.value().trim().is_empty() {
                                            None
                                        } else {
                                            Some(e.value())
                                        };
                                    });
                                }
                            }
                        }

                        // Save Button
                        button {
                            style: "padding: 0.75rem 1.5rem; background: #2563eb; color: white; border: none; border-radius: 0.375rem; font-weight: 500; cursor: pointer; hover:background: #1d4ed8;",
                            onclick: move |_| {
                                // TODO: Save settings to storage
                            },
                            "Save Settings"
                        }
                    }
                }

                // Information Section
                div {
                    style: "padding: 1rem; background: #f3f4f6; border-radius: 0.375rem;",
                    h4 {
                        style: "font-weight: 600; color: #374151; margin-bottom: 0.5rem;",
                        "About LLM Rule Generation"
                    }
                    p {
                        style: "color: #6b7280; font-size: 0.875rem; line-height: 1.5;",
                        "Configure your OpenAI API key to enable AI-powered rewrite rule generation. "
                        "The LLM will analyze your tracks and suggest appropriate rules to clean up metadata issues."
                    }
                }
            }
        }
    }
}
