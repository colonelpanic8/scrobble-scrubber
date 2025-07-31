use crate::types::AppState;
use dioxus::prelude::*;
use scrobble_scrubber::config::{
    JsonLoggingConfig, LastFmConfig, MusicBrainzProviderConfig, OpenAIProviderConfig,
    ProvidersConfig, ScrobbleScrubberConfig, ScrubberConfig, StorageConfig, TrackProviderType,
};

#[component]
pub fn ConfigPage(state: Signal<AppState>) -> Element {
    let config = state.read().config.clone().unwrap_or_default();

    let scrubber_config = use_signal(|| config.scrubber.clone());
    let providers_config = use_signal(|| config.providers.clone());
    let storage_config = use_signal(|| config.storage.clone());
    let lastfm_config = use_signal(|| config.lastfm.clone());

    let mut save_status = use_signal(|| None::<String>);

    let save_config = move |_| {
        spawn(async move {
            let new_config = ScrobbleScrubberConfig {
                scrubber: scrubber_config.read().clone(),
                providers: providers_config.read().clone(),
                storage: storage_config.read().clone(),
                lastfm: lastfm_config.read().clone(),
            };

            match save_config_to_file(&new_config).await {
                Ok(_) => {
                    // Check if we need to restart the scrubber due to configuration changes
                    let needs_scrubber_restart =
                        check_if_scrubber_restart_needed(&state.read().config, &new_config);

                    // Update the app state with new config
                    state.with_mut(|s| s.config = Some(new_config));

                    if needs_scrubber_restart {
                        handle_scrubber_restart_after_config_change(state).await;
                        save_status.set(Some("Configuration saved successfully! Scrubber will restart with new settings.".to_string()));
                    } else {
                        save_status.set(Some("Configuration saved successfully!".to_string()));
                    }
                }
                Err(e) => {
                    save_status.set(Some(format!("Failed to save configuration: {e}")));
                }
            }
        });
    };

    rsx! {
        div {
            style: "background: white; border-radius: 0.5rem; box-shadow: 0 4px 6px rgba(0,0,0,0.1); padding: 2rem; margin-bottom: 1.5rem;",
            h1 {
                style: "font-size: 2rem; font-weight: bold; margin-bottom: 1.5rem; color: #1f2937;",
                "Configuration"
            }

            if let Some(status) = save_status.read().as_ref() {
                div {
                    style: format!(
                        "padding: 1rem; border-radius: 0.5rem; margin-bottom: 1.5rem; {}",
                        if status.contains("successfully") {
                            "background-color: #d1fae5; color: #065f46; border: 1px solid #10b981;"
                        } else {
                            "background-color: #fee2e2; color: #991b1b; border: 1px solid #ef4444;"
                        }
                    ),
                    {status.clone()}
                }
            }

            form {
                onsubmit: move |e| {
                    e.prevent_default();
                    save_config(());
                },

                // Scrubber Configuration Section
                ScrubberConfigSection { config: scrubber_config }

                // Providers Configuration Section
                ProvidersConfigSection { config: providers_config }

                // Storage Configuration Section
                StorageConfigSection { config: storage_config }

                // Last.fm Configuration Section
                LastFmConfigSection { config: lastfm_config }

                // Save Button
                div {
                    style: "margin-top: 2rem; padding-top: 2rem; border-top: 1px solid #e5e7eb;",
                    button {
                        r#type: "submit",
                        style: "background-color: #2563eb; color: white; padding: 0.75rem 2rem; border: none; border-radius: 0.5rem; font-weight: 500; cursor: pointer; transition: background-color 0.2s;",
                        onmouseenter: move |_| {},
                        onmouseleave: move |_| {},
                        "Save Configuration"
                    }
                }
            }
        }
    }
}

