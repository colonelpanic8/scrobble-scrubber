# Scrobble Scrubber Debugging Issue

## Problem
The scrubber processor appears to be working correctly (evaluating rewrite rules) but we can't observe it actually applying rules because the test data (Beatles "Remastered 2009" tracks) has moved out of the recent tracks window.

## Current State
- Successfully implemented debugging CLI commands:
  - `show-cache` - displays cached tracks
  - `show-rules` - displays active rewrite rules
  - `set-anchor --tracks N` - sets timestamp anchor back N tracks
  - `set-anchor-timestamp --timestamp "2025-07-22T07:07:00Z"` - sets anchor to specific timestamp
- Trace logging shows detailed rule evaluation working correctly
- 4 active rewrite rules for removing "Remaster" patterns from track/album names

## Issue Discovered
Beatles tracks with "Remastered 2009" in names (at timestamps like `2025-07-22 07:08:12 UTC`) are no longer in Last.fm's recent tracks API window. When running `once` command, only tracks from current day are found, even with timestamp anchor set to yesterday.

## Test Data Location
```bash
# These tracks should trigger rewrite rules:
'Cry Baby Cry - Remastered 2009' by 'The Beatles' [2025-07-22 07:11:25 UTC]
'Sexy Sadie - Remastered 2009' by 'The Beatles' [2025-07-22 07:08:12 UTC]
```

## Debugging Commands
```bash
# Show rules that should match the pattern
cargo run -p scrobble-scrubber-cli -- show-rules

# Set anchor to specific timestamp  
cargo run -p scrobble-scrubber-cli -- set-anchor-timestamp --timestamp "2025-07-22T07:07:00Z"

# Run with trace logging to see rule evaluation
RUST_LOG=scrobble_scrubber=trace cargo run -p scrobble-scrubber-cli -- once --dry-run --max-tracks 10

# Alternative: target specific artist
RUST_LOG=scrobble_scrubber=trace cargo run -p scrobble-scrubber-cli -- artist --name "The Beatles" --dry-run
```

## Expected Behavior
Should see trace logs showing rules applying to Beatles tracks with patterns like:
- Track name: "Cry Baby Cry - Remastered 2009" should match pattern `(.+) -.*Remaster.*` → "Cry Baby Cry"

## Next Steps
Need to either:
1. Use `artist` command to process Beatles tracks specifically, or
2. Find tracks that contain "remaster" patterns within current API window, or  
3. Investigate why `once` command with timestamp anchor isn't reaching older tracks