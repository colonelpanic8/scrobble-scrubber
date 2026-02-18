use crate::config::ScrobbleScrubberConfig;
use crate::session_manager::SessionManager;
use lastfm_edit::{
    ClientConfig, LastFmEditClientImpl, LastFmError, OperationalDelayConfig, Result, RetryConfig,
};

/// Create an authenticated Last.fm client, using saved session if available
pub async fn create_authenticated_client(
    config: &ScrobbleScrubberConfig,
) -> Result<LastFmEditClientImpl> {
    let session_manager = SessionManager::new(&config.lastfm.username);

    // Try to restore an existing session first
    if let Some(persisted_session) = session_manager.try_restore_session().await {
        log::info!(
            "Using existing session for user: {}",
            persisted_session.username
        );
        let http_client = http_client::native::NativeClient::new();
        let client_config = ClientConfig::default()
            .with_retry_config(RetryConfig::unbounded())
            .with_operational_delays(OperationalDelayConfig::default().with_get_delay_ms(1200));
        return Ok(LastFmEditClientImpl::from_session_with_client_config(
            Box::new(http_client),
            persisted_session,
            client_config,
        ));
    }

    // No valid session found, need to login with credentials
    log::info!("No valid session found, logging in to Last.fm...");

    if config.lastfm.username.is_empty() || config.lastfm.password.is_empty() {
        return Err(LastFmError::Io(std::io::Error::other(
            "Username and password are required for login. Please check your configuration.",
        )));
    }

    // Create new session and save it
    match session_manager
        .create_and_save_session(&config.lastfm.username, &config.lastfm.password)
        .await
    {
        Ok(persisted_session) => {
            log::info!("Successfully logged in and saved session for future use");
            let http_client = http_client::native::NativeClient::new();
            let client_config = ClientConfig::default()
                .with_retry_config(RetryConfig::unbounded())
                .with_operational_delays(OperationalDelayConfig::default().with_get_delay_ms(1200));
            Ok(LastFmEditClientImpl::from_session_with_client_config(
                Box::new(http_client),
                persisted_session,
                client_config,
            ))
        }
        Err(e) => {
            log::error!("Login failed: {e}");
            Err(e)
        }
    }
}
