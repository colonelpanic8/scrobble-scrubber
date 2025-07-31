use crate::types::{AppState, GlobalScrubber};
use ::scrobble_scrubber::config::ScrobbleScrubberConfig;
use ::scrobble_scrubber::persistence::FileStorage;
use ::scrobble_scrubber::rewrite::RewriteRule;
use ::scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
use ::scrobble_scrubber::scrubber::ScrobbleScrubber;
use dioxus::prelude::*;
use lastfm_edit::{LastFmEditClientImpl, LastFmEditSession};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Creates a new scrubber instance with the given configuration
pub async fn create_scrubber_instance(
    session_json: String,
    storage: Arc<Mutex<FileStorage>>,
    saved_rules: Vec<RewriteRule>,
    config: ScrobbleScrubberConfig,
) -> Result<GlobalScrubber, Box<dyn std::error::Error + Send + Sync>> {
    // Deserialize the session
    let session: LastFmEditSession = serde_json::from_str(&session_json)?;

    // Create a client with the restored session
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClientImpl::from_session(Box::new(http_client), session);

    // Create action provider with current rules
    let action_provider = RewriteRulesScrubActionProvider::from_rules(saved_rules);

    // Create scrubber instance
    let scrubber = ScrobbleScrubber::new(
        storage.clone(),
        Box::new(client),
        action_provider,
        config.clone(),
    );

    // Start event logger for JSON logging of edit attempts
    {
        let event_receiver = scrubber.subscribe_events();
        let log_file_path = ::scrobble_scrubber::config::StorageConfig::get_edit_log_path(
            &config.storage.state_file,
        );
        let mut event_logger = ::scrobble_scrubber::event_logger::EventLogger::new(
            log_file_path.clone(),
            true,
            event_receiver,
            config.scrubber.clone(),
        );

        tokio::spawn(async move {
            // Log to console in web context if needed
            log::debug!("Started edit logging to: {log_file_path}");
            event_logger.run().await;
        });
    }

    Ok(scrubber)
}

/// Gets or creates the global scrubber instance, updating it if configuration changed
pub async fn get_or_create_scrubber(
    mut state: Signal<AppState>,
) -> Result<Arc<Mutex<GlobalScrubber>>, Box<dyn std::error::Error + Send + Sync>> {
    // First check if we need to recreate the scrubber (config/session changed)
    let needs_recreation = state.with(|s| {
        // Need recreation if no instance exists or if critical data is missing
        s.scrubber_instance.is_none()
            || s.session.is_none()
            || s.storage.is_none()
            || s.config.is_none()
    });

    if needs_recreation {
        let (session_json, storage, saved_rules, config) = state.with(|s| {
            (
                s.session.clone(),
                s.storage.clone(),
                s.saved_rules.clone(),
                s.config.clone(),
            )
        });

        let session_json = session_json.ok_or("No session available")?;
        let storage = storage.ok_or("No storage available")?;
        let config = config.ok_or("No config available")?;

        // Create new scrubber instance
        let scrubber = create_scrubber_instance(session_json, storage, saved_rules, config).await?;
        let scrubber_arc = Arc::new(Mutex::new(scrubber));

        // Store it in the app state
        state.with_mut(|s| {
            s.scrubber_instance = Some(scrubber_arc.clone());
        });

        Ok(scrubber_arc)
    } else {
        // Return existing instance
        state.with(|s| {
            s.scrubber_instance
                .clone()
                .ok_or_else(|| "Scrubber instance unexpectedly missing".into())
        })
    }
}

/// Updates the scrubber's rules without recreating the entire instance
#[allow(dead_code)]
pub async fn update_scrubber_rules(
    mut state: Signal<AppState>,
    new_rules: Vec<RewriteRule>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Update the saved rules in state first
    state.with_mut(|s| {
        s.saved_rules = new_rules.clone();
    });

    // Force recreation of the scrubber instance with new rules
    state.with_mut(|s| {
        s.scrubber_instance = None;
    });

    // This will create a new instance with the updated rules
    get_or_create_scrubber(state).await?;

    Ok(())
}

/// Clears the global scrubber instance (used when logging out or on errors)
#[allow(dead_code)]
pub fn clear_scrubber_instance(mut state: Signal<AppState>) {
    state.with_mut(|s| {
        s.scrubber_instance = None;
    });
}
