mod commands;

use clap::{Parser, Subcommand, ValueEnum};
use commands::*;
use config::ConfigError;
use lastfm_edit::{LastFmEditClientImpl, LastFmError, Result};
use scrobble_scrubber::config::{OpenAIProviderConfig, ScrobbleScrubberConfig, StorageConfig};
use scrobble_scrubber::event_logger::EventLogger;
use scrobble_scrubber::openai_provider::OpenAIScrubActionProvider;
use scrobble_scrubber::persistence::{FileStorage, StateStorage};
use scrobble_scrubber::scrub_action_provider::{
    OrScrubActionProvider, RewriteRulesScrubActionProvider,
};
use scrobble_scrubber::scrubber::ScrobbleScrubber;
use scrobble_scrubber::session_manager::SessionManager;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(ValueEnum, Clone, Debug)]
enum ProviderType {
    /// MusicBrainz metadata correction provider
    Musicbrainz,
    /// OpenAI-powered suggestion provider
    Openai,
}

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

    /// Select which suggestion provider to use (can be used multiple times)
    #[arg(long = "provider", value_enum)]
    providers: Vec<ProviderType>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum ScrubberCommands {
    /// Run continuously, monitoring for new tracks (default mode)
    Run {
        /// Check interval in seconds
        #[arg(short, long)]
        interval: Option<u64>,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,
    },
    /// Run once and exit after processing new tracks since last run
    Once {
        /// Set timestamp anchor to specific time before processing (ISO 8601 format like "2025-07-22T07:08:00Z")
        #[arg(long)]
        set_anchor_timestamp: Option<String>,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,
    },
    /// Process the last N tracks without updating timestamp state
    LastN {
        /// Number of tracks to process
        #[arg(short, long)]
        tracks: u32,

        /// Focus on pattern analysis and rewrite rule suggestions
        #[arg(long)]
        rule_focus: bool,

        /// Skip applying existing rewrite rules (useful with --rule-focus)
        #[arg(long)]
        no_existing_rules: bool,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,
    },
    /// Process tracks for a specific artist or album
    Artist {
        /// Artist name to process
        #[arg(short, long)]
        name: String,

        /// Album name to process (optional - if specified, only process tracks from this album)
        #[arg(short, long)]
        album: Option<String>,

        /// Dry run mode - don't actually make any edits
        #[arg(long)]
        dry_run: bool,

        /// Require confirmation for all edits
        #[arg(long)]
        require_confirmation: bool,

        /// Require confirmation for proposed rewrite rules
        #[arg(long)]
        require_proposed_rule_confirmation: bool,
    },
}

