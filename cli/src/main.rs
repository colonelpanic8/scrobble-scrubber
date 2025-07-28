use clap::{Parser, Subcommand, ValueEnum};
use config::ConfigError;
use lastfm_edit::{LastFmEditClient, LastFmEditClientImpl, LastFmError, Result};
use scrobble_scrubber::config::{OpenAIProviderConfig, ScrobbleScrubberConfig, StorageConfig};
use scrobble_scrubber::event_logger::EventLogger;
use scrobble_scrubber::openai_provider::OpenAIScrubActionProvider;
use scrobble_scrubber::persistence::{FileStorage, StateStorage};
use scrobble_scrubber::rewrite::{RewriteRule, SdRule};
use scrobble_scrubber::scrub_action_provider::{
    OrScrubActionProvider, RewriteRulesScrubActionProvider,
};
use scrobble_scrubber::scrubber::ScrobbleScrubber;
use scrobble_scrubber::session_manager::SessionManager;
use scrobble_scrubber::web_interface;
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
enum Commands {
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

        /// Enable web interface for managing pending rules and edits
        #[arg(long)]
        enable_web_interface: bool,

        /// Port for web interface (default: 8080)
        #[arg(long)]
        web_port: Option<u16>,
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

        /// Enable web interface for managing pending rules and edits
        #[arg(long)]
        enable_web_interface: bool,

        /// Port for web interface (default: 8080)
        #[arg(long)]
        web_port: Option<u16>,
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

        /// Enable web interface for managing pending rules and edits
        #[arg(long)]
        enable_web_interface: bool,

