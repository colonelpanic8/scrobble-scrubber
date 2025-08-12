use crate::config::ScrobbleScrubberConfig;
use crate::session_manager::SessionManager;
use lastfm_edit::{LastFmEditClientImpl, LastFmError, Result};

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
        return Ok(LastFmEditClientImpl::from_session(
            Box::new(http_client),
            persisted_session,
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
            Ok(LastFmEditClientImpl::from_session(
                Box::new(http_client),
                persisted_session,
            ))
        }
        Err(e) => {
            log::error!("Login failed: {e}");
            Err(e)
        }
    }
}
