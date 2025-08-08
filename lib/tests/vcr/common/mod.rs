#![allow(dead_code)]
use http_client_vcr::{CassetteFormat, FilterChain, NoOpClient, VcrClient, VcrMode};
use lastfm_edit::vcr_matcher::LastFmEditVcrMatcher;
use lastfm_edit::vcr_test_utils::create_lastfm_test_filter_chain;
use lastfm_edit::{LastFmEditClient, LastFmEditClientImpl};
use std::env;
use std::fs;

/// Shared setup for VCR test clients
struct VcrTestSetup {
    cassette_path: String,
    vcr_record: bool,
    mode: VcrMode,
}

impl VcrTestSetup {
    fn new(test_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let cassette_path = format!("tests/vcr/fixtures/{test_name}");

        let vcr_record_env = env::var("SCROBBLE_SCRUBBER_VCR_RECORD").unwrap_or_default();
        let vcr_record = !vcr_record_env.is_empty();

        let mode = match vcr_record_env.as_str() {
            "filter" => VcrMode::Filter,
            "" => VcrMode::Replay,
            _ => VcrMode::Record,
        };

        // Ensure fixtures directory exists
        if let Some(parent_dir) = std::path::Path::new(&cassette_path).parent() {
            fs::create_dir_all(parent_dir)?;
        }

        // Only create cassette directory if NOT in record mode
        // Record mode should start with no existing cassette
        let cassette_exists = std::path::Path::new(&cassette_path).exists();

        // Fail fast if we're not recording/filtering and no cassette exists
        if !vcr_record && !cassette_exists {
            return Err(format!(
                "No cassette found at '{cassette_path}' and SCROBBLE_SCRUBBER_VCR_RECORD is not set. Either set SCROBBLE_SCRUBBER_VCR_RECORD to record new interactions or ensure the cassette file exists."
            ).into());
        }

        Ok(Self {
            cassette_path,
            vcr_record,
            mode,
        })
    }

    fn get_credentials(&self) -> (String, String) {
        match self.mode {
            VcrMode::Record => {
                // Recording mode: need real credentials
                let username = env::var("SCROBBLE_SCRUBBER_LASTFM_USERNAME").expect(
                    "SCROBBLE_SCRUBBER_LASTFM_USERNAME required when SCROBBLE_SCRUBBER_VCR_RECORD is set to record",
                );
                let password = env::var("SCROBBLE_SCRUBBER_LASTFM_PASSWORD").expect(
                    "SCROBBLE_SCRUBBER_LASTFM_PASSWORD required when SCROBBLE_SCRUBBER_VCR_RECORD is set to record",
                );
                (username, password)
            }
            _ => {
                // Replay/Filter mode: use real username for URL matching, dummy password
                let username =
                    env::var("LASTFM_EDIT_USERNAME").unwrap_or_else(|_| "TestUser".to_string());
                (username, "dummy_password".to_string())
            }
        }
    }

    async fn create_vcr_client(&self) -> Result<VcrClient, Box<dyn std::error::Error>> {
        // Handle Filter mode by applying filters and saving the cassette
        if matches!(self.mode, VcrMode::Filter) {
            log::debug!("Filter mode: applying filters to existing cassette");
            let filter_chain = create_lastfm_test_filter_chain()?;
            http_client_vcr::filter_cassette_file(&self.cassette_path, filter_chain).await?;
            log::debug!("Filters applied and cassette saved");

            // After filtering, switch to Replay mode for the actual test
            let inner_client = Box::new(NoOpClient::new());
            let vcr_client = VcrClient::builder(&self.cassette_path)
                .inner_client(inner_client)
                .mode(VcrMode::Replay)
                .matcher(Box::new(LastFmEditVcrMatcher::new()))
                .format(CassetteFormat::Directory)
                .build()
                .await?;
            return Ok(vcr_client);
        }

        let inner_client: Box<dyn http_client::HttpClient + Send + Sync> = match self.mode {
            VcrMode::Record => Box::new(http_client::native::NativeClient::new()),
            _ => Box::new(NoOpClient::new()),
        };

        let mut builder = VcrClient::builder(&self.cassette_path)
            .inner_client(inner_client)
            .mode(self.mode.clone())
            .matcher(Box::new(LastFmEditVcrMatcher::new()));

        // Add filter chain for Record mode only (Filter mode is handled above)
        if matches!(self.mode, VcrMode::Record) {
            let filter_chain = create_lastfm_test_filter_chain()?;
            builder = builder.filter_chain(filter_chain);
        }

        let vcr_client = builder.build().await?;
        log::debug!("VCR client created successfully");

        Ok(vcr_client)
    }
}

