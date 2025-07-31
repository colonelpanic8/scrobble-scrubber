use crate::types::AppState;
use crate::utils::{import_default_rules, load_default_remaster_rules, DefaultRule};
use dioxus::prelude::*;
use std::collections::HashSet;

#[component]
pub fn DefaultRulesSection(mut state: Signal<AppState>) -> Element {
    let mut selected_rules = use_signal(HashSet::<usize>::new);
    let mut import_status = use_signal(|| None::<String>);
    let mut is_importing = use_signal(|| false);
    let mut show_section = use_signal(|| false);

    // Load default rules on mount using use_resource
    let default_rules = use_resource(move || async move {
        match load_default_remaster_rules() {
            Ok(rules) => {
                log::info!("Loaded {} default rules", rules.rules.len());
                Some(rules)
            }
            Err(e) => {
                log::error!("Failed to load default rules: {e}");
                None
            }
        }
    });

    rsx! {
        div {
            style: "border: 1px solid #e5e7eb; border-radius: 0.5rem; padding: 1rem; margin-bottom: 1rem;",

            // Header with toggle
            div {
                style: format!("display: flex; justify-content: space-between; align-items: center; margin-bottom: {};", if show_section() { "1rem" } else { "0" }),
                div {
                    h3 { style: "font-weight: 600; color: #374151; margin: 0;", "Default Remaster Cleanup Rules" }
                    p { style: "font-size: 0.875rem; color: #6b7280; margin: 0.25rem 0 0 0;",
                        "Curated rules to remove remaster information, based on 600+ real Last.fm tracks"
                    }
                }
                button {
                    style: "background: #3b82f6; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                    onclick: move |_| {
                        show_section.set(!show_section());
                    },
                    {if show_section() { "Hide Rules" } else { "Show Rules" }}
                }
            }

            // Import status message
            if let Some(message) = import_status.read().as_ref() {
                div {
                    style: format!("padding: 0.75rem; border-radius: 0.375rem; margin-bottom: 1rem; {}",
                        if message.contains("Successfully") {
                            "background: #dcfce7; color: #166534; border: 1px solid #bbf7d0;"
                        } else {
                            "background: #fef2f2; color: #dc2626; border: 1px solid #fecaca;"
                        }
                    ),
                    {message.clone()}
                }
            }

            // Expandable rules section
            if show_section() {
                {
                    match default_rules.read().as_ref() {
                        Some(Some(rule_set)) => {
                        let rule_count = rule_set.rules.len();
                        let rules_clone = rule_set.rules.clone();
                        rsx! {
                            div {
                        // Controls
                        div {
                            style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; padding: 1rem; background: #f9fafb; border-radius: 0.375rem;",
                            div {
                                style: "display: flex; gap: 1rem; align-items: center;",
                                button {
                                    style: "background: #6b7280; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        let all_indices: HashSet<usize> = (0..rule_count).collect();
                                        selected_rules.set(all_indices);
                                    },
                                    "Select All"
                                }
                                button {
                                    style: "background: #6b7280; color: white; padding: 0.5rem 1rem; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        selected_rules.set(HashSet::new());
                                    },
                                    "Clear Selection"
                                }
                                span {
                                    style: "font-size: 0.875rem; color: #6b7280;",
                                    { format!("{} rules selected", selected_rules.read().len()) }
                                }
                            }
                            button {
                                style: format!("background: {}; color: white; padding: 0.75rem 1.5rem; border: none; border-radius: 0.375rem; cursor: {}; font-weight: 600;",
                                    if selected_rules.read().is_empty() || is_importing() { "#9ca3af" } else { "#10b981" },
                                    if selected_rules.read().is_empty() || is_importing() { "not-allowed" } else { "pointer" }
                                ),
                                disabled: selected_rules.read().is_empty() || is_importing(),
                                onclick: {
                                    let rules_clone_for_import = rules_clone.clone();
                                    move |_| {
                                        let selected_indices = selected_rules.read().clone();
                                        let rules_to_import: Vec<DefaultRule> = selected_indices
                                            .iter()
                                            .filter_map(|&idx| rules_clone_for_import.get(idx))
                                            .cloned()
                                            .collect();


                                        is_importing.set(true);
                                        import_status.set(None);

                                        spawn(async move {
                                            match import_default_rules(state, rules_to_import).await {
                                                Ok(count) => {
                                                    import_status.set(Some(format!("Successfully imported {count} rules!")));
                                                    selected_rules.set(HashSet::new());
                                                }
                                                Err(e) => {
                                                    import_status.set(Some(format!("Failed to import rules: {e}")));
                                                }
                                            }
                                            is_importing.set(false);
                                        });
                                    }
                                },
                                if is_importing() { "Importing..." } else { "Import Selected Rules" }
                            }
                        }

                        // Rules list
                        div {
                            style: "max-height: 60vh; overflow-y: auto; border: 1px solid #e5e7eb; border-radius: 0.375rem;",
                            for (idx, rule) in rule_set.rules.iter().enumerate() {
                                div {
                                    key: "{idx}",
                                    style: "border-bottom: 1px solid #f3f4f6; last:border-bottom-0;",

                                    label {
                                        style: "display: block; padding: 1rem; cursor: pointer; hover:background: #f9fafb;",
                                        r#for: "rule-{idx}",

                                        div {
                                            style: "display: flex; align-items: start; gap: 0.75rem;",

                                            input {
                                                r#type: "checkbox",
                                                id: "rule-{idx}",
                                                checked: selected_rules.read().contains(&idx),
                                                onchange: move |e| {
                                                    let mut current = selected_rules.read().clone();
                                                    if e.checked() {
                                                        current.insert(idx);
                                                    } else {
                                                        current.remove(&idx);
                                                    }
                                                    selected_rules.set(current);
                                                }
                                            }

                                            div {
                                                style: "flex: 1;",

                                                div {
                                                    style: "display: flex; justify-content: space-between; align-items: start; margin-bottom: 0.5rem;",
                                                    h4 {
                                                        style: "font-weight: 600; color: #374151; margin: 0;",
                                                        {rule.name.clone()}
                                                    }
                                                }

                                                p {
                                                    style: "font-size: 0.875rem; color: #6b7280; margin-bottom: 0.75rem;",
                                                    {rule.description.clone()}
                                                }

                                                // Show pattern
                                                if let Some(track_pattern) = rule.track_name.as_ref() {
                                                    div {
                                                        style: "margin-bottom: 0.5rem;",
                                                        strong { style: "font-size: 0.875rem;", "Pattern: " }
                                                        code {
                                                            style: "background: #f3f4f6; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.75rem; font-family: monospace;",
                                                            "\"{track_pattern.find}\" → \"{track_pattern.replace}\""
                                                        }
                                                    }
                                                }

                                                // Show examples
                                                if !rule.examples.is_empty() {
                                                    div {
                                                        style: "font-size: 0.75rem; color: #6b7280;",
                                                        strong { "Examples: " }
                                                        for (ex_idx, example) in rule.examples.iter().take(2).enumerate() {
                                                            span {
                                                                key: "{ex_idx}",
                                                                style: "display: inline-block; margin-right: 0.5rem;",
                                                                {example.clone()}
                                                                if ex_idx < rule.examples.len().min(2) - 1 { " • " }
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
                        }
                        Some(None) => {
                            rsx! {
                                div {
                                    style: "text-center; color: #dc2626; padding: 2rem;",
                                    "Failed to load default rules"
                                }
                            }
                        }
                        None => {
                            rsx! {
                                div {
                                    style: "text-center; color: #6b7280; padding: 2rem;",
                                    "Loading default rules..."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
