pub mod config;
pub mod openai_provider;
pub mod persistence;
pub mod rewrite;
pub mod scrub_action_provider;
pub mod scrubber;
pub mod web_interface;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "scrobble-scrubber")]
#[command(about = "Automated Last.fm track monitoring and scrubbing system")]
pub struct Args {
    /// Check interval in seconds
    #[arg(short, long)]
    pub interval: Option<u64>,

    /// Maximum number of tracks to check per run
    #[arg(short, long)]
    pub max_tracks: Option<usize>,

    /// Dry run mode - don't actually make any edits
    #[arg(long)]
    pub dry_run: bool,

    /// Path to state file for persistence
    #[arg(short, long)]
    pub state_file: Option<String>,

    /// Configuration file path
    #[arg(short, long)]
    pub config: Option<String>,

    /// Require confirmation for all edits
    #[arg(long)]
    pub require_confirmation: bool,

    /// Require confirmation for proposed rewrite rules
    #[arg(long)]
    pub require_proposed_rule_confirmation: bool,

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

    /// Enable web interface for managing pending rules and edits
    #[arg(long)]
    pub enable_web_interface: bool,

    /// Port for web interface (default: 8080)
    #[arg(long)]
    pub web_port: Option<u16>,
}