        /// Port for web interface (default: 8080)
        #[arg(long)]
        web_port: Option<u16>,
    },
    /// Start only the web interface for managing pending rules and edits
    Web {
        /// Port for web interface (default: 8080)
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Clear saved session data (forces fresh login on next run)
    ClearSession,
    /// Show recent tracks cache state (track names, artists, timestamps)
    ShowCache {
        /// Limit the number of tracks to show (default: 50)
        #[arg(short, long, default_value = "50")]
        limit: usize,
        /// Show tracks from all cached pages
        #[arg(long)]
        all_pages: bool,
    },
    /// Show current active rewrite rules
    ShowRules,
    /// Add a new rewrite rule
    AddRule {
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
    RemoveRule {
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
    /// Set timestamp anchor back N tracks from current position
    SetAnchor {
        /// Number of tracks to go back
        #[arg(short, long)]
        tracks: u32,
    },
    /// Set timestamp anchor to a specific timestamp
    SetAnchorTimestamp {
        /// Timestamp in ISO 8601 format (e.g., "2025-07-22T07:08:00Z")
        #[arg(short, long)]
        timestamp: String,
    },
    /// Show recent tracks directly from Last.fm API
    ShowRecentTracks {
        /// Number of tracks to show (default: 50)
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
    /// Refresh track cache from Last.fm API (overwrites existing cache)
    RefreshCache {
        /// Number of pages to fetch (default: 1, ~50 tracks per page)
        #[arg(short, long, default_value = "1")]
        pages: usize,
    },
    /// Extend track cache by fetching more tracks from Last.fm API
    ExtendCache {
        /// Number of pages to fetch (default: 1, ~50 tracks per page)
        #[arg(short, long, default_value = "1")]
        pages: usize,
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
            dry_run,
            require_confirmation,
            require_proposed_rule_confirmation,
            enable_web_interface,
            web_port,
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
            if *enable_web_interface {
                config.scrubber.enable_web_interface = true;
            }
            if let Some(web_port) = web_port {
                config.scrubber.web_port = *web_port;
            }
        }
        Commands::Once {
            set_anchor_timestamp: _,
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
        }
        Commands::LastN {
            tracks: _,
            rule_focus: _,
            no_existing_rules: _,
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
            // Note: tracks count, rule_focus, and no_existing_rules are handled in main.rs, not stored in config
        }
        Commands::Artist {
            name: _,
            album: _,
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
            // Note: artist name and album are handled in main.rs, not stored in config
        }
        Commands::Web { port } => {
            // Enable web interface for web-only mode
            config.scrubber.enable_web_interface = true;
            if let Some(web_port) = port {
                config.scrubber.web_port = *web_port;
            }
        }
        Commands::ShowCache { .. } => {
            // No specific configuration needed for cache inspection
        }
        Commands::ShowRules => {
            // No specific configuration needed for rules inspection
        }
        Commands::AddRule { .. } => {
            // No specific configuration needed for adding rules
        }
        Commands::RemoveRule { .. } => {
            // No specific configuration needed for removing rules
        }
        Commands::SetAnchor { .. } => {
            // No specific configuration needed for setting anchor
        }
        Commands::SetAnchorTimestamp { .. } => {
            // No specific configuration needed for setting anchor by timestamp
        }
        Commands::ShowRecentTracks { .. } => {
            // No specific configuration needed for showing recent tracks
        }
        Commands::RefreshCache { .. } => {
            // No specific configuration needed for refreshing cache
        }
        Commands::ExtendCache { .. } => {
            // No specific configuration needed for extending cache
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

/// Show recent tracks cache state (track names, artists, timestamps)
fn show_cache_state(limit: usize, all_pages: bool) -> Result<()> {
    use chrono::DateTime;
    use scrobble_scrubber::track_cache::TrackCache;

    let cache = TrackCache::load();

    println!("📂 Track Cache State");
    println!("==================");

    // Show recent tracks
    let recent_tracks_count = cache.recent_tracks.len();

    println!("Recent Tracks:");
    println!("  {recent_tracks_count} tracks cached");

    if recent_tracks_count > 0 {
        let tracks = if all_pages {
            cache.get_all_recent_tracks()
        } else {
            cache.get_recent_tracks_limited(limit)
        };

        println!("  Showing {} tracks:", tracks.len().min(limit));
        for (i, track) in tracks.iter().take(limit).enumerate() {
            let timestamp = track
                .timestamp
                .map(|ts| {
                    DateTime::from_timestamp(ts as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                })
                .unwrap_or_else(|| "No timestamp".to_string());

            println!(
                "    {}: '{}' by '{}' [{}]",
                i + 1,
                track.name,
                track.artist,
                timestamp
            );
        }

        if tracks.len() > limit {
            println!("    ... and {} more tracks", tracks.len() - limit);
        }
    } else {
        println!("  No recent tracks cached");
    }

    // Show artist tracks
    let artist_tracks_count: usize = cache.artist_tracks.values().map(|v| v.len()).sum();
    let artists_count = cache.artist_tracks.len();

    println!("\nArtist Tracks:");
    println!("  {artists_count} artists cached, {artist_tracks_count} total tracks");

    if artists_count > 0 {
        for (artist, tracks) in &cache.artist_tracks {
            println!("    '{}': {} tracks", artist, tracks.len());
        }
    } else {
        println!("  No artist tracks cached");
    }

    Ok(())
}

/// Refresh track cache from Last.fm API (clear and reload)
async fn refresh_cache(client: &LastFmEditClientImpl, pages: usize) -> Result<()> {
    use lastfm_edit::AsyncPaginatedIterator;
    use scrobble_scrubber::track_cache::TrackCache;

    println!("🔄 Refreshing Track Cache");
    println!("========================");
    println!(
        "This will clear the existing cache and fetch {pages} page(s) of fresh data from Last.fm"
    );

    let mut cache = TrackCache::default(); // Start with empty cache
    let mut recent_iterator = client.recent_tracks();
    let mut fetched_tracks = Vec::new();
    let mut current_page = 0;

    println!("Fetching page {} from Last.fm API...", current_page + 1);

    while current_page < pages {
        let mut page_tracks = 0;

        // Fetch approximately 50 tracks per page (typical Last.fm page size)
        while page_tracks < 50 {
            if let Some(track) = recent_iterator.next().await? {
                fetched_tracks.push(track);
                page_tracks += 1;
            } else {
                println!(
                    "📄 Reached end of available tracks after {} tracks",
                    fetched_tracks.len()
                );
                break;
            }
        }

        current_page += 1;
        if current_page < pages && page_tracks > 0 {
            println!("Fetching page {} from Last.fm API...", current_page + 1);
        }

        // Break if we didn't get a full page (likely at end of data)
        if page_tracks < 50 {
            break;
        }
    }

    // Add all fetched tracks to the cache
    cache.add_recent_tracks(fetched_tracks);
    cache.save().map_err(|e| {
        LastFmError::Io(std::io::Error::other(format!("Failed to save cache: {e}")))
    })?;

    let stats = cache.stats();
    println!("✅ Cache refreshed successfully");
    println!(
        "  Fetched {} pages (~{} tracks)",
        current_page, stats.recent_track_count
    );
    println!("  Total tracks cached: {}", stats.recent_track_count);

    Ok(())
}

/// Extend track cache by fetching additional tracks
async fn extend_cache(client: &LastFmEditClientImpl, pages: usize) -> Result<()> {
    use lastfm_edit::AsyncPaginatedIterator;
    use scrobble_scrubber::track_cache::TrackCache;

    println!("📈 Extending Track Cache");
    println!("=======================");

    let mut cache = TrackCache::load();
    let initial_count = cache.stats().recent_track_count;

    println!("Current cache contains {initial_count} tracks");
    println!("Fetching {pages} additional page(s) from Last.fm API...");

    // Get the oldest timestamp to continue from where cache ends
    let oldest_cached = cache
        .recent_tracks
        .last()
        .and_then(|track| track.timestamp)
        .and_then(|ts| chrono::DateTime::from_timestamp(ts as i64, 0));

    let mut recent_iterator = client.recent_tracks();
    let mut fetched_tracks = Vec::new();
    let mut current_page = 0;
    let mut new_tracks_found = 0;

    // Skip to where we left off (if we have cached data)
    if let Some(oldest_time) = oldest_cached {
        println!("Skipping to tracks older than {oldest_time}...");
        loop {
            if let Some(track) = recent_iterator.next().await? {
                if let Some(track_ts) = track.timestamp {
                    if let Some(track_time) = chrono::DateTime::from_timestamp(track_ts as i64, 0) {
                        if track_time < oldest_time {
                            // Found older tracks, include this track and continue
                            fetched_tracks.push(track);
                            break;
                        }
                    }
                }
            } else {
                println!("📄 No older tracks available");
                return Ok(());
            }
        }
    }

    println!("Fetching page {} from Last.fm API...", current_page + 1);

    // Now fetch the requested number of pages
    while current_page < pages {
        let mut page_tracks = 0;

        // Fetch approximately 50 tracks per page
        while page_tracks < 50 {
            if let Some(track) = recent_iterator.next().await? {
                fetched_tracks.push(track);
                page_tracks += 1;
                new_tracks_found += 1;
            } else {
                println!("📄 Reached end of available tracks");
                break;
            }
        }

        current_page += 1;
        if current_page < pages && page_tracks > 0 {
            println!("Fetching page {} from Last.fm API...", current_page + 1);
        }

        // Break if we didn't get a full page (likely at end of data)
        if page_tracks < 50 {
            break;
        }
    }

    // Merge with existing cache
    let _stats = cache.merge_recent_tracks(fetched_tracks);
    cache.save().map_err(|e| {
        LastFmError::Io(std::io::Error::other(format!("Failed to save cache: {e}")))
    })?;

    let final_count = cache.stats().recent_track_count;
    let actual_added = final_count.saturating_sub(initial_count);

    println!("✅ Cache extended successfully");
    println!("  Fetched {current_page} pages (~{new_tracks_found} tracks)");
    println!("  Added {actual_added} new unique tracks");
    println!("  Total tracks cached: {final_count}");

    Ok(())
}

/// Show current active rewrite rules
async fn show_active_rules(
    storage: &Arc<Mutex<scrobble_scrubber::persistence::FileStorage>>,
) -> Result<()> {
    use scrobble_scrubber::persistence::StateStorage;

    println!("📝 Active Rewrite Rules");
    println!("=====================");

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

    if rules_state.rewrite_rules.is_empty() {
        println!("No rewrite rules configured");
        return Ok(());
    }

    println!("Found {} rewrite rules:", rules_state.rewrite_rules.len());

    for (i, rule) in rules_state.rewrite_rules.iter().enumerate() {
        println!(
            "  Rule {}: {}",
            i + 1,
            rule.name.as_deref().unwrap_or(&format!("Rule #{}", i + 1))
        );

        if let Some(track_rule) = &rule.track_name {
            println!(
                "    Track: '{}' → '{}'",
                track_rule.find, track_rule.replace
            );
        }
        if let Some(artist_rule) = &rule.artist_name {
            println!(
                "    Artist: '{}' → '{}'",
                artist_rule.find, artist_rule.replace
            );
        }
        if let Some(album_rule) = &rule.album_name {
            println!(
                "    Album: '{}' → '{}'",
                album_rule.find, album_rule.replace
            );
        }
        if let Some(album_artist_rule) = &rule.album_artist_name {
            println!(
                "    Album Artist: '{}' → '{}'",
                album_artist_rule.find, album_artist_rule.replace
            );
        }

        println!();
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
/// Add a new rewrite rule
async fn add_rewrite_rule(
    storage: &Arc<Mutex<scrobble_scrubber::persistence::FileStorage>>,
    name: Option<&str>,
    track_find: Option<&str>,
    track_replace: Option<&str>,
    artist_find: Option<&str>,
    artist_replace: Option<&str>,
    album_find: Option<&str>,
    album_replace: Option<&str>,
    album_artist_find: Option<&str>,
    album_artist_replace: Option<&str>,
    flags: Option<&str>,
    require_confirmation: bool,
) -> Result<()> {
    use scrobble_scrubber::persistence::StateStorage;

    println!("📝 Adding New Rewrite Rule");
    println!("==========================");

    // Validate that at least one field is specified
    let has_track_rule = track_find.is_some() && track_replace.is_some();
    let has_artist_rule = artist_find.is_some() && artist_replace.is_some();
    let has_album_rule = album_find.is_some() && album_replace.is_some();
    let has_album_artist_rule = album_artist_find.is_some() && album_artist_replace.is_some();

    if !has_track_rule && !has_artist_rule && !has_album_rule && !has_album_artist_rule {
        println!("❌ Error: At least one field rule must be specified");
        println!("   Use --track-find/--track-replace, --artist-find/--artist-replace,");
        println!("   --album-find/--album-replace, or --album-artist-find/--album-artist-replace");
        return Ok(());
    }

    // Validate that find/replace pairs are complete
    if (track_find.is_some() && track_replace.is_none())
        || (track_find.is_none() && track_replace.is_some())
    {
        println!("❌ Error: Track rule requires both --track-find and --track-replace");
        return Ok(());
    }
    if (artist_find.is_some() && artist_replace.is_none())
        || (artist_find.is_none() && artist_replace.is_some())
    {
        println!("❌ Error: Artist rule requires both --artist-find and --artist-replace");
        return Ok(());
    }
    if (album_find.is_some() && album_replace.is_none())
        || (album_find.is_none() && album_replace.is_some())
    {
        println!("❌ Error: Album rule requires both --album-find and --album-replace");
        return Ok(());
    }
    if (album_artist_find.is_some() && album_artist_replace.is_none())
        || (album_artist_find.is_none() && album_artist_replace.is_some())
    {
        println!("❌ Error: Album artist rule requires both --album-artist-find and --album-artist-replace");
        return Ok(());
    }

    // Create SdRule helper function
    let create_sd_rule = |find: &str, replace: &str, flags: Option<&str>| -> SdRule {
        SdRule {
            find: find.to_string(),
            replace: replace.to_string(),
            flags: flags.map(|f| f.to_string()),
        }
    };

    // Create the new rule
    let new_rule = RewriteRule {
        name: name.map(|n| n.to_string()),
        track_name: if has_track_rule {
            Some(create_sd_rule(
                track_find.unwrap(),
                track_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        artist_name: if has_artist_rule {
            Some(create_sd_rule(
                artist_find.unwrap(),
                artist_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        album_name: if has_album_rule {
            Some(create_sd_rule(
                album_find.unwrap(),
                album_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        album_artist_name: if has_album_artist_rule {
            Some(create_sd_rule(
                album_artist_find.unwrap(),
                album_artist_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        requires_confirmation: require_confirmation,
    };

    // Load existing rules
    let mut rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load rewrite rules: {e}"
            )))
        })?;

    // Add the new rule
    rules_state.rewrite_rules.push(new_rule.clone());

    // Save updated rules
    storage
        .lock()
        .await
        .save_rewrite_rules_state(&rules_state)
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to save rewrite rules: {e}"
            )))
        })?;

    // Display what was added
    println!("✅ Successfully added rewrite rule:");
    println!("   Name: {}", name.unwrap_or("(unnamed)"));
    if let Some(track_rule) = &new_rule.track_name {
        println!("   Track: '{}' → '{}'", track_rule.find, track_rule.replace);
    }
    if let Some(artist_rule) = &new_rule.artist_name {
        println!(
            "   Artist: '{}' → '{}'",
            artist_rule.find, artist_rule.replace
        );
    }
    if let Some(album_rule) = &new_rule.album_name {
        println!("   Album: '{}' → '{}'", album_rule.find, album_rule.replace);
    }
    if let Some(album_artist_rule) = &new_rule.album_artist_name {
        println!(
            "   Album Artist: '{}' → '{}'",
            album_artist_rule.find, album_artist_rule.replace
        );
    }
    if let Some(flags) = flags {
        println!("   Flags: {flags}");
    }
    if require_confirmation {
        println!("   Requires confirmation: yes");
    }

    Ok(())
}

/// Remove a rewrite rule
async fn remove_rewrite_rule(
    storage: &Arc<Mutex<scrobble_scrubber::persistence::FileStorage>>,
    index: Option<usize>,
    name: Option<&str>,
    all: bool,
) -> Result<()> {
    use scrobble_scrubber::persistence::StateStorage;
    use std::io::{self, Write};

    println!("🗑️ Removing Rewrite Rule");
    println!("========================");

    // Load existing rules
    let mut rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load rewrite rules: {e}"
            )))
        })?;

    if rules_state.rewrite_rules.is_empty() {
        println!("❌ No rewrite rules found to remove");
        return Ok(());
    }

    // Handle different removal methods
    if all {
        // Remove all rules (with confirmation)
        println!(
            "⚠️ This will remove ALL {} rewrite rules.",
            rules_state.rewrite_rules.len()
        );
        print!("Are you sure you want to continue? (y/N): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!("Failed to read input: {e}")))
        })?;

        if input.trim().to_lowercase() != "y" && input.trim().to_lowercase() != "yes" {
            println!("❌ Operation cancelled");
            return Ok(());
        }

        rules_state.rewrite_rules.clear();
        println!("✅ Removed all rewrite rules");
    } else if let Some(rule_index) = index {
        // Remove by index (1-based)
        if rule_index == 0 || rule_index > rules_state.rewrite_rules.len() {
            println!("❌ Invalid rule index: {rule_index}");
            println!("   Valid range: 1-{}", rules_state.rewrite_rules.len());
            return Ok(());
        }

        let removed_rule = rules_state.rewrite_rules.remove(rule_index - 1);
        println!(
            "✅ Removed rule #{rule_index}: {}",
            removed_rule.name.as_deref().unwrap_or("(unnamed)")
        );
    } else if let Some(rule_name) = name {
        // Remove by name
        let original_len = rules_state.rewrite_rules.len();
        rules_state
            .rewrite_rules
            .retain(|rule| rule.name.as_deref() != Some(rule_name));

        let removed_count = original_len - rules_state.rewrite_rules.len();
        if removed_count == 0 {
            println!("❌ No rule found with name: '{rule_name}'");
            return Ok(());
        } else if removed_count == 1 {
            println!("✅ Removed rule: '{rule_name}'");
        } else {
            println!("✅ Removed {removed_count} rules with name: '{rule_name}'");
        }
    } else {
        // No removal method specified
        println!("❌ Error: Must specify --index, --name, or --all");
        println!("   Use 'show-rules' to see rule indices and names");
        return Ok(());
    }

    // Save updated rules
    storage
        .lock()
        .await
        .save_rewrite_rules_state(&rules_state)
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to save rewrite rules: {e}"
            )))
        })?;

    println!("Remaining rules: {}", rules_state.rewrite_rules.len());
    Ok(())
}

