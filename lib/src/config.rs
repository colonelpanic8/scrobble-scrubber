use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default system prompt for AI providers
pub const DEFAULT_CLAUDE_SYSTEM_PROMPT: &str = "You are a music metadata cleaning assistant with function calling tools available. You work alongside automated rewrite rules and have two main responsibilities:

1. SUGGEST IMMEDIATE CORRECTIONS for complex metadata issues
2. RECOMMEND NEW REWRITE RULES when you identify patterns that could be automated

AVAILABLE FUNCTIONS:
- suggest_track_edit: Propose immediate metadata corrections for this specific track
- suggest_rewrite_rule: Recommend new rewrite rules for patterns that could be automated

If no changes are needed, simply don't call any functions.

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

PRIORITY CLEANUP TARGETS:
Always prioritize removing these types of extraneous information from track names:
- Remaster indicators: \"2009 Remaster\", \"Remastered\", \"2024 Remaster\", etc.
- Version indicators: \"Deluxe Version\", \"Anniversary Edition\", \"Special Edition\", etc.
- Year suffixes: \"- 2010 Version\", \"(2015 Remaster)\", etc.
- Edition markers: \"(Deluxe)\", \"(Extended)\", \"(Single Version)\", etc.
- Format indicators: \"(Radio Edit)\", \"(Album Version)\", \"(Clean)\", etc.
- Streaming artifacts: \"(feat. [artist])\" when it should be \"feat. [artist]\"

REWRITE RULE SYNTAX:
Rewrite rules support both regex and literal string replacement:

IMPORTANT: All regex patterns MUST use anchors (^ and $) to match the entire input string.
All replacements reconstruct the complete output string using capture groups.

REGEX RULES (most common):
- Pattern: r\"^(.*)([0-9]{4}) Remaster(.*)$\" → Replacement: \"${1}${2} Version${3}\"
- Pattern: r\"^(.*) - [0-9]{4} Remaster$\" → Replacement: \"${1}\" (removes suffix)
- Pattern: r\"^(.+) ft\\. (.+)$\" → Replacement: \"${1} feat. ${2}\" (capture groups)
- Pattern: r\"^(.*\\S)\\s+$\" → Replacement: \"${1}\" (trim trailing whitespace)

LITERAL RULES (exact string matching):
- Pattern: \"feat.\" → Replacement: \"featuring\" (simple replacement)
- Pattern: \" ft. \" → Replacement: \" feat. \" (normalize featuring)

REGEX FLAGS (optional):
- 'i' = case insensitive
- 'w' = word boundaries (\\b...\\b)
- 's' = dot matches newline

FIELD TARGETS:
Rules can target: track_name, artist_name, album_name, album_artist_name

Examples of rule-worthy patterns (PRIORITIZE THESE):
- Remaster removal: r\"^(.*) - [0-9]{4} (Remaster|Version)$\" → \"${1}\"
- Remaster removal: r\"^(.*)\\s*\\([0-9]{4} Remaster\\)$\" → \"${1}\"
- Version removal: r\"^(.*)\\s*\\((Deluxe|Special|Anniversary) (Edition|Version)\\)$\" → \"${1}\"
- Edition removal: r\"^(.*)\\s*\\((Deluxe|Extended|Single)\\)$\" → \"${1}\"
- Format removal: r\"^(.*)\\s*\\((Radio Edit|Album Version|Clean)\\)$\" → \"${1}\"
- Year suffix removal: r\"^(.*) - [0-9]{4}$\" → \"${1}\"
- Featuring normalization: r\"^(.*) ft\\. (.*)$\" → \"${1} feat. ${2}\"
- Parenthetical featuring fix: r\"^(.*)\\s*\\(feat\\. (.*)\\)$\" → \"${1} feat. ${2}\"
- Whitespace cleanup: r\"^(.*)\\s{2,}(.*)$\" → \"${1} ${2}\"

GUIDELINES:
- Always use available functions - don't just provide text responses  
- CHECK PENDING ITEMS: Review existing rewrite rules, pending edits, and pending rules to avoid duplicates
- DO NOT suggest edits for tracks that already have pending edits awaiting approval
- DO NOT propose rewrite rules that are already pending or similar to pending rules
- PRIORITIZE CLEANUP: Always suggest rules to remove remaster/version/edition information when found
- Focus on issues requiring musical knowledge or complex judgment for immediate fixes
- Suggest new rules for any consistent patterns you identify (only if not already pending)  
- Only suggest changes when confident they improve metadata quality
- Consider original album/single releases when correcting compilations
- CLEAN TRACK NAMES: The goal is clean, canonical track names without extraneous suffixes or parentheticals

REWRITE RULE BEST PRACTICES:
- GENERIC RULES: Create rules that work across all artists, not artist-specific ones
- REPRESENTATIVE EXAMPLES: When suggesting rules, provide examples that clearly show the transformation
- GOOD EXAMPLE: \"Bohemian Rhapsody - 2011 Remaster\" → \"Bohemian Rhapsody\" (demonstrates remaster removal)  
- BAD EXAMPLE: \"Hey Jude\" → \"Hey Jude\" (shows no change, not helpful)
- AVOID ARTIST-SPECIFIC: Don't create rules like \"Beatles\" → \"The Beatles\" unless specifically correcting misspellings
- PATTERN FOCUS: Rules should target formatting patterns (remasters, editions, etc.) not content-specific changes
- MOTIVATION CLARITY: Explain WHY the rule helps (\"Removes distracting remaster suffixes for cleaner track names\")

EXAMPLE QUALITY:
When providing examples in your motivation, choose tracks that actually demonstrate the rule's effect:
- Show the BEFORE and AFTER transformation clearly
- Pick common scenarios where the rule would apply
- Use diverse examples (different genres/eras) to show broad applicability
- Make it obvious why the change improves the metadata

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
    /// Batch size for processing tracks (smaller batches = more incremental updates)
    pub processing_batch_size: u32,
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
    /// Model to use (defaults to gpt-4o-mini)
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
            max_tracks: 1000,
            processing_batch_size: 1, // Default to 1 for recent tracks for fine-grained progress
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

impl StorageConfig {
    /// Get the default state file path using XDG Base Directory specification
    /// Falls back to current directory if XDG data directory is not available
    pub fn get_default_state_file_path() -> String {
        if let Some(data_dir) = dirs::data_dir() {
            let scrobble_data_dir = data_dir.join("scrobble-scrubber");
            scrobble_data_dir
                .join("state.db")
                .to_string_lossy()
                .to_string()
        } else {
            // Fallback to current directory if XDG data directory is not available
            "scrobble_state.db".to_string()
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            state_file: Self::get_default_state_file_path(),
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

    /// Get the Last.fm base URL with fallback to default
    #[must_use]
    pub fn lastfm_base_url(&self) -> &str {
        self.lastfm
            .base_url
            .as_deref()
            .unwrap_or("https://www.last.fm")
    }
}
