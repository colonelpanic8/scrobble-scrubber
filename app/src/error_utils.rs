use dioxus::prelude::*;

/// Helper trait to convert any error to ServerFnError with context
pub trait ToServerError<T> {
    #[allow(dead_code)] // Used in #[server] macro-generated code
    fn to_server_error(self, context: &str) -> Result<T, ServerFnError>;
}

impl<T, E: std::fmt::Display> ToServerError<T> for Result<T, E> {
    fn to_server_error(self, context: &str) -> Result<T, ServerFnError> {
        self.map_err(|e| ServerFnError::new(format!("{context}: {e}")))
    }
}

/// Helper for timeout operations that returns a more descriptive error
#[allow(dead_code)] // Used in #[server] macro-generated code
pub async fn with_timeout<F, T>(
    duration: std::time::Duration,
    future: F,
    operation_name: &str,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
where
    F: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(format!("Timeout during {operation_name}").into()),
    }
}

/// Creates a closure that handles async operations with error/success message management
#[allow(dead_code)] // Used in #[server] macro-generated code
pub fn create_async_handler<F, Fut>(
    mut error_signal: Signal<String>,
    mut success_message: Signal<String>,
    operation: F,
) -> impl Fn() + 'static
where
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + 'static,
{
    move || {
        let operation = operation.clone();
        spawn(async move {
            // Clear previous messages
            success_message.set(String::new());
            error_signal.set(String::new());

            match operation().await {
                Ok(msg) => success_message.set(msg),
                Err(e) => error_signal.set(e),
            }
        });
    }
}

/// Helper for storage operations - creates storage and loads config
#[allow(dead_code)] // Used in #[server] macro-generated code
pub async fn create_storage() -> Result<
    std::sync::Arc<tokio::sync::Mutex<scrobble_scrubber::persistence::FileStorage>>,
    ServerFnError,
> {
    use scrobble_scrubber::config::ScrobbleScrubberConfig;
    use scrobble_scrubber::persistence::FileStorage;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let config = ScrobbleScrubberConfig::load().to_server_error("Failed to load config")?;

    let storage = FileStorage::new(&config.storage.state_file)
        .to_server_error("Failed to initialize storage")?;

    Ok(Arc::new(Mutex::new(storage)))
}

/// Helper to deserialize session string
#[allow(dead_code)] // Used in #[server] macro-generated code
pub fn deserialize_session(
    session_str: &str,
) -> Result<lastfm_edit::LastFmEditSession, ServerFnError> {
    serde_json::from_str(session_str).to_server_error("Failed to deserialize session")
}

/// Helper to create LastFM client from session
#[allow(dead_code)] // Used in #[server] macro-generated code
pub fn create_client_from_session(
    session: lastfm_edit::LastFmEditSession,
) -> lastfm_edit::LastFmEditClient {
    let http_client = http_client::native::NativeClient::new();
    lastfm_edit::LastFmEditClient::from_session(Box::new(http_client), session)
}

/// Helper to find and remove an edit by ID
#[allow(dead_code)] // Used in #[server] macro-generated code
pub async fn remove_pending_edit(
    storage: &std::sync::Arc<tokio::sync::Mutex<scrobble_scrubber::persistence::FileStorage>>,
    edit_id: &str,
) -> Result<scrobble_scrubber::persistence::PendingEdit, ServerFnError> {
    use scrobble_scrubber::persistence::StateStorage;

    let mut pending_edits_state = storage
        .lock()
        .await
        .load_pending_edits_state()
        .await
        .to_server_error("Failed to load pending edits")?;

    let edit_index = pending_edits_state
        .pending_edits
        .iter()
        .position(|e| e.id == edit_id)
        .ok_or_else(|| ServerFnError::new("Edit not found"))?;

    let removed_edit = pending_edits_state.pending_edits.remove(edit_index);

    storage
        .lock()
        .await
        .save_pending_edits_state(&pending_edits_state)
        .await
        .to_server_error("Failed to save pending edits")?;

    Ok(removed_edit)
}

/// Helper to find and remove a rule by ID
#[allow(dead_code)] // Used in #[server] macro-generated code
pub async fn remove_pending_rule(
    storage: &std::sync::Arc<tokio::sync::Mutex<scrobble_scrubber::persistence::FileStorage>>,
    rule_id: &str,
) -> Result<scrobble_scrubber::persistence::PendingRewriteRule, ServerFnError> {
    use scrobble_scrubber::persistence::StateStorage;

    let mut pending_rules_state = storage
        .lock()
        .await
        .load_pending_rewrite_rules_state()
        .await
        .to_server_error("Failed to load pending rules")?;

    let rule_index = pending_rules_state
        .pending_rules
        .iter()
        .position(|r| r.id == rule_id)
        .ok_or_else(|| ServerFnError::new("Rule not found"))?;

    let removed_rule = pending_rules_state.pending_rules.remove(rule_index);

    storage
        .lock()
        .await
        .save_pending_rewrite_rules_state(&pending_rules_state)
        .await
        .to_server_error("Failed to save pending rules")?;

    Ok(removed_rule)
}

/// Helper to approve a rewrite rule (remove from pending and add to active)
#[allow(dead_code)] // Used in #[server] macro-generated code
pub async fn approve_rewrite_rule(
    storage: &std::sync::Arc<tokio::sync::Mutex<scrobble_scrubber::persistence::FileStorage>>,
    rule_id: &str,
) -> Result<(), ServerFnError> {
    use scrobble_scrubber::persistence::StateStorage;

    // Remove from pending rules
    let approved_rule = remove_pending_rule(storage, rule_id).await?;

    // Add to active rules
    let mut rewrite_rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .to_server_error("Failed to load rewrite rules")?;

    rewrite_rules_state.rewrite_rules.push(approved_rule.rule);

    storage
        .lock()
        .await
        .save_rewrite_rules_state(&rewrite_rules_state)
        .await
        .to_server_error("Failed to save rewrite rules")?;

    Ok(())
}
