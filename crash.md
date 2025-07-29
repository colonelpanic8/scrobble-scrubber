# Stack Overflow Crash in lastfm-edit Library

## Summary

The `lastfm-edit` library experiences consistent **stack overflows** when calling `get_album_tracks()` method, causing applications to crash with "fatal runtime error: stack overflow, aborting".

## Reproduction

### Environment
- Platform: Linux 6.15.4
- Rust toolchain: Standard stable
- Application: scrobble-scrubber using lastfm-edit v3.3.0

### Minimal Reproduction Steps

1. Create a `LastFmEditClientImpl` instance (authenticated)
2. Call `client.get_artist_albums_page(artist_name, page)` - **this works fine**
3. Call `client.get_album_tracks(album_name, artist_name)` - **this crashes with stack overflow**

### Test Case
```rust
// This works fine
let album_page = client.get_artist_albums_page("Radiohead", 1).await?;

// This crashes with stack overflow
for album in album_page.albums {
    let tracks = client.get_album_tracks(&album.name, "Radiohead").await?; // CRASH HERE
}
```

## Error Details

### Error Message
```
thread 'tokio-runtime-worker' has overflowed its stack
fatal runtime error: stack overflow, aborting
```

### Behavior Pattern
- **Consistent**: Happens 100% of the time when calling `get_album_tracks()`
- **Immediate**: Crash occurs immediately upon method call, not after processing
- **Universal**: Affects multiple different artists and albums (tested: Radiohead, others)
- **Method-specific**: Only `get_album_tracks()` crashes, other methods work fine

### Attempted Workarounds
1. **spawn_blocking**: Still crashed even when wrapped in `tokio::task::spawn_blocking`
2. **Separate runtime**: Creating new tokio runtime for the call still crashed
3. **Different artists/albums**: All tested combinations crashed
4. **Timeout wrapping**: Crashes before timeout can trigger

## Call Stack Context

The crash happens during the `get_album_tracks()` method call. Based on testing:

- **get_artist_albums_page()** ✅ Works perfectly
- **get_recent_scrobbles()** ✅ Works fine  
- **get_album_tracks()** ❌ Immediate stack overflow

## Impact

This makes the `get_album_tracks()` method completely unusable, as it crashes the entire application process. Applications must work around this by avoiding album track fetching entirely.

## Technical Analysis

The stack overflow suggests:
1. **Deep recursion** in the album track fetching logic
2. **Infinite recursion** due to circular references or retry logic
3. **Large recursive data structures** being built on the stack
4. **Nested async calls** that build up stack frames

Since the crash is immediate and consistent, it's likely a logic error rather than a data-dependent issue.

## Testing Command

A CLI test command was added to reproduce this issue:
```bash
cargo run -- track-cache load-artist --artist "Radiohead"
```

This will consistently crash when it reaches the first `get_album_tracks()` call.

## Expected Behavior

The `get_album_tracks()` method should return track information for the specified album without crashing, similar to how `get_artist_albums_page()` successfully returns album information.

## Additional Context

- The application has proper panic handling with `std::panic::catch_unwind()` but this doesn't help since it's a stack overflow (not a panic)
- Other lastfm-edit methods work reliably
- The issue appears to be in the core album track fetching logic, not in HTTP client or JSON parsing layers

This bug renders the `get_album_tracks()` functionality completely unusable in production applications.