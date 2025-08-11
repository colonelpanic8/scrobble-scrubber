# Scrobble Scrubber

[![CI](https://github.com/colonelpanic8/scrobble-scrubber/actions/workflows/ci.yml/badge.svg)](https://github.com/imalison/scrobble-scrubber/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/scrobble-scrubber.svg)](https://crates.io/crates/scrobble-scrubber)
[![docs.rs](https://docs.rs/scrobble-scrubber/badge.svg)](https://docs.rs/scrobble-scrubber)

> **📖 For end users:** See the [**User Guide**](USER_GUIDE.md) for installation and usage instructions.

Automated Last.fm track monitoring and scrubbing system that continuously monitors your recent tracks and applies cleaning rules to fix common issues.

## Features

- **Continuous Monitoring**: Polls your recent tracks at configurable intervals
- **State Management**: Remembers which tracks have been processed to avoid duplicates
- **Multiple Cleaning Providers**:
  - **Rewrite Rules**: Fast pattern-based cleaning (removes remaster suffixes, normalizes featuring formats, trims whitespace)
  - **OpenAI Provider**: AI-powered metadata cleaning for complex issues requiring musical knowledge
  - **MusicBrainze Provider**: Uses musicbrainz release library to correct metadata
- **Self-Improving AI Integration**: The AI provider not only handles complex metadata issues but also identifies patterns for new automated rules, creating a system that gets smarter over time
- **Dry Run Mode**: Test changes without actually modifying your scrobbles
- **Flexible Configuration**: Environment variables, config files, and command-line arguments

## Usage

Set up environment variables:
```bash
# Required: Last.fm credentials
export SCROBBLE_SCRUBBER_LASTFM_USERNAME="your_username"
export SCROBBLE_SCRUBBER_LASTFM_PASSWORD="your_password"

# Optional: OpenAI provider for AI-powered metadata cleaning
export SCROBBLE_SCRUBBER_PROVIDERS_ENABLE_OPENAI=true
export SCROBBLE_SCRUBBER_PROVIDERS_OPENAI_API_KEY="sk-..."
```

Run the scrubber:
```bash
# Basic usage (checks every 5 minutes)
cargo run

# Custom interval (check every 10 minutes)
cargo run -- --interval 600

# Dry run mode (see what would be changed)
cargo run -- --dry-run

# Limit tracks per cycle
cargo run -- --max-tracks 50
```

## Command Line Options

- `-i, --interval <SECONDS>`: Check interval in seconds (default: 300)
- `-m, --max-tracks <NUMBER>`: Maximum tracks to process per run (default: 100)
- `--dry-run`: Show what would be changed without making actual edits

## Environment Variables

All environment variables use the `SCROBBLE_SCRUBBER_` prefix and follow the configuration structure:

### Required Configuration
```bash
# Last.fm credentials
export SCROBBLE_SCRUBBER_LASTFM_USERNAME="your_lastfm_username"
export SCROBBLE_SCRUBBER_LASTFM_PASSWORD="your_lastfm_password"
```

### OpenAI Provider (Optional)
```bash
# Enable OpenAI provider for AI-powered metadata cleaning
export SCROBBLE_SCRUBBER_PROVIDERS_ENABLE_OPENAI=true
export SCROBBLE_SCRUBBER_PROVIDERS_OPENAI_API_KEY="sk-..."

# Optional: Specify model (defaults to gpt-4o)
export SCROBBLE_SCRUBBER_PROVIDERS_OPENAI_MODEL="gpt-4o-mini"
```

### Scrubber Settings
```bash
# Check interval in seconds (default: 300)
export SCROBBLE_SCRUBBER_SCRUBBER_INTERVAL=300

# Maximum tracks to process per run (default: 100)
export SCROBBLE_SCRUBBER_SCRUBBER_MAX_TRACKS=100

# Dry run mode (default: false)
export SCROBBLE_SCRUBBER_SCRUBBER_DRY_RUN=true

# Require confirmation for all edits (default: false)
export SCROBBLE_SCRUBBER_SCRUBBER_REQUIRE_CONFIRMATION=false
```

### Other Settings
```bash
# Enable/disable providers
export SCROBBLE_SCRUBBER_PROVIDERS_ENABLE_REWRITE_RULES=true
export SCROBBLE_SCRUBBER_PROVIDERS_ENABLE_HTTP=false

# Storage configuration
export SCROBBLE_SCRUBBER_STORAGE_STATE_FILE="scrobble_state.db"

# Last.fm base URL (optional, defaults to https://www.last.fm)
export SCROBBLE_SCRUBBER_LASTFM_BASE_URL="https://www.last.fm"
```

### Configuration File Locations

Configuration files are searched for in this order:
1. `./scrobble-scrubber.toml` (current directory)
2. `./config/scrobble-scrubber.toml` (config subdirectory)
3. `$XDG_CONFIG_HOME/scrobble-scrubber/config.toml` (typically `~/.config/scrobble-scrubber/config.toml`)
4. `$XDG_CONFIG_HOME/scrobble-scrubber/scrobble-scrubber.toml`
5. `~/.config/scrobble-scrubber/config.toml` (legacy fallback)
6. `~/.scrobble-scrubber.toml` (legacy fallback)

### Configuration Priority
Configuration is loaded in this order (highest priority first):
1. Command line arguments
2. Environment variables (with `SCROBBLE_SCRUBBER_` prefix)
3. Configuration file (first found from locations above)
4. Defaults

### Example Configuration File

See `config.example.toml` for a complete example configuration file with all available options and their defaults.

## Cleaning System

The scrobble scrubber uses a two-tier cleaning approach:

### 1. Automated Rewrite Rules (Fast Pattern Matching)
These rules use regex patterns and literal string replacement to handle common formatting issues:

**Remaster Removal**:
- `Song Name - 2019 Remaster` → `Song Name`
- `Song Name (Remaster 2019)` → `Song Name`
- `Song Name (Remaster)` → `Song Name`

**Featuring Normalization**:
- `Artist ft. Other` → `Artist feat. Other`
- `Artist featuring Other` → `Artist feat. Other`

**Whitespace Cleanup**:
- Removes trailing/leading spaces
- Normalizes multiple spaces

**Custom Pattern Rules**:
- Supports complex regex patterns with capture groups
- Can target specific fields (track name, artist name, album name, album artist)
- Configurable through JSON/TOML rule files

### 2. AI Provider (Complex Judgment-Based Cleaning)
When enabled, the OpenAI provider has two key functions:

**Immediate Corrections** for complex issues requiring musical knowledge:
- **Typo Correction**: Fixes spelling errors that don't match simple patterns
- **Album Disambiguation**: Changes compilation albums to original album names
- **Artist Standardization**: Resolves artist name variations (e.g., "The Beatles" vs "Beatles")
- **Compilation Metadata**: Corrects album artist fields for various artists compilations
- **Complex Collaborations**: Restructures complex featuring/collaboration formats
- **Context-Dependent Decisions**: Uses musical knowledge to make informed corrections

**Pattern Detection** for system improvement:
- **Rule Suggestions**: Identifies recurring patterns that could be automated with new rewrite rules
- **System Learning**: Helps evolve the automated rule system by spotting consistent issues
- **Efficiency Optimization**: Converts manual fixes into automated patterns when possible

The AI provider uses function calling to both fix immediate issues AND suggest improvements to the automated rule system, creating a self-improving metadata cleaning pipeline.

## Architecture

The scrubber uses the `lastfm-edit` library for all Last.fm interactions and implements:

1. **Track Iterator**: Uses `RecentTracksIterator` with timestamp-based stopping
2. **State Tracking**: Maintains a set of seen tracks to avoid reprocessing
3. **Rule Engine**: Modular system for adding new cleaning rules
4. **Action System**: Structured approach to track/artist modifications
# Linux app test
# macOS ARM app test