#[component]
fn ScrubberConfigSection(config: Signal<ScrubberConfig>) -> Element {
    rsx! {
        ConfigSection { title: "Scrubber Settings",
            NumberInput {
                label: "Check Interval (seconds)",
                value: config.read().interval,
                onchange: move |value| config.with_mut(|c| c.interval = value),
                help: "How often to check for new tracks to process"
            }

            CheckboxInput {
                label: "Dry Run Mode",
                checked: config.read().dry_run,
                onchange: move |checked| config.with_mut(|c| c.dry_run = checked),
                help: "Preview changes without actually making edits"
            }

            CheckboxInput {
                label: "Require Confirmation for Edits",
                checked: config.read().require_confirmation,
                onchange: move |checked| config.with_mut(|c| c.require_confirmation = checked),
                help: "Require manual confirmation before applying edits (deprecated)"
            }

            CheckboxInput {
                label: "Require Confirmation for Proposed Rules",
                checked: config.read().require_proposed_rule_confirmation,
                onchange: move |checked| config.with_mut(|c| c.require_proposed_rule_confirmation = checked),
                help: "Require manual confirmation before applying proposed rewrite rules (deprecated)"
            }

            CheckboxInput {
                label: "Auto-start Scrubber",
                checked: config.read().auto_start,
                onchange: move |checked| config.with_mut(|c| c.auto_start = checked),
                help: "Automatically start the scrubber when the application launches"
            }

            TrackProviderSelect {
                value: config.read().track_provider.clone(),
                onchange: move |value| config.with_mut(|c| c.track_provider = value),
            }

            JsonLoggingSection { config: config.read().json_logging.clone(), onchange: move |new_config| config.with_mut(|c| c.json_logging = new_config) }
        }
    }
}

#[component]
fn ProvidersConfigSection(config: Signal<ProvidersConfig>) -> Element {
    rsx! {
        ConfigSection { title: "Providers Settings",
            CheckboxInput {
                label: "Enable Rewrite Rules Provider",
                checked: config.read().enable_rewrite_rules,
                onchange: move |checked| config.with_mut(|c| c.enable_rewrite_rules = checked),
                help: "Enable automated rewrite rules for track metadata cleanup"
            }

            CheckboxInput {
                label: "Enable OpenAI Provider",
                checked: config.read().enable_openai,
                onchange: move |checked| config.with_mut(|c| c.enable_openai = checked),
                help: "Enable AI-powered metadata suggestions using OpenAI"
            }

            if config.read().enable_openai {
                OpenAIConfigSection {
                    config: config.read().openai.clone().unwrap_or_else(|| OpenAIProviderConfig {
                        api_key: String::new(),
                        model: None,
                        system_prompt: None,
                    }),
                    onchange: move |new_config| config.with_mut(|c| c.openai = Some(new_config))
                }
            }

            CheckboxInput {
                label: "Enable MusicBrainz Provider",
                checked: config.read().enable_musicbrainz,
                onchange: move |checked| config.with_mut(|c| c.enable_musicbrainz = checked),
                help: "Enable MusicBrainz database lookup for metadata corrections"
            }

            if config.read().enable_musicbrainz {
                MusicBrainzConfigSection {
                    config: config.read().musicbrainz.clone().unwrap_or_else(MusicBrainzProviderConfig::default),
                    onchange: move |new_config| config.with_mut(|c| c.musicbrainz = Some(new_config))
                }
            }
        }
    }
}

#[component]
fn StorageConfigSection(config: Signal<StorageConfig>) -> Element {
    rsx! {
        ConfigSection { title: "Storage Settings",
            TextInput {
                label: "State File Path",
                value: config.read().state_file.clone(),
                onchange: move |value| config.with_mut(|c| c.state_file = value),
                help: "Path to the file where application state is stored"
            }
        }
    }
}

#[component]
fn LastFmConfigSection(config: Signal<LastFmConfig>) -> Element {
    rsx! {
        ConfigSection { title: "Last.fm Settings",
            TextInput {
                label: "Username",
                value: config.read().username.clone(),
                onchange: move |value| config.with_mut(|c| c.username = value),
                help: "Your Last.fm username"
            }

            PasswordInput {
                label: "Password",
                value: config.read().password.clone(),
                onchange: move |value| config.with_mut(|c| c.password = value),
                help: "Your Last.fm password"
            }

            TextInput {
                label: "Base URL (optional)",
                value: config.read().base_url.clone().unwrap_or_default(),
                onchange: move |value: String| config.with_mut(|c| c.base_url = if value.is_empty() { None } else { Some(value) }),
                help: "Custom Last.fm base URL (defaults to https://www.last.fm)"
            }
        }
    }
}

#[derive(Props, Clone)]
struct JsonLoggingSectionProps {
    config: JsonLoggingConfig,
    onchange: EventHandler<JsonLoggingConfig>,
}

impl PartialEq for JsonLoggingSectionProps {
    fn eq(&self, other: &Self) -> bool {
        self.config.enabled == other.config.enabled && self.config.log_file == other.config.log_file
    }
}

