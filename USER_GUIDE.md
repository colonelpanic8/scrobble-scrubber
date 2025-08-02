# Scrobble Scrubber User Guide

Scrobble Scrubber is desktop application that helps correct errors and other
annoying mislabelings in last.fm scrobble data.

## Getting Started

### Prerequisites

- A Last.fm Pro subscription (required for editing scrobbles)

### Installation

1. **Download the latest release:**
   - Visit the [releases page](https://github.com/colonelpanic8/scrobble-scrubber/releases)
   - Download the appropriate version for your operating system

## Core Features

### What Scrobble Scrubber Does

Scrobble Scrubber offers two main ways to clean your Last.fm metadata:

- **Continuous Monitoring**: Automatically monitors your recent tracks and applies cleaning rules in real-time
- **Ad-hoc Processing**: Manually process specific tracks, time periods, or your entire library on-demand

The app applies intelligent cleaning rules to fix common metadata issues:

**Examples of metadata corrections include:**

- **Remove remaster suffixes**: "Song - 2019 Remaster" → "Song"
- **Remove version indicators**: "Song (Single Version)" → "Song"
- **Remove audio format labels**: "Song (Stereo)" → "Song"
- **Normalize featuring formats**: "Artist ft. Other" → "Artist feat. Other"
- **Clean whitespace and formatting**: Remove extra spaces, fix capitalization
- **Correct artist attribution**: Fix cases where Spotify scrobbles multi-artist tracks to only the first artist

### Tag Correction Methods

Scrobble Scrubber uses multiple approaches to clean your metadata, each with different strengths:

#### Rewrite Rules
Fast, pattern-based corrections using regular expressions and literal text replacements. Perfect for systematic issues like remaster suffixes, featuring format inconsistencies, and whitespace problems. Ships with 30+ default rules and supports custom rule creation.

#### MusicBrainz Provider
Leverages the comprehensive MusicBrainz database to correct track metadata using verified release information. Ideal for fixing incorrect track titles, artist names, and album information by matching against the world's largest music database.

#### LLM Provider (AI-Powered)
Uses large language models (like OpenAI's GPT) to handle complex metadata issues requiring musical knowledge and context. Can identify subtle problems, suggest new rewrite rules, and handle edge cases that pattern-based rules miss. More expensive but highly effective for nuanced corrections.

## Rewrite Rules

Rewrite rules are the core pattern-based cleaning system in Scrobble Scrubber. They use regular expressions to find and replace problematic metadata patterns.

**For complete documentation on creating and using rewrite rules, see the [Rewrite Rules Guide](REWRITE_RULES.md).**

### Quick Overview

- **Target any metadata field**: Track name, artist name, album name, or album artist
- **Pattern-based matching**: Use regular expressions to find specific text patterns
- **Capture groups**: Extract and reuse parts of the matched text with `$1`, `$2`, etc.
- **Multi-field rules**: Combine patterns across fields (ALL must match for rule to apply)
- **30+ default rules**: Ships with comprehensive rules for common issues

### Common Examples

- Remove remaster suffixes: `"Song - 2019 Remaster"` → `"Song"`
- Normalize featuring: `"Artist ft. Other"` → `"Artist feat. Other"`
- Clean whitespace and formatting issues
- Fix artist attribution problems

## Desktop App Guide

### Main Pages

#### Scrubber Page
Your main dashboard for monitoring and controlling the cleaning process:
- **Start/Stop** the automatic scrubber
- View **processing status** and recent activity
- Monitor **progress** and track counts
- Toggle **dry run mode** for testing

#### Configuration Page
Central hub for all settings:
- **Last.fm Authentication**: Enter your username and password
- **Scrubber Settings**: Adjust check intervals and processing limits
- **Provider Settings**: Configure different cleaning methods
- **Dry Run Toggle**: Enable/disable preview mode
- **Important**: Always click **Save** after making changes to apply your configuration

#### Rewrite Rules Page
Manage the pattern-based cleaning rules:
- View and edit **active rules**
- **Enable/disable** specific rules
- Create **custom rules** for your specific needs
- Import/export rule sets

#### Pending Edits Page
Review changes before they're applied:
- See **proposed edits** to your tracks
- **Approve or reject** individual changes
- Bulk approve multiple edits
- View edit history

### Basic Usage Workflow

1. **Configure your credentials** on the Configuration page (don't forget to click **Save**)
2. **Start the scrubber** from the Scrubber page
3. **Monitor progress** as it processes your tracks
4. **Review pending edits** and approve changes you want
5. **Customize rules** as needed for your music library

## Configuration Options

### Authentication Settings

- **Last.fm Username**: Your Last.fm account username
- **Last.fm Password**: Your Last.fm account password
- **Auto-login**: Automatically log in when the app starts

### Scrubber Behavior

- **Check Interval**: How often to check for new tracks (default: 5 minutes)
- **Max Tracks per Run**: Maximum tracks to process at once (default: 100)
- **Require Confirmation**: Require manual approval before applying edits
- **Auto-start**: Automatically start scrubbing when the app launches

### Cleaning Providers

#### Rewrite Rules Provider (Default)
Fast pattern-based cleaning using predefined rules:
- **Enabled by default** with comprehensive rule set
- Handles common issues like remasters, featuring formats, whitespace
- **Customizable** - add your own rules or modify existing ones

#### OpenAI Provider (Optional)
AI-powered cleaning for complex metadata issues:
- **Requires OpenAI API key**
- Configure model
- Handles complex cases requiring musical knowledge
- More expensive but very effective

#### MusicBrainz Provider (Advanced)
Connect to the musicbrainz API to correct tags

### Storage Settings

- **State File Location**: Where to store processing history
- **Cache Management**: Control cached track data
- **Per-user Data**: Isolate data between different users