/// Set timestamp anchor back N tracks from current position
async fn set_timestamp_anchor(
    storage: &Arc<Mutex<scrobble_scrubber::persistence::FileStorage>>,
    tracks_back: u32,
) -> Result<()> {
    use chrono::DateTime;
    use scrobble_scrubber::persistence::{StateStorage, TimestampState};
    use scrobble_scrubber::track_cache::TrackCache;

    println!("⏰ Setting Timestamp Anchor");
    println!("=========================");

    let cache = TrackCache::load();
    let recent_tracks = cache.get_all_recent_tracks();

    if recent_tracks.is_empty() {
        println!("❌ No recent tracks in cache. Load some tracks first.");
        return Ok(());
    }

    if tracks_back as usize >= recent_tracks.len() {
        println!(
            "❌ Requested to go back {} tracks, but only {} tracks available",
            tracks_back,
            recent_tracks.len()
        );
        return Ok(());
    }

    let target_track = &recent_tracks[tracks_back as usize];

    if let Some(timestamp) = target_track.timestamp {
        let dt = DateTime::from_timestamp(timestamp as i64, 0)
            .ok_or_else(|| LastFmError::Io(std::io::Error::other("Invalid timestamp")))?;

        println!(
            "Setting anchor to track '{}' by '{}'",
            target_track.name, target_track.artist
        );
        println!("Timestamp: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));

        let timestamp_state = TimestampState {
            last_processed_timestamp: Some(dt),
        };

        storage
            .lock()
            .await
            .save_timestamp_state(&timestamp_state)
            .await
            .map_err(|e| {
                LastFmError::Io(std::io::Error::other(format!(
                    "Failed to save timestamp state: {e}"
                )))
            })?;

        println!("✅ Timestamp anchor set successfully");
        println!("Next scrubber run will process tracks from this point forward");
    } else {
        println!("❌ Target track has no timestamp information");
    }

    Ok(())
}

/// Set timestamp anchor to a specific timestamp
async fn set_timestamp_anchor_to_timestamp(
    storage: &Arc<Mutex<scrobble_scrubber::persistence::FileStorage>>,
    timestamp_str: &str,
) -> Result<()> {
    use chrono::DateTime;
    use scrobble_scrubber::persistence::{StateStorage, TimestampState};

    println!("⏰ Setting Timestamp Anchor to Specific Time");
    println!("==========================================");

    // Parse the timestamp string
    let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to parse timestamp '{timestamp_str}': {e}. Use ISO 8601 format like '2025-07-22T07:08:00Z'"
            )))
        })?
        .with_timezone(&chrono::Utc);

    println!("Setting anchor to timestamp: {timestamp}");

    let timestamp_state = TimestampState {
        last_processed_timestamp: Some(timestamp),
    };

    storage
        .lock()
        .await
        .save_timestamp_state(&timestamp_state)
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to save timestamp state: {e}"
            )))
        })?;

    println!("✅ Timestamp anchor set successfully");
    println!("Next scrubber run will process tracks after {timestamp}");

    Ok(())
}