#[component]
fn JsonLoggingSection(props: JsonLoggingSectionProps) -> Element {
    let JsonLoggingSectionProps { config, onchange } = props;
    let mut local_config = use_signal(|| config.clone());

    use_effect(move || {
        onchange.call(local_config.read().clone());
    });

    rsx! {
        div {
            style: "margin-top: 1rem; padding: 1rem; background-color: #f9fafb; border-radius: 0.5rem;",
            h4 {
                style: "font-weight: 600; margin-bottom: 1rem; color: #374151;",
                "JSON Logging"
            }

            CheckboxInput {
                label: "Enable JSON Logging",
                checked: local_config.read().enabled,
                onchange: move |checked| local_config.with_mut(|c| c.enabled = checked),
                help: "Log track edit events to a JSON file"
            }

            TextInput {
                label: "Log File Path (optional)",
                value: local_config.read().log_file.clone().unwrap_or_default(),
                onchange: move |value: String| local_config.with_mut(|c| c.log_file = if value.is_empty() { None } else { Some(value) }),
                help: "Custom path for JSON log file (defaults to XDG data directory)"
            }
        }
    }
}

#[derive(Props, Clone)]
struct OpenAIConfigSectionProps {
    config: OpenAIProviderConfig,
    onchange: EventHandler<OpenAIProviderConfig>,
}

impl PartialEq for OpenAIConfigSectionProps {
    fn eq(&self, other: &Self) -> bool {
        self.config.api_key == other.config.api_key
            && self.config.model == other.config.model
            && self.config.system_prompt == other.config.system_prompt
    }
}

#[component]
fn OpenAIConfigSection(props: OpenAIConfigSectionProps) -> Element {
    let OpenAIConfigSectionProps { config, onchange } = props;
    let mut local_config = use_signal(|| config.clone());

    use_effect(move || {
        onchange.call(local_config.read().clone());
    });

    rsx! {
        div {
            style: "margin-top: 1rem; padding: 1rem; background-color: #f9fafb; border-radius: 0.5rem;",
            h4 {
                style: "font-weight: 600; margin-bottom: 1rem; color: #374151;",
                "OpenAI Configuration"
            }

            PasswordInput {
                label: "API Key",
                value: local_config.read().api_key.clone(),
                onchange: move |value| local_config.with_mut(|c| c.api_key = value),
                help: "Your OpenAI API key"
            }

            TextInput {
                label: "Model (optional)",
                value: local_config.read().model.clone().unwrap_or_default(),
                onchange: move |value: String| local_config.with_mut(|c| c.model = if value.is_empty() { None } else { Some(value) }),
                help: "OpenAI model to use (defaults to gpt-4o-mini)"
            }

            TextAreaInput {
                label: "Custom System Prompt (optional)",
                value: local_config.read().system_prompt.clone().unwrap_or_default(),
                onchange: move |value: String| local_config.with_mut(|c| c.system_prompt = if value.is_empty() { None } else { Some(value) }),
                help: "Custom system prompt for the AI (uses default if empty)"
            }
        }
    }
}

#[derive(Props, Clone)]
struct MusicBrainzConfigSectionProps {
    config: MusicBrainzProviderConfig,
    onchange: EventHandler<MusicBrainzProviderConfig>,
}

impl PartialEq for MusicBrainzConfigSectionProps {
    fn eq(&self, other: &Self) -> bool {
        self.config.confidence_threshold == other.config.confidence_threshold
            && self.config.max_results == other.config.max_results
            && self.config.api_delay_ms == other.config.api_delay_ms
    }
}

#[component]
fn MusicBrainzConfigSection(props: MusicBrainzConfigSectionProps) -> Element {
    let MusicBrainzConfigSectionProps { config, onchange } = props;
    let mut local_config = use_signal(|| config.clone());

    use_effect(move || {
        onchange.call(local_config.read().clone());
    });

    rsx! {
        div {
            style: "margin-top: 1rem; padding: 1rem; background-color: #f9fafb; border-radius: 0.5rem;",
            h4 {
                style: "font-weight: 600; margin-bottom: 1rem; color: #374151;",
                "MusicBrainz Configuration"
            }

            NumberInput {
                label: "Confidence Threshold",
                value: (local_config.read().confidence_threshold * 100.0) as u64,
                onchange: move |value| local_config.with_mut(|c| c.confidence_threshold = (value as f32) / 100.0),
                help: "Minimum confidence percentage for accepting MusicBrainz matches (0-100)"
            }

            NumberInput {
                label: "Max Results",
                value: local_config.read().max_results as u64,
                onchange: move |value| local_config.with_mut(|c| c.max_results = value as usize),
                help: "Maximum number of search results to examine"
            }

            NumberInput {
                label: "API Delay (milliseconds)",
                value: local_config.read().api_delay_ms,
                onchange: move |value| local_config.with_mut(|c| c.api_delay_ms = value),
                help: "Delay between API requests to be respectful to MusicBrainz"
            }
        }
    }
}

