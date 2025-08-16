# Scrobble Scrubber

[![CI](https://github.com/colonelpanic8/scrobble-scrubber/actions/workflows/ci.yml/badge.svg)](https://github.com/colonelpanic8/scrobble-scrubber/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/scrobble-scrubber.svg)](https://crates.io/crates/scrobble-scrubber)
[![docs.rs](https://docs.rs/scrobble-scrubber/badge.svg)](https://docs.rs/scrobble-scrubber)

Automated Last.fm scrobble monitoring and correction tool that continuously monitors your recent tracks and applies intelligent cleaning rules to fix metadata issues.

## Features

- **🔄 Continuous Monitoring**: Automatically polls your recent tracks at configurable intervals
- **💾 Smart State Management**: Tracks processed scrobbles to avoid duplicate corrections
- **🧹 Multiple Cleaning Providers**:
  - **Pattern-Based Rules**: Lightning-fast regex-based cleaning for common issues
  - **MusicBrainz Integration**: Validates and corrects metadata against the MusicBrainz database
  - **AI-Powered Cleaning**: OpenAI integration for complex metadata issues requiring musical context
  - **Compilation Detection**: Intelligently moves tracks from compilations to original albums
- **🎯 Self-Improving System**: AI provider identifies patterns for new automated rules
- **🔍 Dry Run Mode**: Preview changes before applying them to your scrobbles
- **⚙️ Flexible Configuration**: Supports environment variables, config files, and CLI arguments
- **🖥️ Interactive TUI**: Terminal user interface for managing rules and monitoring corrections

## Quick Start

### Installation

```bash
# Install from crates.io
cargo install scrobble-scrubber

# Or build from source
git clone https://github.com/colonelpanic8/scrobble-scrubber
cd scrobble-scrubber
cargo build --release
```

### Basic Setup

```bash
# Required: Last.fm credentials
export SCROBBLE_SCRUBBER_LASTFM_USERNAME="your_username"
export SCROBBLE_SCRUBBER_LASTFM_PASSWORD="your_password"

# Run the scrubber (checks every 5 minutes by default)
scrobble-scrubber

# Or with custom settings
scrobble-scrubber --interval 600 --dry-run
```

## Command Line Options

```
scrobble-scrubber [OPTIONS]

Options:
  -i, --interval <SECONDS>     Check interval in seconds [default: 300]
  -m, --max-tracks <NUMBER>    Maximum tracks to process per run [default: 100]
  --dry-run                    Preview changes without applying them
  --config <PATH>              Path to configuration file
  -h, --help                   Print help information
  -V, --version                Print version information
```

## Configuration

### Environment Variables

All environment variables use the `SCROBBLE_SCRUBBER_` prefix:

#### Required
```bash
# Last.fm credentials
export SCROBBLE_SCRUBBER_LASTFM_USERNAME="your_lastfm_username"
export SCROBBLE_SCRUBBER_LASTFM_PASSWORD="your_lastfm_password"
```

#### Optional Providers

**OpenAI Integration**
```bash
# Enable OpenAI provider for AI-powered metadata cleaning
export SCROBBLE_SCRUBBER_PROVIDERS_ENABLE_OPENAI=true
export SCROBBLE_SCRUBBER_PROVIDERS_OPENAI_API_KEY="sk-..."

# Optional: Specify model (defaults to gpt-4o)
export SCROBBLE_SCRUBBER_PROVIDERS_OPENAI_MODEL="gpt-4o-mini"
```

#### Scrubber Settings
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

#### Additional Settings
```bash
# Enable/disable providers
export SCROBBLE_SCRUBBER_PROVIDERS_ENABLE_REWRITE_RULES=true
export SCROBBLE_SCRUBBER_PROVIDERS_ENABLE_HTTP=false

# Storage configuration
export SCROBBLE_SCRUBBER_STORAGE_STATE_FILE="scrobble_state.db"

# Last.fm base URL (optional, defaults to https://www.last.fm)
export SCROBBLE_SCRUBBER_LASTFM_BASE_URL="https://www.last.fm"
```

### Configuration Files

Configuration files are searched for in this order:
1. `./scrobble-scrubber.toml` (current directory)
2. `./config/scrobble-scrubber.toml` (config subdirectory)
3. `$XDG_CONFIG_HOME/scrobble-scrubber/config.toml` (typically `~/.config/scrobble-scrubber/config.toml`)
4. `$XDG_CONFIG_HOME/scrobble-scrubber/scrobble-scrubber.toml`
5. `~/.config/scrobble-scrubber/config.toml` (legacy fallback)
6. `~/.scrobble-scrubber.toml` (legacy fallback)

**Priority Order** (highest to lowest):
1. Command line arguments
2. Environment variables
3. Configuration file
4. Built-in defaults

See [`config.example.toml`](config.example.toml) for a complete configuration example.

## How It Works

### Cleaning Providers

#### 1. Pattern-Based Rules
Fast regex-based cleaning for common issues:
- **Remaster suffixes**: `(2019 Remaster)`, `- Remastered`, etc.
- **Featuring formats**: Normalizes `ft.`, `featuring`, `feat.` variations
- **Whitespace**: Trims and normalizes spacing
- **Custom patterns**: User-defined regex rules with capture groups

#### 2. MusicBrainz Integration
Validates and corrects metadata against the MusicBrainz database:
- Album verification and correction
- Artist name standardization
- Track title validation
- Compilation to original album mapping

#### 3. AI-Powered Cleaning (Optional)
OpenAI integration for complex metadata issues:
- Context-aware typo correction
- Artist name disambiguation
- Complex collaboration formatting
- Pattern detection for new rules

## Development

### Building from Source

```bash
git clone https://github.com/colonelpanic8/scrobble-scrubber
cd scrobble-scrubber

# Build with default features
cargo build --release

# Build with all features including OpenAI
cargo build --release --features full

# Run tests
cargo test
```

### Architecture

- **`lastfm-edit`**: Core library for Last.fm API interactions
- **Provider System**: Modular architecture for adding cleaning providers
- **State Management**: SQLite-based tracking of processed scrobbles
- **Action System**: Type-safe representation of metadata modifications

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT License - see [LICENSE](LICENSE) file for details