/// Helper for creating Last.fm VCR test clients that EXCLUDES login from VCR recording
/// Login happens outside VCR, only feature interactions are recorded
/// This is the default behavior for most tests
pub async fn create_lastfm_vcr_test_client(
    test_name: &str,
) -> Result<Box<dyn LastFmEditClient + Send + Sync>, Box<dyn std::error::Error>> {
    let setup = VcrTestSetup::new(test_name)?;

    if matches!(setup.mode, VcrMode::Record) {
        // Recording mode: do real login outside VCR, then create VCR client with session
        let (username, password) = setup.get_credentials();

        // Do login outside VCR to get session - use default config for recording
        let login_client = Box::new(http_client::native::NativeClient::new());
        let config = lastfm_edit::ClientConfig::default(); // Real rate limiting for recording
        let logged_in_client = LastFmEditClientImpl::login_with_credentials_and_client_config(
            login_client,
            &username,
            &password,
            config,
        )
        .await
        .map_err(|e| format!("Login failed: {e}"))?;

        // Extract session from the logged-in client
        let session = logged_in_client.get_session().clone();

        // Now create VCR client for actual test interactions (using special method for recording)
        let mut builder = VcrClient::builder(&setup.cassette_path)
            .inner_client(Box::new(http_client::native::NativeClient::new()))
            .mode(setup.mode.clone())
            .matcher(Box::new(LastFmEditVcrMatcher::new()))
            .format(http_client_vcr::CassetteFormat::Directory);

        // Add filter chain for recording mode
        let filter_chain = create_lastfm_test_filter_chain()?;
        builder = builder.filter_chain(filter_chain);

        let vcr_client = builder.build().await?;

        // Create client with existing session and VCR http client - use default config for recording
        let config = lastfm_edit::ClientConfig::default(); // Real rate limiting for recording
        let client = LastFmEditClientImpl::from_session_with_client_config(
            Box::new(vcr_client),
            session,
            config,
        );

        Ok(Box::new(client))
    } else {
        // Replay/Filter mode: create dummy session and VCR client that will replay/filter interactions
        let vcr_client = setup.create_vcr_client().await?;

        // Create a session for replay/filter mode using the actual username from environment
        // This ensures the VCR requests match the recorded username
        let (username, _) = setup.get_credentials();
        let session = lastfm_edit::LastFmEditSession::new(
            username,
            vec!["dummy_cookie".to_string()],
            Some("dummy_csrf".to_string()),
            "https://www.last.fm".to_string(),
        );

        // Create client with testing config for replay/filter mode (rate limit detection enabled, no delays)
        let config = lastfm_edit::ClientConfig::for_testing();
        let client = LastFmEditClientImpl::from_session_with_client_config(
            Box::new(vcr_client),
            session,
            config,
        );
        Ok(Box::new(client))
    }
}

pub async fn create_vcr_client(
    cassette_path: &str,
    mode: VcrMode,
    filter_chain: Option<FilterChain>,
) -> Result<VcrClient, Box<dyn std::error::Error>> {
    // Ensure fixtures directory exists
    if let Some(parent_dir) = std::path::Path::new(cassette_path).parent() {
        fs::create_dir_all(parent_dir)?;
    }

    // Use NoOpClient when not in Record mode to prevent real HTTP requests
    let inner_client: Box<dyn http_client::HttpClient + Send + Sync> = match mode {
        VcrMode::Record => Box::new(http_client::native::NativeClient::new()),
        _ => Box::new(NoOpClient::new()),
    };
    let mut builder = VcrClient::builder(cassette_path)
        .inner_client(inner_client)
        .mode(mode)
        .format(http_client_vcr::CassetteFormat::Directory);

    if let Some(filters) = filter_chain {
        builder = builder.filter_chain(filters);
    }

    Ok(builder.build().await?)
}