// Helper Components
#[component]
fn ConfigSection(title: &'static str, children: Element) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 2rem; padding-bottom: 2rem; border-bottom: 1px solid #e5e7eb;",
            h2 {
                style: "font-size: 1.5rem; font-weight: 600; margin-bottom: 1.5rem; color: #1f2937;",
                {title}
            }
            {children}
        }
    }
}

#[component]
fn TextInput(
    label: &'static str,
    value: String,
    help: &'static str,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 1rem;",
            label {
                style: "display: block; font-weight: 500; margin-bottom: 0.5rem; color: #374151;",
                {label}
            }
            input {
                r#type: "text",
                value: "{value}",
                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 1rem;",
                oninput: move |e| onchange.call(e.value()),
            }
            if !help.is_empty() {
                p {
                    style: "margin-top: 0.25rem; font-size: 0.875rem; color: #6b7280;",
                    {help}
                }
            }
        }
    }
}

#[component]
fn PasswordInput(
    label: &'static str,
    value: String,
    help: &'static str,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 1rem;",
            label {
                style: "display: block; font-weight: 500; margin-bottom: 0.5rem; color: #374151;",
                {label}
            }
            input {
                r#type: "password",
                value: "{value}",
                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 1rem;",
                oninput: move |e| onchange.call(e.value()),
            }
            if !help.is_empty() {
                p {
                    style: "margin-top: 0.25rem; font-size: 0.875rem; color: #6b7280;",
                    {help}
                }
            }
        }
    }
}

#[component]
fn NumberInput(
    label: &'static str,
    value: u64,
    help: &'static str,
    onchange: EventHandler<u64>,
) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 1rem;",
            label {
                style: "display: block; font-weight: 500; margin-bottom: 0.5rem; color: #374151;",
                {label}
            }
            input {
                r#type: "number",
                value: "{value}",
                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 1rem;",
                oninput: move |e| {
                    if let Ok(val) = e.value().parse::<u64>() {
                        onchange.call(val);
                    }
                },
            }
            if !help.is_empty() {
                p {
                    style: "margin-top: 0.25rem; font-size: 0.875rem; color: #6b7280;",
                    {help}
                }
            }
        }
    }
}

#[component]
fn CheckboxInput(
    label: &'static str,
    checked: bool,
    help: &'static str,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 1rem;",
            label {
                style: "display: flex; items-center; cursor: pointer;",
                input {
                    r#type: "checkbox",
                    checked: "{checked}",
                    style: "margin-right: 0.75rem; width: 1rem; height: 1rem;",
                    onchange: move |e| onchange.call(e.checked()),
                }
                span {
                    style: "font-weight: 500; color: #374151;",
                    {label}
                }
            }
            if !help.is_empty() {
                p {
                    style: "margin-top: 0.25rem; margin-left: 1.75rem; font-size: 0.875rem; color: #6b7280;",
                    {help}
                }
            }
        }
    }
}

#[component]
fn TextAreaInput(
    label: &'static str,
    value: String,
    help: &'static str,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 1rem;",
            label {
                style: "display: block; font-weight: 500; margin-bottom: 0.5rem; color: #374151;",
                {label}
            }
            textarea {
                value: "{value}",
                rows: "4",
                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 1rem; resize: vertical;",
                oninput: move |e| onchange.call(e.value()),
            }
            if !help.is_empty() {
                p {
                    style: "margin-top: 0.25rem; font-size: 0.875rem; color: #6b7280;",
                    {help}
                }
            }
        }
    }
}

#[component]
fn TrackProviderSelect(
    value: TrackProviderType,
    onchange: EventHandler<TrackProviderType>,
) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 1rem;",
            label {
                style: "display: block; font-weight: 500; margin-bottom: 0.5rem; color: #374151;",
                "Track Provider"
            }
            select {
                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 1rem;",
                onchange: move |e| {
                    let new_value = match e.value().as_str() {
                        "cached" => TrackProviderType::Cached,
                        "direct" => TrackProviderType::Direct,
                        _ => TrackProviderType::Direct, // Default fallback
                    };
                    onchange.call(new_value);
                },
                option {
                    value: "direct",
                    selected: matches!(value, TrackProviderType::Direct),
                    "Direct API (no caching)"
                }
                option {
                    value: "cached",
                    selected: matches!(value, TrackProviderType::Cached),
                    "Cached (persistent storage)"
                }
            }
            p {
                style: "margin-top: 0.25rem; font-size: 0.875rem; color: #6b7280;",
                "How to fetch tracks: Direct queries Last.fm API each time, Cached stores tracks locally"
            }
        }
    }
}