#[derive(Subcommand, Debug)]
enum TrackCacheCommands {
    /// Show recent tracks cache state (track names, artists, timestamps)
    Show {
        /// Limit the number of tracks to show (default: 50)
        #[arg(short, long, default_value = "50")]
        limit: usize,
        /// Show tracks from all cached pages
        #[arg(long)]
        all_pages: bool,
    },
    /// Refresh track cache from Last.fm API (overwrites existing cache)
    Refresh {
        /// Number of pages to fetch (default: 1, ~50 tracks per page)
        #[arg(short, long, default_value = "1")]
        pages: usize,
    },
    /// Extend track cache by fetching more tracks from Last.fm API
    Extend {
        /// Number of pages to fetch (default: 1, ~50 tracks per page)
        #[arg(short, long, default_value = "1")]
        pages: usize,
    },
    /// Show recent tracks directly from Last.fm API
    ShowRecent {
        /// Number of tracks to show (default: 50)
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
}

#[derive(Subcommand, Debug)]
enum RulesCommands {
    /// Show current active rewrite rules
    Show,
    /// Add a new rewrite rule
    Add {
        /// Rule name (optional)
        #[arg(short, long)]
        name: Option<String>,

        /// Track name pattern to find (regex)
        #[arg(long)]
        track_find: Option<String>,

        /// Track name replacement text
        #[arg(long)]
        track_replace: Option<String>,

        /// Artist name pattern to find (regex)
        #[arg(long)]
        artist_find: Option<String>,

        /// Artist name replacement text
        #[arg(long)]
        artist_replace: Option<String>,

        /// Album name pattern to find (regex)
        #[arg(long)]
        album_find: Option<String>,

        /// Album name replacement text
        #[arg(long)]
        album_replace: Option<String>,

        /// Album artist name pattern to find (regex)
        #[arg(long)]
        album_artist_find: Option<String>,

        /// Album artist name replacement text
        #[arg(long)]
        album_artist_replace: Option<String>,

        /// Regex flags (e.g., 'i' for case insensitive)
        #[arg(long)]
        flags: Option<String>,

        /// Require confirmation before applying this rule
        #[arg(long)]
        require_confirmation: bool,
    },
    /// Remove a rewrite rule
    Remove {
        /// Rule index to remove (1-based, as shown in show-rules)
        #[arg(short, long)]
        index: Option<usize>,

        /// Rule name to remove (alternative to index)
        #[arg(short, long)]
        name: Option<String>,

        /// Remove all rules (requires confirmation)
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand, Debug)]
enum TimestampCommands {
    /// Set timestamp anchor back N tracks from current position
    SetAnchor {
        /// Number of tracks to go back
        #[arg(short, long)]
        tracks: u32,
    },
    /// Set timestamp anchor to a specific timestamp
    SetTimestamp {
        /// Timestamp in ISO 8601 format (e.g., "2025-07-22T07:08:00Z")
        #[arg(short, long)]
        timestamp: String,
    },
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Core scrubbing operations
    #[command(subcommand)]
    Scrubber(ScrubberCommands),
    /// Track cache management
    #[command(subcommand)]
    TrackCache(TrackCacheCommands),
    /// Rewrite rule management
    #[command(subcommand)]
    Rules(RulesCommands),
    /// Pending edit management
    #[command(subcommand)]
    Pending(PendingCommands),
    /// Timestamp anchor management
    #[command(subcommand)]
    Timestamp(TimestampCommands),
    /// Clear saved session data (forces fresh login on next run)
    ClearSession,
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
        Commands::Scrubber(scrubber_cmd) => match scrubber_cmd {
            ScrubberCommands::Run {
                interval,
                dry_run,
                require_confirmation,
                require_proposed_rule_confirmation,
            } => {
                if let Some(interval) = interval {
                    config.scrubber.interval = *interval;
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
            }
            ScrubberCommands::Once {
                set_anchor_timestamp: _,
                dry_run,
                require_confirmation,
                require_proposed_rule_confirmation,
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
            }
            ScrubberCommands::LastN {
                tracks: _,
                rule_focus: _,
                no_existing_rules: _,
                dry_run,
                require_confirmation,
                require_proposed_rule_confirmation,
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
            }
            ScrubberCommands::Artist {
                name: _,
                album: _,
                dry_run,
                require_confirmation,
                require_proposed_rule_confirmation,
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
            }
        },
        Commands::TrackCache(_) => {
            // No specific configuration needed for track cache commands
        }
        Commands::Rules(_) => {
            // No specific configuration needed for rules commands
        }
        Commands::Pending(_) => {
            // No specific configuration needed for pending commands
        }
        Commands::Timestamp(_) => {
            // No specific configuration needed for timestamp commands
        }
        Commands::ClearSession => {
            // No specific configuration needed for clearing session
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

    // Update state file path to use per-user directory if no explicit state file was provided
    // and we have a username
    if args.state_file.is_none() && !config.lastfm.username.is_empty() {
        config.storage.state_file =
            StorageConfig::get_default_state_file_path_for_user(Some(&config.lastfm.username));
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
    // Update provider configuration based on CLI flags
    // If --provider flags are specified, disable all providers by default
    // and only enable the ones explicitly requested
    if !args.providers.is_empty() {
        config.providers.enable_rewrite_rules = false;
        config.providers.enable_openai = false;
        config.providers.enable_musicbrainz = false;
        config.providers.enable_http = false;

        for provider in &args.providers {
            match provider {
                ProviderType::Musicbrainz => {
                    config.providers.enable_musicbrainz = true;
                }
                ProviderType::Openai => {
                    config.providers.enable_openai = true;
                }
            }
        }
    }

    config
}

/// Create an authenticated Last.fm client, using saved session if available
async fn create_authenticated_client(
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

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize env_logger from environment variables (RUST_LOG), fallback to Info level
    env_logger::init();
    let args = Args::parse();

    // Load configuration from args, env vars, and config files
    let config = load_config_from_args(&args).map_err(|e| {
        LastFmError::Io(std::io::Error::other(format!(
            "Failed to load configuration: {e}"
        )))
    })?;

    log::info!(
        "Starting scrobble-scrubber with interval {}s",
        config.scrubber.interval
    );

    // Create and login to LastFM client (using session if available)
    let client = create_authenticated_client(&config).await?;

    // Create storage wrapped in Arc<Mutex<>>
    log::info!("Using state file: {}", config.storage.state_file);
    let storage = Arc::new(Mutex::new(
        FileStorage::new(&config.storage.state_file).map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to create storage: {e}"
            )))
        })?,
    ));

    // Check if we should skip existing rewrite rules (for pattern analysis)
    let skip_existing_rules = matches!(
        &args.command,
        Commands::Scrubber(ScrubberCommands::LastN {
            no_existing_rules: true,
            ..
        })
    );

    // Create action provider
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

    if config.providers.enable_rewrite_rules && !skip_existing_rules {
        let rewrite_provider = RewriteRulesScrubActionProvider::new(&rules_state);
        action_provider = action_provider.add_provider(rewrite_provider);
        log::info!("Enabled rewrite rules provider");
    } else if skip_existing_rules {
        log::info!("Skipping existing rewrite rules for pattern analysis");
    }

    // Add OpenAI provider if enabled and configured
    if config.providers.enable_openai {
        // Try to get OpenAI config, or create default from environment variables
        let openai_config_opt = if let Some(openai_config) = &config.providers.openai {
            Some(openai_config.clone())
        } else {
            // Check if API key is available from environment variables
            if let Ok(api_key) = std::env::var("SCROBBLE_SCRUBBER_OPENAI_API_KEY") {
                log::info!("Creating default OpenAI configuration from environment variable");
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
                Ok(mut openai_provider) => {
                    // Enable rule focus mode if requested
                    if matches!(
                        &args.command,
                        Commands::Scrubber(ScrubberCommands::LastN {
                            rule_focus: true,
                            ..
                        })
                    ) {
                        openai_provider.enable_rule_focus_mode();
                        log::info!(
                            "Enabled OpenAI provider with RULE FOCUS mode for pattern analysis"
                        );
                    } else {
                        log::info!(
                            "Enabled OpenAI provider with model: {}",
                            openai_config.model.as_deref().unwrap_or("default")
                        );
                    }
                    action_provider = action_provider.add_provider(openai_provider);
                }
                Err(e) => {
                    log::warn!("Failed to create OpenAI provider: {e}");
                }
            }
        }
    }

    // Add MusicBrainz provider if enabled and configured
    if config.providers.enable_musicbrainz {
        let musicbrainz_provider = if let Some(mb_config) = &config.providers.musicbrainz {
            scrobble_scrubber::musicbrainz_provider::MusicBrainzScrubActionProvider::new(
                mb_config.confidence_threshold,
                mb_config.max_results,
            )
        } else {
            scrobble_scrubber::musicbrainz_provider::MusicBrainzScrubActionProvider::default()
        };

        action_provider = action_provider.add_provider(musicbrainz_provider);
        log::info!("Enabled MusicBrainz provider for metadata corrections");
    }

    // Log active providers summary
    let mut active_providers = Vec::new();
    if config.providers.enable_rewrite_rules && !skip_existing_rules {
        active_providers.push("RewriteRules");
    }
    if config.providers.enable_openai {
        active_providers.push("OpenAI");
    }
    if config.providers.enable_musicbrainz {
        active_providers.push("MusicBrainz");
    }
    if config.providers.enable_http {
        active_providers.push("HTTP");
    }

    if active_providers.is_empty() {
        log::warn!("No scrub action providers are enabled");
    } else {
        log::info!("Active providers: {}", active_providers.join(", "));
    }

    // Handle commands that don't need a scrubber instance first
    match &args.command {
        Commands::TrackCache(cache_cmd) => match cache_cmd {
            TrackCacheCommands::Show { limit, all_pages } => {
                show_cache_state(*limit, *all_pages)?;
                return Ok(());
            }
            TrackCacheCommands::Refresh { pages } => {
                refresh_cache(&client, *pages).await?;
                return Ok(());
            }
            TrackCacheCommands::Extend { pages } => {
                extend_cache(&client, *pages).await?;
                return Ok(());
            }
            TrackCacheCommands::ShowRecent { limit } => {
                show_recent_tracks_from_api(&client, *limit).await?;
                return Ok(());
            }
        },
        Commands::Rules(rules_cmd) => match rules_cmd {
            RulesCommands::Show => {
                show_active_rules(&storage).await?;
                return Ok(());
            }
            RulesCommands::Add {
                name,
                track_find,
                track_replace,
                artist_find,
                artist_replace,
                album_find,
                album_replace,
                album_artist_find,
                album_artist_replace,
                flags,
                require_confirmation,
            } => {
                add_rewrite_rule(
                    &storage,
                    name.as_deref(),
                    track_find.as_deref(),
                    track_replace.as_deref(),
                    artist_find.as_deref(),
                    artist_replace.as_deref(),
                    album_find.as_deref(),
                    album_replace.as_deref(),
                    album_artist_find.as_deref(),
                    album_artist_replace.as_deref(),
                    flags.as_deref(),
                    *require_confirmation,
                )
                .await?;
                return Ok(());
            }
            RulesCommands::Remove { index, name, all } => {
                remove_rewrite_rule(&storage, *index, name.as_deref(), *all).await?;
                return Ok(());
            }
        },
        Commands::Pending(pending_cmd) => {
            let data_dir = std::path::PathBuf::from(&config.storage.state_file)
                .parent()
                .unwrap()
                .to_path_buf();
            let pending_args = PendingArgs {
                command: pending_cmd.clone(),
            };
            if let Err(e) = handle_pending_command(pending_args, data_dir).await {
                return Err(lastfm_edit::LastFmError::Io(std::io::Error::other(
                    format!("Pending command failed: {e}"),
                )));
            }
            return Ok(());
        }
        Commands::Timestamp(timestamp_cmd) => match timestamp_cmd {
            TimestampCommands::SetAnchor { tracks } => {
                set_timestamp_anchor(&storage, *tracks).await?;
                return Ok(());
            }
            TimestampCommands::SetTimestamp { timestamp } => {
                set_timestamp_anchor_to_timestamp(&storage, timestamp).await?;
                return Ok(());
            }
        },
        Commands::ClearSession => {
            let session_manager = SessionManager::new(&config.lastfm.username);
            if let Err(e) = session_manager.clear_session() {
                log::error!("Failed to clear session: {e}");
                return Err(LastFmError::Io(e));
            }
            println!("✅ Session cleared successfully");
            println!("Next run will require username/password login");
            return Ok(());
        }
        Commands::Scrubber(_) => {
            // Continue to create scrubber for scrubber commands
        }
    }

    // Create scrubber wrapped in Arc<Mutex<>>
    let scrubber = Arc::new(Mutex::new(ScrobbleScrubber::new(
        storage.clone(),
        Box::new(client),
        action_provider,
        config.clone(),
    )));

    // Start event logger for JSON logging of edit attempts
    {
        let event_receiver = scrubber.lock().await.subscribe_events();
        let log_file_path = StorageConfig::get_edit_log_path(&config.storage.state_file);
        let mut event_logger = EventLogger::new(log_file_path.clone(), true, event_receiver);

        tokio::spawn(async move {
            log::info!("Started edit logging to: {log_file_path}");
            event_logger.run().await;
        });
    }

    // Web interface has been removed

    // For scrubber commands, we need to acquire the lock and execute them
    // All other commands have been handled above

    let mut scrubber_guard = scrubber.lock().await;
    match &args.command {
        Commands::Scrubber(scrubber_cmd) => match scrubber_cmd {
            ScrubberCommands::Run { .. } => {
                log::info!("Starting continuous monitoring mode");
                scrubber_guard.run().await?;
            }
            ScrubberCommands::Once {
                set_anchor_timestamp,
                ..
            } => {
                if let Some(timestamp_str) = set_anchor_timestamp {
                    log::info!("Setting timestamp anchor before processing");
                    drop(scrubber_guard); // Release lock before calling set_timestamp_anchor_to_timestamp
                    set_timestamp_anchor_to_timestamp(&storage, timestamp_str).await?;
                    scrubber_guard = scrubber.lock().await; // Re-acquire lock
                }
                log::info!("Running single pass");
                scrubber_guard.trigger_run().await?;
            }
            ScrubberCommands::LastN {
                tracks,
                rule_focus,
                no_existing_rules,
                ..
            } => {
                let mode_info = match (rule_focus, no_existing_rules) {
                    (true, true) => " (PATTERN ANALYSIS MODE - rule focus, no existing rules)",
                    (true, false) => " (rule focus mode)",
                    (false, true) => " (no existing rules)",
                    (false, false) => "",
                };
                log::info!("Processing last {tracks} tracks{mode_info}");
                scrubber_guard.process_last_n_tracks(*tracks).await?;
            }
            ScrubberCommands::Artist { name, album, .. } => {
                if let Some(album_name) = album {
                    log::info!("Processing tracks for album '{album_name}' by artist '{name}'");
                    scrubber_guard.process_album(name, album_name).await?;
                } else {
                    log::info!("Processing all tracks for artist '{name}'");
                    scrubber_guard.process_artist(name).await?;
                }
            }
        },
        Commands::TrackCache(_)
        | Commands::Rules(_)
        | Commands::Pending(_)
        | Commands::Timestamp(_)
        | Commands::ClearSession => {
            // These cases are handled above
            unreachable!("Non-scrubber commands should have been handled earlier");
        }
    }

    Ok(())
}
