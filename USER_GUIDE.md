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

### First-Time Setup

1. **Configure your Last.fm credentials:**
   - Set environment variables:
     ```bash
     export SCROBBLE_SCRUBBER_LASTFM_USERNAME="your_username"
     export SCROBBLE_SCRUBBER_LASTFM_PASSWORD="your_password"
     ```
   - Or configure through the desktop app's Configuration page

2. **Start the application** - it will automatically log you in and begin monitoring your recent tracks

## Core Features

### What Scrobble Scrubber Does

The app continuously monitors your Last.fm recent tracks and applies cleaning rules to fix common metadata issues:

- **Removes remaster suffixes**: "Song - 2019 Remaster" becomes "Song"
- **Normalizes featuring formats**: "Artist ft. Other" becomes "Artist feat. Other"
- **Cleans whitespace and formatting**: Removes extra spaces, fixes capitalization
- **Fixes common metadata errors**: Corrects artist names, track titles, and album names

### Operating Modes

- **Automatic Mode**: Runs continuously, checking for new tracks every 5 minutes
- **Dry Run Mode**: Preview changes without actually modifying your scrobbles
- **Manual Mode**: Review and approve each change before it's applied

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

## Tips for Best Results

### Getting Started Tips

1. **Start with dry run mode** to see what changes would be made
2. **Review the default rules** to understand what will be cleaned
3. **Begin with a small time window** to test on recent tracks first
4. **Monitor the pending edits** regularly to catch any issues
