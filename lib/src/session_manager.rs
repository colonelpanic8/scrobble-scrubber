use lastfm_edit::{
    LastFmEditClientImpl, LastFmEditSession, Result as LastFmResult,
    SessionManager as LastFmSessionManager,
};

/// Manages session persistence and validation using lastfm-edit's SessionManager
pub struct SessionManager {
    inner: LastFmSessionManager,
    username: String,
}

impl SessionManager {
    /// Create a new session manager for the given username
    pub fn new(username: &str) -> Self {
        let inner = LastFmSessionManager::new("scrobble-scrubber");
        Self {
            inner,
            username: username.to_string(),
        }
    }

    /// Try to load an existing session from disk
    pub fn load_session(&self) -> Option<LastFmEditSession> {
        match self.inner.load_session(&self.username) {
            Ok(session) => {
                log::info!("Loaded existing session for user: {}", session.username);
                Some(session)
            }
            Err(e) => {
                log::debug!("No existing session found: {e}");
                None
            }
        }
    }

    /// Save a session to disk
    pub fn save_session(&self, session: &LastFmEditSession) -> Result<(), std::io::Error> {
        match self.inner.save_session(session) {
            Ok(()) => {
                log::info!("Session saved for user: {}", session.username);
                Ok(())
            }
            Err(e) => {
                log::warn!("Failed to save session: {e}");
                Err(std::io::Error::other(format!(
                    "Failed to save session: {e}"
                )))
            }
        }
    }

    /// Validate if a session is still working by making a test request to settings page
    pub async fn validate_session(&self, session: &LastFmEditSession) -> bool {
        log::debug!("Validating existing session...");

        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.test_session_validity(session),
        )
        .await
        {
            Ok(true) => {
                log::info!("Session validation successful");
                true
            }
            Ok(false) => {
                log::warn!("Session validation failed - redirected to login");
                false
            }
            Err(_) => {
                log::warn!("Session validation timed out");
                false
            }
        }
    }

    /// Test session validity by making a request to a protected settings page
    async fn test_session_validity(&self, session: &LastFmEditSession) -> bool {
        use reqwest::Client;

        let client = Client::new();
        let test_url = "https://www.last.fm/settings/subscription/automatic-edits/tracks";

        // Build cookie header from session data
        let cookie_header = session.cookies.join("; ");

        let mut request_builder = client
            .get(test_url)
            .header("User-Agent", "Mozilla/5.0 (compatible; scrobble-scrubber)")
            .header("Cookie", &cookie_header);

        // Add CSRF token if available
        if let Some(csrf_token) = &session.csrf_token {
            request_builder = request_builder.header("X-CSRFToken", csrf_token);
        }

        match request_builder.send().await {
            Ok(response) => {
                let final_url = response.url().to_string();
                log::debug!("Session validation response URL: {final_url}");

                // If we're redirected to login, the session is invalid
                // If we stay on the settings page, the session is valid
                !final_url.contains("/login")
            }
            Err(e) => {
                log::warn!("Session validation request failed: {e}");
                false
            }
        }
    }

    /// Remove the saved session file
    pub fn clear_session(&self) -> Result<(), std::io::Error> {
        match self.inner.remove_session(&self.username) {
            Ok(()) => {
                log::info!("Cleared session for user: {}", self.username);
                Ok(())
            }
            Err(e) => {
                log::warn!("Failed to clear session: {e}");
                Err(std::io::Error::other(format!(
                    "Failed to clear session: {e}"
                )))
            }
        }
    }

    /// Try to restore a working session, validating it if necessary
    pub async fn try_restore_session(&self) -> Option<LastFmEditSession> {
        let session = self.load_session()?;

        // Always validate the session since lastfm-edit's SessionManager doesn't track staleness
        log::info!("Validating loaded session...");

        if self.validate_session(&session).await {
            // Session is still valid, re-save it to update any metadata
            if let Err(e) = self.save_session(&session) {
                log::warn!("Failed to update session: {e}");
            }
            Some(session)
        } else {
            // Session is invalid, clear it
            log::warn!("Session validation failed, clearing stored session");
            let _ = self.clear_session();
            None
        }
    }

    /// Create and save a new session after successful login
    pub async fn create_and_save_session(
        &self,
        username: &str,
        password: &str,
    ) -> LastFmResult<LastFmEditSession> {
        log::info!("Creating new Last.fm session...");

        let http_client = http_client::native::NativeClient::new();
        let client =
            LastFmEditClientImpl::login_with_credentials(Box::new(http_client), username, password)
                .await?;

        let session = client.get_session();

        // Save the session
        if let Err(e) = self.save_session(&session) {
            log::warn!("Failed to save session: {e}");
        }

        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_manager_creation() {
        let manager = SessionManager::new("testuser");
        assert_eq!(manager.username, "testuser");
    }
}
