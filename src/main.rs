use clap::{Parser, Subcommand};
use config::ConfigError;
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
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(name = "scrobble-scrubber")]
#[command(about = "Automated Last.fm track monitoring and scrubbing system")]
struct Args {
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Path to state file for persistence
    #[arg(short, long)]
    state_file: Option<String>,

    /// Last.fm username
    #[arg(long)]
    lastfm_username: Option<String>,

    /// Last.fm password
    #[arg(long)]
    lastfm_password: Option<String>,

    /// Enable `OpenAI` provider
    #[arg(long)]
    enable_openai: bool,

    /// `OpenAI` API key
    #[arg(long)]
    openai_api_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run continuously, monitoring for new tracks (default mode)
    Run {
        /// Check interval in seconds
        #[arg(short, long)]
        interval: Option<u64>,

        /// Maximum number of tracks to check per run
        #[arg(short, long)]
        max_tracks: Option<usize>,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,

        /// Enable web interface for managing pending rules and edits
        #[arg(long)]
        enable_web_interface: bool,

        /// Port for web interface (default: 8080)
        #[arg(long)]
        web_port: Option<u16>,
    },
    /// Run once and exit after processing new tracks since last run
    Once {
        /// Maximum number of tracks to check
        #[arg(short, long)]
        max_tracks: Option<usize>,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,

        /// Enable web interface for managing pending rules and edits
        #[arg(long)]
        enable_web_interface: bool,

        /// Port for web interface (default: 8080)
        #[arg(long)]
        web_port: Option<u16>,
    },
    /// Process the last N tracks without updating timestamp state
    LastN {
        /// Number of tracks to process
        #[arg(short, long)]
        tracks: u32,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,

        /// Enable web interface for managing pending rules and edits
        #[arg(long)]
        enable_web_interface: bool,

        /// Port for web interface (default: 8080)
        #[arg(long)]
        web_port: Option<u16>,
    },
    /// Process all tracks for a specific artist
    Artist {
        /// Artist name to process
        #[arg(short, long)]
        name: String,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,

        /// Enable web interface for managing pending rules and edits
        #[arg(long)]
        enable_web_interface: bool,

        /// Port for web interface (default: 8080)
        #[arg(long)]
        web_port: Option<u16>,
    },
}

/// Load configuration from args with optional config file override
fn load_config_from_args(args: &Args) -> std::result::Result<ScrobbleScrubberConfig, ConfigError> {
    let config = if let Some(config_path) = &args.config {
        ScrobbleScrubberConfig::load_with_file(Some(config_path))?
    } else {
        ScrobbleScrubberConfig::load()?
    };

    Ok(merge_args_into_config(config, args))
}

/// Merge command line arguments into the configuration
fn merge_args_into_config(
    mut config: ScrobbleScrubberConfig,
    args: &Args,
) -> ScrobbleScrubberConfig {
    // Apply command-specific overrides
    match &args.command {
        Commands::Run {
            interval,
            max_tracks,
            dry_run,
            require_confirmation,
            require_proposed_rule_confirmation,
            enable_web_interface,
            web_port,
        } => {
            if let Some(interval) = interval {
                config.scrubber.interval = *interval;
            }
            if let Some(max_tracks) = max_tracks {
                config.scrubber.max_tracks = *max_tracks as u32;
            }
            if *dry_run {
                config.scrubber.dry_run = true;
            }
            if *require_confirmation {
                config.scrubber.require_confirmation = true;
            }
            if *require_proposed_rule_confirmation {
                config.scrubber.require_proposed_rule_confirmation = true;
            }
            if *enable_web_interface {
                config.scrubber.enable_web_interface = true;
            }
            if let Some(web_port) = web_port {
                config.scrubber.web_port = *web_port;
            }
        }
        Commands::Once {
            max_tracks,
            dry_run,
            require_confirmation,
            require_proposed_rule_confirmation,
            enable_web_interface,
            web_port,
        } => {
            if let Some(max_tracks) = max_tracks {
                config.scrubber.max_tracks = *max_tracks as u32;
            }
            if *dry_run {
                config.scrubber.dry_run = true;
            }
            if *require_confirmation {
                config.scrubber.require_confirmation = true;
            }
            if *require_proposed_rule_confirmation {
                config.scrubber.require_proposed_rule_confirmation = true;
            }
            if *enable_web_interface {
                config.scrubber.enable_web_interface = true;
            }
            if let Some(web_port) = web_port {
                config.scrubber.web_port = *web_port;
            }
        }
        Commands::LastN {
            tracks: _,
            dry_run,
            require_confirmation,
            require_proposed_rule_confirmation,
            enable_web_interface,
            web_port,
        } => {
            if *dry_run {
                config.scrubber.dry_run = true;
            }
            if *require_confirmation {
                config.scrubber.require_confirmation = true;
            }
            if *require_proposed_rule_confirmation {
                config.scrubber.require_proposed_rule_confirmation = true;
            }
            if *enable_web_interface {
                config.scrubber.enable_web_interface = true;
            }
            if let Some(web_port) = web_port {
                config.scrubber.web_port = *web_port;
            }
            // Note: tracks count is handled in main.rs, not stored in config
        }
        Commands::Artist {
            name: _,
            dry_run,
            require_confirmation,
            require_proposed_rule_confirmation,
            enable_web_interface,
            web_port,
        } => {
            if *dry_run {
                config.scrubber.dry_run = true;
            }
            if *require_confirmation {
                config.scrubber.require_confirmation = true;
            }
            if *require_proposed_rule_confirmation {
                config.scrubber.require_proposed_rule_confirmation = true;
            }
            if *enable_web_interface {
                config.scrubber.enable_web_interface = true;
            }
            if let Some(web_port) = web_port {
                config.scrubber.web_port = *web_port;
            }
            // Note: artist name is handled in main.rs, not stored in config
        }
    }

    // Apply global args overrides
    if let Some(state_file) = &args.state_file {
        config.storage.state_file = state_file.clone();
    }
    if let Some(username) = &args.lastfm_username {
        config.lastfm.username = username.clone();
    }
    if let Some(password) = &args.lastfm_password {
        config.lastfm.password = password.clone();
    }
    if args.enable_openai {
        config.providers.enable_openai = true;
    }
    if let Some(api_key) = &args.openai_api_key {
        if config.providers.openai.is_none() {
            config.providers.openai = Some(OpenAIProviderConfig {
                api_key: api_key.clone(),
                model: None,
                system_prompt: None,
            });
        } else {
            config.providers.openai.as_mut().unwrap().api_key = api_key.clone();
        }
    }

    config
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    let args = Args::parse();

    // Load configuration from args, env vars, and config files
    let config = load_config_from_args(&args).map_err(|e| {
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
        Commands::Run { .. } => {
            info!("Starting continuous monitoring mode");
            scrubber_guard.run().await?;
        }
        Commands::Once { .. } => {
            info!("Running single pass");
            scrubber_guard.trigger_run().await?;
        }
        Commands::LastN { tracks, .. } => {
            info!("Processing last {tracks} tracks");
            scrubber_guard.process_last_n_tracks(*tracks).await?;
        }
        Commands::Artist { name, .. } => {
            info!("Processing all tracks for artist '{name}'");
            scrubber_guard.process_artist(name).await?;
        }
    }

    Ok(())
}
