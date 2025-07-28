use lastfm_edit::{LastFmEditClientImpl, LastFmError, Result};
use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::session_manager::SessionManager;
use std::io::{self, Write};

/// Create an authenticated Last.fm client with session management
pub async fn create_authenticated_client(
    config: &ScrobbleScrubberConfig,
) -> Result<LastFmEditClientImpl> {
    let session_manager = SessionManager::new(&config.lastfm.username);

    // Try to load existing session first
    let client = if let Some(session) = session_manager.load_session()? {
        println!("🔐 Using saved session for user '{}'", config.lastfm.username);
        LastFmEditClientImpl::from_session(Box::new(reqwest::Client::new()), session)
    } else {
        println!("🔑 No saved session found, performing authentication...");

        // Check if we have credentials
        if config.lastfm.username.is_empty() {
            return Err(LastFmError::Io(std::io::Error::other(
                "Username not configured. Set via config file or --lastfm-username",
            )));
        }

        let password = if config.lastfm.password.is_empty() {
            // Interactive password prompt
            print!("Enter Last.fm password for '{}': ", config.lastfm.username);
            io::stdout().flush().unwrap();
            
            rpassword::read_password().map_err(|e| {
                LastFmError::Io(std::io::Error::other(format!("Failed to read password: {e}")))
            })?
        } else {
            config.lastfm.password.clone()
        };

        println!("🔐 Authenticating with Last.fm...");
        let client = LastFmEditClientImpl::authenticate(&config.lastfm.username, &password).await?;

        // Save session for future use
        if let Err(e) = session_manager.save_session(client.get_session()) {
            log::warn!("Failed to save session key: {e}");
            println!("⚠️ Warning: Could not save session key, will need to re-authenticate next time");
        } else {
            println!("✅ Session saved for future use");
        }

        client
    };

    Ok(client)
}