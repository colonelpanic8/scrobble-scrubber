use crate::track_cache::TrackCache;
use lastfm_edit::{LastFmEditClient, LastFmEditClientImpl, LastFmError, Result};

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

    // Calculate educated guess for starting page based on cache size
    let estimated_start_page = if initial_count > 0 {
        ((initial_count / 50) + 1) as u32 // 50 tracks per page, start from next estimated page
    } else {
        1u32
    };

    let mut recent_iterator = if let Some(oldest_time) = oldest_cached {
        println!("Starting search from estimated page {estimated_start_page} (based on {initial_count} cached tracks)...");
        println!("Looking for tracks older than {oldest_time}...");

        // Start from our estimated page instead of page 1
        let mut search_page = estimated_start_page;

        loop {
            println!("Checking page {search_page} for continuation point...");
            let mut page_iterator = client.recent_tracks_from_page(search_page);
            let mut found_tracks_in_page = false;

            // Check this page for tracks older than our cache
            while let Some(track) = page_iterator.next().await? {
                found_tracks_in_page = true;
                if let Some(track_ts) = track.timestamp {
                    if let Some(track_time) = chrono::DateTime::from_timestamp(track_ts as i64, 0) {
                        if track_time < oldest_time {
                            // Found older tracks! Use iterator starting from this page
                            println!("Found continuation point at page {search_page}");
                            break;
                        }
                    }
                }
            }

            if !found_tracks_in_page {
                // No more tracks available
                println!("📄 No older tracks available");
                return Ok(());
            }

            // Check if we found older tracks on this page
            let page_iterator = client.recent_tracks_from_page(search_page);
            let mut found_older_track = false;

            // Quick check to see if this page has tracks older than our cache
            let mut temp_iterator = client.recent_tracks_from_page(search_page);
            while let Some(track) = temp_iterator.next().await? {
                if let Some(track_ts) = track.timestamp {
                    if let Some(track_time) = chrono::DateTime::from_timestamp(track_ts as i64, 0) {
                        if track_time < oldest_time {
                            found_older_track = true;
                            break;
                        }
                    }
                }
            }

            if found_older_track {
                // This page contains our continuation point
                break page_iterator;
            } else {
                // This page is still in our cached range, try next page
                search_page += 1;
                println!(
                    "Page {} still contains cached tracks, trying page {}...",
                    search_page - 1,
                    search_page
                );
            }
        }
    } else {
        println!("No cached data, starting from page 1...");
        client.recent_tracks_from_page(1u32)
    };

    let mut fetched_tracks = Vec::new();
    let mut current_page = 0;
    let mut new_tracks_found = 0;

    // Skip to where we left off (if we have cached data)
    if let Some(oldest_time) = oldest_cached {
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

/// Load tracks for a specific artist (for debugging artist track loading)
pub async fn load_artist_tracks_cli(
    client: &LastFmEditClientImpl,
    artist_name: &str,
) -> Result<()> {
    use chrono::DateTime;

    println!("🎨 Loading Artist Tracks from Last.fm API");
    println!("==========================================");
    println!("Artist: '{artist_name}'");
    println!();

    // Check cache first
    let mut cache = TrackCache::load();
    if let Some(cached_tracks) = cache.get_artist_tracks(artist_name) {
        println!(
            "📂 Found {} cached tracks for '{artist_name}':",
            cached_tracks.len()
        );
        for (i, track) in cached_tracks.iter().enumerate() {
            let timestamp = track
                .timestamp
                .map(|ts| {
                    DateTime::from_timestamp(ts as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "Invalid timestamp".to_string())
                })
                .unwrap_or_else(|| "No timestamp".to_string());

            println!(
                "  {}: '{}' by '{}' from '{}' [{}]",
                i + 1,
                track.name,
                track.artist,
                track.album.as_deref().unwrap_or("Unknown Album"),
                timestamp
            );
        }
        println!();
    }

    println!("🔄 Fetching fresh data from Last.fm API...");

    // Step 1: Fetch albums for the artist
    println!("Step 1: Fetching albums for artist '{artist_name}'");
    let mut albums = Vec::new();
    let mut page = 1;
    const MAX_PAGES: u32 = 10; // Reasonable limit

    loop {
        println!("  Fetching albums page {page}...");
        let album_page = match client.get_artist_albums_page(artist_name, page).await {
            Ok(page) => page,
            Err(e) => {
                println!("  ❌ Error fetching albums page {page}: {e}");
                return Err(e);
            }
        };

        if album_page.albums.is_empty() {
            println!("  📄 No more albums found");
            break;
        }

        println!(
            "  ✅ Found {} albums on page {page}",
            album_page.albums.len()
        );
        for album in &album_page.albums {
            println!("    - '{}'", album.name);
        }

        albums.extend(album_page.albums);

        if !album_page.has_next_page || page >= MAX_PAGES {
            if page >= MAX_PAGES {
                println!("  ⚠️ Reached maximum page limit ({MAX_PAGES})");
            }
            break;
        }

        page += 1;
    }

    println!("📀 Found {} total albums for '{artist_name}'", albums.len());
    println!();

    if albums.is_empty() {
        println!("❌ No albums found for artist '{artist_name}'");
        return Ok(());
    }

    // Step 2: Fetch tracks for each album
    println!("Step 2: Fetching tracks for each album");
    let mut all_tracks = Vec::new();

    // Process albums one by one to isolate stack overflow issues
    for (idx, album) in albums.iter().enumerate() {
        let album_name = &album.name;
        println!("  Album {}/{}: '{album_name}'", idx + 1, albums.len());

        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.get_album_tracks(album_name, artist_name),
        )
        .await
        {
            Ok(Ok(tracks)) => {
                println!("    ✅ Found {} tracks", tracks.len());
                for track in &tracks {
                    println!("      - '{}'", track.name);
                }
                all_tracks.extend(tracks);
            }
            Ok(Err(e)) => {
                println!("    ❌ Error fetching tracks: {e}");
            }
            Err(_) => {
                println!("    ⏱️ Timeout fetching tracks (30s limit exceeded)");
            }
        }
    }

    println!();
    println!("🎵 Total tracks collected: {}", all_tracks.len());

    if all_tracks.is_empty() {
        println!("❌ No tracks found for artist '{artist_name}'");
        return Ok(());
    }

    // Step 3: Cache the results
    println!("Step 3: Caching results");
    cache.cache_artist_tracks(artist_name.to_string(), all_tracks.clone());
    cache.save().map_err(|e| {
        lastfm_edit::LastFmError::Io(std::io::Error::other(format!("Failed to save cache: {e}")))
    })?;
    println!("💾 Cached {} tracks for '{artist_name}'", all_tracks.len());

    // Step 4: Display summary
    println!();
    println!("📊 Summary:");
    println!("  Albums processed: {}", albums.len());
    println!("  Total tracks found: {}", all_tracks.len());
    println!("  Tracks cached successfully: ✅");

    Ok(())
}

/// Show recent tracks directly from Last.fm API
pub async fn show_recent_tracks_from_api(
    client: &LastFmEditClientImpl,
    limit: usize,
) -> Result<()> {
    use chrono::DateTime;

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