#[component]
fn SelectInput(
    label: &'static str,
    value: String,
    options: Vec<(String, String)>,
    help: &'static str,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 1rem;",
            label {
                style: "display: block; font-weight: 500; margin-bottom: 0.5rem; color: #374151;",
                {label}
            }
            select {
                value: "{value}",
                style: "width: 100%; padding: 0.75rem; border: 1px solid #d1d5db; border-radius: 0.375rem; font-size: 1rem;",
                onchange: move |e| onchange.call(e.value()),
                for (option_value, option_label) in options {
                    option {
                        value: "{option_value}",
                        selected: option_value == value,
                        {option_label}
                    }
                }
            }
            if !help.is_empty() {
                p {
                    style: "margin-top: 0.25rem; font-size: 0.875rem; color: #6b7280;",
                    {help}
                }
            }
        }
    }
}

// Config change analysis and scrubber restart logic

fn check_if_scrubber_restart_needed(
    old_config: &Option<ScrobbleScrubberConfig>,
    new_config: &ScrobbleScrubberConfig,
) -> bool {
    let Some(old_config) = old_config else {
        // If there was no previous config, any scrubber instance needs to be recreated
        return true;
    };

    // Check for changes that require scrubber restart
    old_config.scrubber.interval != new_config.scrubber.interval
        || old_config.scrubber.dry_run != new_config.scrubber.dry_run
        || old_config.scrubber.track_provider != new_config.scrubber.track_provider
        || old_config.providers.enable_rewrite_rules != new_config.providers.enable_rewrite_rules
        || old_config.providers.enable_openai != new_config.providers.enable_openai
        || old_config.providers.enable_musicbrainz != new_config.providers.enable_musicbrainz
        || old_config.providers.openai != new_config.providers.openai
        || old_config.providers.musicbrainz != new_config.providers.musicbrainz
        || old_config.storage.state_file != new_config.storage.state_file
        || old_config.lastfm.username != new_config.lastfm.username
        || old_config.lastfm.password != new_config.lastfm.password
        || old_config.lastfm.base_url != new_config.lastfm.base_url
}

async fn handle_scrubber_restart_after_config_change(mut state: Signal<AppState>) {
    use crate::types::ScrubberStatus;

    let was_running = {
        let s = state.read();
        matches!(
            s.scrubber_state.status,
            ScrubberStatus::Running | ScrubberStatus::Sleeping { .. }
        )
    };

    if was_running {
        // Gracefully stop the running scrubber first
        state.with_mut(|s| s.scrubber_state.status = ScrubberStatus::Stopping);

        // Wait a moment for the running task to notice the status change and stop
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Clear the scrubber instance to force recreation with new config
    state.with_mut(|s| {
        s.scrubber_instance = None;
        s.scrubber_state.status = ScrubberStatus::Stopped;
        s.scrubber_state.event_sender = None;
    });

    // If it was running before, restart it with new configuration
    if was_running {
        // Import the start function from scrobble_scrubber component
        // We'll trigger a restart by setting a flag that the UI can detect
        state.with_mut(|s| {
            let restart_event = ::scrobble_scrubber::events::ScrubberEvent {
                timestamp: chrono::Utc::now(),
                event_type: ::scrobble_scrubber::events::ScrubberEventType::Info(
                    "Scrubber will restart with new configuration settings...".to_string(),
                ),
            };
            s.scrubber_state.events.push(restart_event);
        });
    }
}

// Config saving functionality
async fn save_config_to_file(config: &ScrobbleScrubberConfig) -> Result<(), String> {
    use scrobble_scrubber::config::ScrobbleScrubberConfig;
    use std::fs;

    let config_path = ScrobbleScrubberConfig::get_preferred_config_path()
        .ok_or("Could not determine config directory")?;

    // Create directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }

    let config_toml =
        toml::to_string_pretty(config).map_err(|e| format!("Failed to serialize config: {e}"))?;

    fs::write(&config_path, config_toml)
        .map_err(|e| format!("Failed to write config file: {e}"))?;

    Ok(())
}