/// Show recent tracks directly from Last.fm API
async fn show_recent_tracks_from_api(client: &LastFmEditClientImpl, limit: usize) -> Result<()> {
    use chrono::DateTime;
    use lastfm_edit::AsyncPaginatedIterator;

    println!("🎵 Recent Tracks from Last.fm API");
    println!("=================================");

    let mut recent_iterator = client.recent_tracks();
    let mut count = 0;

    while let Some(track) = recent_iterator.next().await? {
        count += 1;

        let timestamp = if let Some(ts) = track.timestamp {
            DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "Invalid timestamp".to_string())
        } else {
            "No timestamp".to_string()
        };

        println!(
            "  {}: '{}' by '{}' [{}]",
            count, track.name, track.artist, timestamp
        );

        if count >= limit {
            break;
        }
    }

    if count == 0 {
        println!("  No recent tracks found");
    } else {
        println!("\nShowed {count} tracks from Last.fm API");
    }

    Ok(())
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
        Commands::LastN {
            no_existing_rules: true,
            ..
        }
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
                        Commands::LastN {
                            rule_focus: true,
                            ..
                        }
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
        Commands::ShowCache { limit, all_pages } => {
            show_cache_state(*limit, *all_pages)?;
            return Ok(());
        }
        Commands::ShowRules => {
            show_active_rules(&storage).await?;
            return Ok(());
        }
        Commands::AddRule {
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
        Commands::RemoveRule { index, name, all } => {
            remove_rewrite_rule(&storage, *index, name.as_deref(), *all).await?;
            return Ok(());
        }
        Commands::SetAnchor { tracks } => {
            set_timestamp_anchor(&storage, *tracks).await?;
            return Ok(());
        }
        Commands::SetAnchorTimestamp { timestamp } => {
            set_timestamp_anchor_to_timestamp(&storage, timestamp).await?;
            return Ok(());
        }
        Commands::ShowRecentTracks { limit } => {
            show_recent_tracks_from_api(&client, *limit).await?;
            return Ok(());
        }
        Commands::RefreshCache { pages } => {
            refresh_cache(&client, *pages).await?;
            return Ok(());
        }
        Commands::ExtendCache { pages } => {
            extend_cache(&client, *pages).await?;
            return Ok(());
        }
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
        _ => {
            // Continue to create scrubber for other commands
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

        log::info!("Web interface started on port {}", config.scrubber.web_port);
    }

    // Run based on the command
    match &args.command {
        Commands::ShowCache { .. } => {
            // This case is handled above
            unreachable!("ShowCache command should have been handled earlier");
        }
        Commands::ShowRules => {
            // This case is handled above
            unreachable!("ShowRules command should have been handled earlier");
        }
        Commands::AddRule { .. } => {
            // This case is handled above
            unreachable!("AddRule command should have been handled earlier");
        }
        Commands::RemoveRule { .. } => {
            // This case is handled above
            unreachable!("RemoveRule command should have been handled earlier");
        }
        Commands::SetAnchor { .. } => {
            // This case is handled above
            unreachable!("SetAnchor command should have been handled earlier");
        }
        Commands::SetAnchorTimestamp { .. } => {
            // This case is handled above
            unreachable!("SetAnchorTimestamp command should have been handled earlier");
        }
        Commands::ShowRecentTracks { .. } => {
            // This case is handled above
            unreachable!("ShowRecentTracks command should have been handled earlier");
        }
        Commands::ClearSession => {
            // This case is handled above
            unreachable!("ClearSession command should have been handled earlier");
        }
        _ => {
            // For other commands, we need to acquire the lock
        }
    }

    let mut scrubber_guard = scrubber.lock().await;
    match &args.command {
        Commands::Run { .. } => {
            log::info!("Starting continuous monitoring mode");
            scrubber_guard.run().await?;
        }
        Commands::Once {
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
        Commands::LastN {
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
        Commands::Artist { name, album, .. } => {
            if let Some(album_name) = album {
                log::info!("Processing tracks for album '{album_name}' by artist '{name}'");
                scrubber_guard.process_album(name, album_name).await?;
            } else {
                log::info!("Processing all tracks for artist '{name}'");
                scrubber_guard.process_artist(name).await?;
            }
        }
        Commands::Web { .. } => {
            log::info!(
                "Starting web interface only mode on port {}",
                config.scrubber.web_port
            );
            log::info!(
                "Web interface available at: http://localhost:{}",
                config.scrubber.web_port
            );
            log::info!("Press Ctrl+C to stop");

            // The web interface is already started above if enable_web_interface is true
            // Wait for shutdown signal
            if let Err(e) = tokio::signal::ctrl_c().await {
                log::error!("Failed to listen for shutdown signal: {e}");
            }
            log::info!("Received shutdown signal, stopping web interface...");
        }
        Commands::ShowCache { .. } => {
            // This case is handled above
            unreachable!("ShowCache command should have been handled earlier");
        }
        Commands::ShowRules => {
            // This case is handled above
            unreachable!("ShowRules command should have been handled earlier");
        }
        Commands::AddRule { .. } => {
            // This case is handled above
            unreachable!("AddRule command should have been handled earlier");
        }
        Commands::RemoveRule { .. } => {
            // This case is handled above
            unreachable!("RemoveRule command should have been handled earlier");
        }
        Commands::SetAnchor { .. } => {
            // This case is handled above
            unreachable!("SetAnchor command should have been handled earlier");
        }
        Commands::SetAnchorTimestamp { .. } => {
            // This case is handled above
            unreachable!("SetAnchorTimestamp command should have been handled earlier");
        }
        Commands::ShowRecentTracks { .. } => {
            // This case is handled above
            unreachable!("ShowRecentTracks command should have been handled earlier");
        }
        Commands::RefreshCache { .. } => {
            // This case is handled above
            unreachable!("RefreshCache command should have been handled earlier");
        }
        Commands::ExtendCache { .. } => {
            // This case is handled above
            unreachable!("ExtendCache command should have been handled earlier");
        }
        Commands::ClearSession => {
            // This case is handled above
            unreachable!("ClearSession command should have been handled earlier");
        }
    }

    Ok(())
}
