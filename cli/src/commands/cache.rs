use lastfm_edit::{LastFmEditClient, LastFmEditClientImpl, LastFmError, Result};
use scrobble_scrubber::track_cache::TrackCache;

/// Show recent tracks cache state (track names, artists, timestamps)
pub fn show_cache_state(limit: usize, all_pages: bool) -> Result<()> {
    use chrono::DateTime;

    let cache = TrackCache::load();

    println!("📂 Track Cache State");
    println!("==================");

    // Show recent tracks
    let recent_tracks_count = cache.recent_tracks.len();

    println!("Recent Tracks:");
    println!("  {recent_tracks_count} tracks cached");

    if recent_tracks_count > 0 {
        let tracks = if all_pages {
            cache.get_all_recent_tracks()
        } else {
            cache.get_recent_tracks_limited(limit)
        };

        println!("  Showing {} tracks:", tracks.len().min(limit));
        for (i, track) in tracks.iter().take(limit).enumerate() {
            let timestamp = track
                .timestamp
                .map(|ts| {
                    DateTime::from_timestamp(ts as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                })
                .unwrap_or_else(|| "No timestamp".to_string());

            println!(
                "    {}: '{}' by '{}' [{}]",
                i + 1,
                track.name,
                track.artist,
                timestamp
            );
        }

        if tracks.len() > limit {
            println!("    ... and {} more tracks", tracks.len() - limit);
        }
    } else {
        println!("  No recent tracks cached");
    }

    // Show artist tracks
    let artist_tracks_count: usize = cache.artist_tracks.values().map(|v| v.len()).sum();
    let artists_count = cache.artist_tracks.len();

    println!("\nArtist Tracks:");
    println!("  {artists_count} artists cached, {artist_tracks_count} total tracks");

    if artists_count > 0 {
        for (artist, tracks) in &cache.artist_tracks {
            println!("    '{}': {} tracks", artist, tracks.len());
        }
    } else {
        println!("  No artist tracks cached");
    }

    Ok(())
}

/// Refresh track cache from Last.fm API (clear and reload)
pub async fn refresh_cache(client: &LastFmEditClientImpl, pages: usize) -> Result<()> {
    use lastfm_edit::AsyncPaginatedIterator;

    println!("🔄 Refreshing Track Cache");
    println!("========================");
    println!(
        "This will clear the existing cache and fetch {pages} page(s) of fresh data from Last.fm"
    );

    let mut cache = TrackCache::default(); // Start with empty cache
    let mut recent_iterator = client.recent_tracks();
    let mut fetched_tracks = Vec::new();
    let mut current_page = 0;

    println!("Fetching page {} from Last.fm API...", current_page + 1);

    while current_page < pages {
        let mut page_tracks = 0;

        // Fetch approximately 50 tracks per page (typical Last.fm page size)
        while page_tracks < 50 {
            if let Some(track) = recent_iterator.next().await? {
                fetched_tracks.push(track);
                page_tracks += 1;
            } else {
                println!(
                    "📄 Reached end of available tracks after {} tracks",
                    fetched_tracks.len()
                );
                break;
            }
        }

        current_page += 1;
        if current_page < pages && page_tracks > 0 {
            println!("Fetching page {} from Last.fm API...", current_page + 1);
        }

        // Break if we didn't get a full page (likely at end of data)
        if page_tracks < 50 {
            break;
        }
    }

    // Add all fetched tracks to the cache
    cache.add_recent_tracks(fetched_tracks);
    cache.save().map_err(|e| {
        LastFmError::Io(std::io::Error::other(format!("Failed to save cache: {e}")))
    })?;

    let stats = cache.stats();
    println!("✅ Cache refreshed successfully");
    println!(
        "  Fetched {} pages (~{} tracks)",
        current_page, stats.recent_track_count
    );
    println!("  Total tracks cached: {}", stats.recent_track_count);

    Ok(())
}

/// Extend track cache by fetching additional tracks
pub async fn extend_cache(client: &LastFmEditClientImpl, pages: usize) -> Result<()> {
    use lastfm_edit::AsyncPaginatedIterator;

    println!("📈 Extending Track Cache");
    println!("=======================");

    let mut cache = TrackCache::load();
    let initial_count = cache.stats().recent_track_count;

    println!("Current cache contains {initial_count} tracks");
    println!("Fetching {pages} additional page(s) from Last.fm API...");

    // Get the oldest timestamp to continue from where cache ends
    let oldest_cached = cache
        .recent_tracks
        .last()
        .and_then(|track| track.timestamp)
        .and_then(|ts| chrono::DateTime::from_timestamp(ts as i64, 0));

    let mut recent_iterator = client.recent_tracks();
    let mut fetched_tracks = Vec::new();
    let mut current_page = 0;
    let mut new_tracks_found = 0;

    // Skip to where we left off (if we have cached data)
    if let Some(oldest_time) = oldest_cached {
        println!("Skipping to tracks older than {oldest_time}...");
        loop {
            if let Some(track) = recent_iterator.next().await? {
                if let Some(track_ts) = track.timestamp {
                    if let Some(track_time) = chrono::DateTime::from_timestamp(track_ts as i64, 0) {
                        if track_time < oldest_time {
                            // Found older tracks, include this track and continue
                            fetched_tracks.push(track);
                            break;
                        }
                    }
                }
            } else {
                println!("📄 No older tracks available");
                return Ok(());
            }
        }
    }

    println!("Fetching page {} from Last.fm API...", current_page + 1);

    // Now fetch the requested number of pages
    while current_page < pages {
        let mut page_tracks = 0;

        // Fetch approximately 50 tracks per page
        while page_tracks < 50 {
            if let Some(track) = recent_iterator.next().await? {
                fetched_tracks.push(track);
                page_tracks += 1;
                new_tracks_found += 1;
            } else {
                println!("📄 Reached end of available tracks");
                break;
            }
        }

        current_page += 1;
        if current_page < pages && page_tracks > 0 {
            println!("Fetching page {} from Last.fm API...", current_page + 1);
        }

        // Break if we didn't get a full page (likely at end of data)
        if page_tracks < 50 {
            break;
        }
    }

    // Merge with existing cache
    let _stats = cache.merge_recent_tracks(fetched_tracks);
    cache.save().map_err(|e| {
        LastFmError::Io(std::io::Error::other(format!("Failed to save cache: {e}")))
    })?;

    let final_count = cache.stats().recent_track_count;
    let actual_added = final_count.saturating_sub(initial_count);

    println!("✅ Cache extended successfully");
    println!("  Fetched {current_page} pages (~{new_tracks_found} tracks)");
    println!("  Added {actual_added} new unique tracks");
    println!("  Total tracks cached: {final_count}");

    Ok(())
}

/// Show recent tracks directly from Last.fm API
pub async fn show_recent_tracks_from_api(
    client: &LastFmEditClientImpl,
    limit: usize,
) -> Result<()> {
    use chrono::DateTime;
    use lastfm_edit::AsyncPaginatedIterator;

    println!("🎵 Recent Tracks from Last.fm API");
    println!("=================================");

    let mut recent_iterator = client.recent_tracks();
    let mut count = 0;

    while let Some(track) = recent_iterator.next().await? {
        count += 1;

        let timestamp = if let Some(ts) = track.timestamp {
            DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "Invalid timestamp".to_string())
        } else {
            "No timestamp".to_string()
        };

        println!(
            "  {}: '{}' by '{}' [{}]",
            count, track.name, track.artist, timestamp
        );

        if count >= limit {
            break;
        }
    }

    if count == 0 {
        println!("  No recent tracks found");
    } else {
        println!("\nShowed {count} tracks from Last.fm API");
    }

    Ok(())
}
