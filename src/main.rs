use clap::Parser;
use lastfm_edit::{LastFmEditClient, LastFmError, Result};
use log::info;
use scrobble_scrubber::config::{OpenAIProviderConfig, ScrobbleScrubberConfig};
use scrobble_scrubber::openai_provider::OpenAIScrubActionProvider;
use scrobble_scrubber::persistence::{FileStorage, StateStorage};
use scrobble_scrubber::scrub_action_provider::{
    OrScrubActionProvider, RewriteRulesScrubActionProvider,
};
use scrobble_scrubber::scrubber::ScrobbleScrubber;
use scrobble_scrubber::web_interface;
use scrobble_scrubber::Args;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    let args = Args::parse();

    // Load configuration from args, env vars, and config files
    let config = ScrobbleScrubberConfig::load_from_args(&args).map_err(|e| {
        LastFmError::Io(std::io::Error::other(format!(
            "Failed to load configuration: {e}"
        )))
    })?;

    info!(
        "Starting scrobble-scrubber with interval {}s",
        config.scrubber.interval
    );

    // Create and login to LastFM client
    let http_client = http_client::native::NativeClient::new();
    let mut client = LastFmEditClient::new(Box::new(http_client));

    info!("Logging in to Last.fm...");
    client
        .login(&config.lastfm.username, &config.lastfm.password)
        .await?;
    info!("Successfully logged in to Last.fm");

    // Create storage wrapped in Arc<Mutex<>>
    info!("Using state file: {}", config.storage.state_file);
    let storage = Arc::new(Mutex::new(
        FileStorage::new(&config.storage.state_file).map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to create storage: {e}"
            )))
        })?,
    ));

    // Create action provider (for now, just rewrite rules)
    let rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load rewrite rules: {e}"
            )))
        })?;

    let mut action_provider = OrScrubActionProvider::new();

    if config.providers.enable_rewrite_rules {
        let rewrite_provider = RewriteRulesScrubActionProvider::new(&rules_state);
        action_provider = action_provider.add_provider(rewrite_provider);
    }

    // Add OpenAI provider if enabled and configured
    if config.providers.enable_openai {
        // Try to get OpenAI config, or create default from environment variables
        let openai_config_opt = if let Some(openai_config) = &config.providers.openai {
            Some(openai_config.clone())
        } else {
            // Check if API key is available from environment variables
            if let Ok(api_key) = std::env::var("SCROBBLE_SCRUBBER_OPENAI_API_KEY") {
                info!("Creating default OpenAI configuration from environment variable");
                Some(OpenAIProviderConfig {
                    api_key,
                    model: std::env::var("SCROBBLE_SCRUBBER_OPENAI_MODEL").ok(),
                    system_prompt: std::env::var("SCROBBLE_SCRUBBER_OPENAI_SYSTEM_PROMPT").ok(),
                })
            } else {
                log::warn!("OpenAI provider enabled but no API key found in configuration or SCROBBLE_SCRUBBER_OPENAI_API_KEY environment variable");
                None
            }
        };

        if let Some(openai_config) = openai_config_opt {
            match OpenAIScrubActionProvider::new(
                openai_config.api_key.clone(),
                openai_config.model.clone(),
                openai_config.system_prompt.clone(),
                rules_state.rewrite_rules.clone(),
            ) {
                Ok(openai_provider) => {
                    info!(
                        "Enabled OpenAI provider with model: {}",
                        openai_config.model.as_deref().unwrap_or("default")
                    );
                    action_provider = action_provider.add_provider(openai_provider);
                }
                Err(e) => {
                    log::warn!("Failed to create OpenAI provider: {e}");
                }
            }
        }
    }

    // Create scrubber wrapped in Arc<Mutex<>>
    let scrubber = Arc::new(Mutex::new(ScrobbleScrubber::new(
        storage.clone(),
        client,
        action_provider,
        config.clone(),
    )));

    // Start web interface if enabled
    if config.scrubber.enable_web_interface {
        let web_storage = storage.clone();
        let web_scrubber = scrubber.clone();
        let web_port = config.scrubber.web_port;

        tokio::spawn(async move {
            if let Err(e) =
                web_interface::start_web_server(web_storage, web_scrubber, web_port).await
            {
                log::error!("Web interface error: {e}");
            }
        });

        info!("Web interface started on port {}", config.scrubber.web_port);
    }

    // Run based on the command
    let mut scrubber_guard = scrubber.lock().await;
    match &args.command {
        scrobble_scrubber::Commands::Run { .. } => {
            info!("Starting continuous monitoring mode");
            scrubber_guard.run().await?;
        }
        scrobble_scrubber::Commands::Once { .. } => {
            info!("Running single pass");
            scrubber_guard.trigger_run().await?;
        }
        scrobble_scrubber::Commands::LastN { tracks, .. } => {
            info!("Processing last {tracks} tracks");
            scrubber_guard.process_last_n_tracks(*tracks).await?;
        }
        scrobble_scrubber::Commands::Artist { name, .. } => {
            info!("Processing all tracks for artist '{name}'");
            scrubber_guard.process_artist(name).await?;
        }
    }

    Ok(())
}
