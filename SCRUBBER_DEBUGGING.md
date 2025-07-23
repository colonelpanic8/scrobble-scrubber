y# Scrobble Scrubber Debugging Issue

## Problem
The scrubber processor is not working correctly in that  we can't observe processing a track and taking actio.

## Current State
- Successfully implemented debugging CLI commands:
  - `show-cache` - displays cached tracks
  - `show-rules` - displays active rewrite rules
  - `set-anchor --tracks N` - sets timestamp anchor back N tracks
  - `set-anchor-timestamp --timestamp "2025-07-22T07:07:00Z"` - sets anchor to specific timestamp
- Trace logging shows detailed rule evaluation working correctly
- 4 active rewrite rules for removing "Remaster" patterns from track/album names

## Issue Discovered
Beatles tracks with "Remastered 2009" in names (at timestamps like `2025-07-22 07:08:12 UTC`) don't appear to get processed even when we set the anchor before that time. This suggests that maybe there is some issue with the way we feed tracks through to the action proposers or the way we batch tracks up or the way we advance the timestamp.
 
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


## Expected Behavior
Should see trace logaas showing rules applying to Beatles tracks with patterns like:
- Track name: "Cry Baby Cry - Remastered 2009" should match pattern `(.+) -.*Remaster.*` → "Cry Baby Cry"

## Next Steps
* Try to log all tracks as they come in, make sure they are all processed.
* Try to get something to 'happen' in a dry run
