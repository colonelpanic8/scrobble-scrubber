pub mod config;
pub mod openai_provider;
pub mod persistence;
pub mod rewrite;
pub mod rewrite_processor;
pub mod scrub_action_provider;
pub mod scrubber;
pub mod web_interface;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "scrobble-scrubber")]
#[command(about = "Automated Last.fm track monitoring and scrubbing system")]
pub struct Args {
    /// Configuration file path
    #[arg(short, long)]
    pub config: Option<String>,

    /// Path to state file for persistence
    #[arg(short, long)]
    pub state_file: Option<String>,

    /// Last.fm username
    #[arg(long)]
    pub lastfm_username: Option<String>,

    /// Last.fm password
    #[arg(long)]
    pub lastfm_password: Option<String>,

    /// Enable `OpenAI` provider
    #[arg(long)]
    pub enable_openai: bool,

    /// `OpenAI` API key
    #[arg(long)]
    pub openai_api_key: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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
