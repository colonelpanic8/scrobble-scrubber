use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default system prompt for AI providers
pub const DEFAULT_CLAUDE_SYSTEM_PROMPT: &str = "You are a music metadata cleaning assistant with function calling tools available. You work alongside automated rewrite rules and have two main responsibilities:

1. SUGGEST IMMEDIATE CORRECTIONS for complex metadata issues
2. RECOMMEND NEW REWRITE RULES when you identify patterns that could be automated

AVAILABLE FUNCTIONS:
- suggest_track_edit: Propose immediate metadata corrections for this specific track
- no_action_needed: Indicate the metadata is already correct

WHEN TO SUGGEST IMMEDIATE CORRECTIONS (suggest_track_edit):
- Complex typos requiring musical knowledge to identify
- Album name corrections from compilations to original albums
- Artist name standardization (e.g. \"The Beatles\" vs \"Beatles\")
- Context-dependent punctuation/capitalization fixes
- Album artist corrections for compilations vs. regular albums
- Complex featuring/collaboration format restructuring
- Issues that don't match existing automated rule patterns

WHEN TO RECOMMEND NEW REWRITE RULES:
If you notice patterns that could be automated, mention in your reasoning:
\"PATTERN DETECTED: This issue could be handled by a rewrite rule like [pattern] → [replacement]\"

REWRITE RULE SYNTAX:
Rewrite rules support both regex and literal string replacement:

REGEX RULES (most common):
- Pattern: r\"(\\d{4}) Remaster\" → Replacement: \"$1 Version\" 
- Pattern: r\" - \\d{4} Remaster\" → Replacement: \"\" (removes suffix)
- Pattern: r\"(.+) ft\\. (.+)\" → Replacement: \"$1 feat. $2\" (capture groups)
- Pattern: r\"\\s+$\" → Replacement: \"\" (trim trailing whitespace)

LITERAL RULES (exact string matching):
- Pattern: \"feat.\" → Replacement: \"featuring\" (simple replacement)
- Pattern: \" ft. \" → Replacement: \" feat. \" (normalize featuring)

REGEX FLAGS (optional):
- 'i' = case insensitive
- 'w' = word boundaries (\\b...\\b)
- 's' = dot matches newline

FIELD TARGETS:
Rules can target: track_name, artist_name, album_name, album_artist_name

Examples of rule-worthy patterns:
- Consistent suffix patterns: r\"\\s*\\(Deluxe Edition\\)\" → \"\"
- Date removals: r\" - \\d{4} (Remaster|Version)\" → \"\"
- Featuring normalization: r\" ft\\. \" → \" feat. \"
- Whitespace cleanup: r\"\\s+\" → \" \"

GUIDELINES:
- Always use available functions - don't just provide text responses
- Check existing rewrite rules (provided in context) to avoid duplication
- Focus on issues requiring musical knowledge or complex judgment for immediate fixes
- Suggest new rules for any consistent patterns you identify
- Only suggest changes when confident they improve metadata quality
- Consider original album/single releases when correcting compilations

Help build a smarter cleaning system by identifying both immediate fixes AND patterns for future automation!";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobbleScrubberConfig {
    pub scrubber: ScrubberConfig,
    pub providers: ProvidersConfig,
    pub storage: StorageConfig,
    pub lastfm: LastFmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubberConfig {
    /// Check interval in seconds
    pub interval: u64,
    /// Maximum number of tracks to check per run
    pub max_tracks: u32,
    /// Dry run mode - don't actually make any edits
    pub dry_run: bool,
    /// Global setting to require confirmation for all edits
    pub require_confirmation: bool,
    /// Require confirmation for proposed rewrite rules (default: true)
    pub require_proposed_rule_confirmation: bool,
    /// Enable web interface for managing pending items
    pub enable_web_interface: bool,
    /// Port for web interface
    pub web_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    /// Enable rewrite rules provider
    pub enable_rewrite_rules: bool,
    /// Enable `OpenAI` provider
    pub enable_openai: bool,
    /// Enable HTTP provider
    pub enable_http: bool,
    /// `OpenAI` configuration
    pub openai: Option<OpenAIProviderConfig>,
    /// HTTP provider configuration
    pub http: Option<HttpProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIProviderConfig {
    /// `OpenAI` API key
    pub api_key: String,
    /// Model to use (defaults to gpt-4o)
    pub model: Option<String>,
    /// Custom system prompt
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpProviderConfig {
    /// HTTP endpoint URL
    pub endpoint_url: String,
    /// Request timeout in seconds
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to state file for persistence
    pub state_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastFmConfig {
    /// Last.fm username
    pub username: String,
    /// Last.fm password
    pub password: String,
    /// Base URL for Last.fm (defaults to <https://www.last.fm>)
    pub base_url: Option<String>,
}

impl Default for ScrubberConfig {
    fn default() -> Self {
        Self {
            interval: 300,
            max_tracks: 100,
            dry_run: false,
            require_confirmation: false,
            require_proposed_rule_confirmation: true,
            enable_web_interface: false,
            web_port: 8080,
        }
    }
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            enable_rewrite_rules: true,
            enable_openai: false,
            enable_http: false,
            openai: None,
            http: None,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            state_file: "scrobble_state.db".to_string(),
        }
    }
}

impl Default for ScrobbleScrubberConfig {
    fn default() -> Self {
        Self {
            scrubber: ScrubberConfig::default(),
            providers: ProvidersConfig::default(),
            storage: StorageConfig::default(),
            lastfm: LastFmConfig {
                username: String::new(),
                password: String::new(),
                base_url: None,
            },
        }
    }
}

impl ScrobbleScrubberConfig {
    /// Get default configuration file paths in order of preference
    /// Uses XDG Base Directory specification
    #[must_use]
    pub fn get_default_config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Current directory
        paths.push(PathBuf::from("scrobble-scrubber.toml"));
        paths.push(PathBuf::from("config/scrobble-scrubber.toml"));

        // XDG config directory
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("scrobble-scrubber").join("config.toml"));
            paths.push(
                config_dir
                    .join("scrobble-scrubber")
                    .join("scrobble-scrubber.toml"),
            );
        }

        // Legacy home directory location
        if let Some(home_dir) = dirs::home_dir() {
            paths.push(
                home_dir
                    .join(".config")
                    .join("scrobble-scrubber")
                    .join("config.toml"),
            );
            paths.push(home_dir.join(".scrobble-scrubber.toml"));
        }

        paths
    }

    /// Get the preferred configuration file path for creating new config files
    /// Returns the XDG config directory path
    #[must_use]
    pub fn get_preferred_config_path() -> Option<PathBuf> {
        dirs::config_dir()
            .map(|config_dir| config_dir.join("scrobble-scrubber").join("config.toml"))
    }

    /// Load configuration from multiple sources with priority:
    /// 1. Command line arguments (highest priority)
    /// 2. Environment variables
    /// 3. Configuration file
    /// 4. Defaults (lowest priority)
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_with_file::<&str>(None)
    }

    /// Load configuration with a specific config file
    pub fn load_with_file<P: AsRef<Path>>(config_file: Option<P>) -> Result<Self, ConfigError> {
        let mut builder = Config::builder();

        // Start with defaults
        builder = builder.add_source(Config::try_from(&Self::default())?);

        // Add config file if it exists
        if let Some(file_path) = config_file {
            if file_path.as_ref().exists() {
                builder = builder.add_source(File::from(file_path.as_ref()));
            }
        } else {
            // Try common config file locations in order of preference
            let config_paths = Self::get_default_config_paths();

            for config_path in config_paths {
                if config_path.exists() {
                    builder = builder.add_source(File::from(config_path));
                    break;
                }
            }
        }

        // Add environment variables with prefix
        builder = builder.add_source(
            Environment::with_prefix("SCROBBLE_SCRUBBER")
                .separator("_")
                .try_parsing(true),
        );

        builder.build()?.try_deserialize()
    }

    /// Load configuration from args with optional config file override
    pub fn load_from_args(args: &crate::Args) -> Result<Self, ConfigError> {
        let config = if let Some(config_path) = &args.config {
            Self::load_with_file(Some(config_path))?
        } else {
            Self::load()?
        };

        Ok(config.merge_args(args))
    }

    /// Merge command line arguments into the configuration
    #[must_use]
    pub fn merge_args(mut self, args: &crate::Args) -> Self {
        // Override with command line arguments if provided
        if let Some(interval) = args.interval {
            self.scrubber.interval = interval;
        }
        if let Some(max_tracks) = args.max_tracks {
            self.scrubber.max_tracks = max_tracks as u32;
        }
        if args.dry_run {
            self.scrubber.dry_run = true;
        }
        if args.require_confirmation {
            self.scrubber.require_confirmation = true;
        }
        if args.require_proposed_rule_confirmation {
            self.scrubber.require_proposed_rule_confirmation = true;
        }
        if args.enable_web_interface {
            self.scrubber.enable_web_interface = true;
        }
        if let Some(web_port) = args.web_port {
            self.scrubber.web_port = web_port;
        }
        if let Some(state_file) = &args.state_file {
            self.storage.state_file = state_file.clone();
        }
        if let Some(username) = &args.lastfm_username {
            self.lastfm.username = username.clone();
        }
        if let Some(password) = &args.lastfm_password {
            self.lastfm.password = password.clone();
        }
        if args.enable_openai {
            self.providers.enable_openai = true;
        }
        if let Some(api_key) = &args.openai_api_key {
            if self.providers.openai.is_none() {
                self.providers.openai = Some(OpenAIProviderConfig {
                    api_key: api_key.clone(),
                    model: None,
                    system_prompt: None,
                });
            } else {
                self.providers.openai.as_mut().unwrap().api_key = api_key.clone();
            }
        }

        self
    }

    /// Get the Last.fm base URL with fallback to default
    #[must_use]
    pub fn lastfm_base_url(&self) -> &str {
        self.lastfm
            .base_url
            .as_deref()
            .unwrap_or("https://www.last.fm")
    }
}
