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

### How Rewrite Rules Work

Rewrite rules are the core pattern-based cleaning system in Scrobble Scrubber. They use regular expressions to find and replace problematic metadata patterns.

#### Rule Structure

Each rewrite rule can target any combination of four metadata fields:
- **Track Name** - Song titles
- **Artist Name** - Performing artist
- **Album Name** - Album titles
- **Album Artist Name** - Album-level artist attribution

**Important**: For a rule to apply, **ALL** specified patterns must match. If you define patterns for both artist and album, the track must match both patterns or the rule won't trigger.

#### Rule Format

Rules consist of **find/replace patterns** using rust regular expressions:

```
Find Pattern: ^(.+) - \d{4} Remaster$
Replace: $1
```

This rule finds tracks ending with "- [Year] Remaster" and replaces the entire title with just the captured song name.

#### Capture Groups Explained

Capture groups are the key to powerful rewrite rules - they let you extract and reuse parts of the matched text. They're created using parentheses `()` in your find pattern.

**Basic Numbered Groups:**
```
Find: ^(.+) - (\d{4}) Remaster$
Replace: $1 (originally from $2)
```
- `(.+)` captures the song title as group 1
- `(\d{4})` captures the year as group 2
- Input: "Hotel California - 1976 Remaster"
- Output: "Hotel California (originally from 1976)"

**Multiple Groups Example:**
```
Find: ^(.+) [Ff]eat\. (.+) - (.+)$
Replace: $3 by $1 featuring $2
```
- Group 1: Main artist
- Group 2: Featured artist
- Group 3: Song title
- Input: "Taylor Swift feat. Ed Sheeran - Everything Has Changed"
- Output: "Everything Has Changed by Taylor Swift featuring Ed Sheeran"

**Named Capture Groups:**
```
Find: ^(?P<artist>.+) - (?P<song>.+) \((?P<year>\d{4})\)$
Replace: ${song} by ${artist}
```
- Uses `(?P<name>...)` syntax for named groups
- Reference with `${name}` in replacement
- Input: "The Beatles - Hey Jude (1968)"
- Output: "Hey Jude by The Beatles"

**Escaping Special Characters:**
- Use `\$` for literal dollar signs
- Use `\{` and `\}` for literal braces
- Use `\\` for literal backslashes

#### Real Examples

**Remove Remaster Suffixes:**
- Find: `^(.+) - \d{4} Digital Remaster$`
- Replace: `$1`
- Input: "The Big Ship - 2004 Digital Remaster"
- Output: "The Big Ship"

**Normalize Featuring Formats:**
- Find: `(.+) [Ff]t\. (.+)`
- Replace: `$1 feat. $2`
- Input: "Artist ft. Other Artist"
- Output: "Artist feat. Other Artist"

**Complex Multi-Field Rule:**
```
Artist Name: ^Chris Thile$ → Chris Thile & Michael Daves
Album Name: Sleep With One Eye Open → Sleep With One Eye Open
Album Artist: .* → Chris Thile & Michael Daves
```

This rule demonstrates the "ALL patterns must match" requirement - it only applies when:
1. The artist is exactly "Chris Thile" AND
2. The album contains "Sleep With One Eye Open" AND
3. There is an album artist field (any value)

Only when all three conditions are met will the rule trigger and correct the collaboration attribution.

#### Key Features

- **All Patterns Must Match**: For multi-field rules, every specified pattern must match for the rule to apply
- **Whole String Replacement**: When a pattern matches, the entire field is replaced
- **30+ Default Rules**: Ships with comprehensive rules for common issues
- **Custom Rules**: Create your own rules through the GUI or configuration

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
