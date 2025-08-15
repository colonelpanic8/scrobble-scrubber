use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::events::ScrubberEventType;
use scrobble_scrubber::musicbrainz::CompilationToCanonicalProvider;
use scrobble_scrubber::persistence::MemoryStorage;
use scrobble_scrubber::scrub_action_provider::OrScrubActionProvider;
use scrobble_scrubber::scrubber::ScrobbleScrubber;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::common::create_lastfm_vcr_client;

#[test_log::test(tokio::test)]
#[ignore]
async fn beatles_artist_compilation_renaming() {
    // Create VCR client for recording/replaying Beatles artist tracks from Last.fm
    let lastfm_client = create_lastfm_vcr_client("beatles_full_artist_processing")
        .await
        .expect("Failed to create VCR test client");

    // Set up storage
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));

    // Create provider chain with CompilationToCanonicalProvider
    let mut action_provider = OrScrubActionProvider::new();
    let compilation_provider = CompilationToCanonicalProvider::new();
    action_provider = action_provider.add_provider(compilation_provider);

    // Create configuration with dry_run ENABLED (we just want to see suggestions)
    let mut config = ScrobbleScrubberConfig::default();
    config.scrubber.dry_run = true; // DRY RUN - no actual edits
    config.scrubber.require_confirmation = false;

    log::info!("Starting Beatles artist dry run processing with CompilationToCanonicalProvider");

    // Create scrubber
    let mut scrubber = ScrobbleScrubber::with_direct_provider(
        storage.clone(),
        lastfm_client,
        action_provider,
        config,
    );

    // Subscribe to events to track suggestions
    let mut event_receiver = scrubber.subscribe_events();

    // Spawn a task to collect suggestion events
    let suggestions = Arc::new(Mutex::new(Vec::new()));
    let suggestions_clone = suggestions.clone();
    let skipped_tracks = Arc::new(Mutex::new(Vec::new()));
    let skipped_tracks_clone = skipped_tracks.clone();

    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            match &event.event_type {
                ScrubberEventType::TrackProcessed {
                    track,
                    suggestions: track_suggestions,
                    ..
                } => {
                    if !track_suggestions.is_empty() {
                        log::info!(
                            "Suggestions for '{}' by '{}' (album: {:?}): {} suggestions",
                            track.name,
                            track.artist,
                            track.album,
                            track_suggestions.len()
                        );

                        for suggestion in track_suggestions {
                            if let scrobble_scrubber::scrub_action_provider::ScrubActionSuggestion::Edit(edit) = suggestion {
                                if let Some(new_album) = &edit.album_name {
                                    log::info!("  -> Suggest moving to album: '{new_album}'");
                                    suggestions_clone
                                        .lock()
                                        .await
                                        .push((track.clone(), edit.clone()));
                                }
                            }
                        }
                    }
                }
                ScrubberEventType::TrackSkipped { track, reason, .. } => {
                    log::debug!(
                        "Track skipped: '{}' by '{}' - Reason: {}",
                        track.name,
                        track.artist,
                        reason
                    );
                    skipped_tracks_clone
                        .lock()
                        .await
                        .push((track.clone(), reason.clone()));
                }
                _ => {}
            }
        }
    });

    // Process all tracks for The Beatles artist
    log::info!("Running artist processing for The Beatles");

    let result = scrubber.process_artist("The Beatles").await;

    // Log result but don't fail the test if no tracks are found
    match result {
        Ok(()) => log::info!("Artist processing completed successfully"),
        Err(e) => log::warn!("Artist processing completed with error: {e:?}"),
    }

    // Give event handler time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Analyze results
    let all_suggestions = suggestions.lock().await;
    let all_skipped = skipped_tracks.lock().await;

    log::info!("\n=== DRY RUN RESULTS ===");
    log::info!("Total suggestions: {}", all_suggestions.len());
    log::info!("Total skipped tracks: {}", all_skipped.len());

    // Group suggestions by original album
    let mut suggestions_by_album: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut bootleg_suggestions = Vec::new();

    for (track, edit) in all_suggestions.iter() {
        let none_string = "<none>".to_string();
        let original_album = track.album.as_ref().unwrap_or(&none_string);
        let new_album = edit
            .album_name
            .as_ref()
            .unwrap_or(track.album.as_ref().unwrap_or(&none_string));

        // Track the suggestion
        suggestions_by_album
            .entry(original_album.clone())
            .or_default()
            .push((track.name.clone(), new_album.clone()));

        // Check for bootlegs (should be none!)
        let new_album_lower = new_album.to_lowercase();
        if new_album_lower.contains("bootleg")
            || new_album_lower.contains("working version")
            || new_album_lower.contains("outtake")
            || new_album_lower.contains("demo")
            || new_album_lower.contains("sessions")
            || new_album_lower.contains("unreleased")
            || new_album_lower.contains("get back .... continued")
            || new_album_lower.contains("anthology") && new_album_lower.contains("unreleased")
        {
            bootleg_suggestions.push(format!(
                "'{}' by '{}': {} -> {}",
                track.name, track.artist, original_album, new_album
            ));
        }
    }

    // Report by album
    if !suggestions_by_album.is_empty() {
        log::info!("\n=== Suggestions by Original Album ===");
        for (album, tracks) in &suggestions_by_album {
            log::info!("Album '{}': {} suggestions", album, tracks.len());
            for (track_name, new_album) in tracks {
                log::info!("  - '{track_name}' -> '{new_album}'");
            }
        }
    }

    // Check for Beatles tracks specifically
    let beatles_suggestions: Vec<_> = all_suggestions
        .iter()
        .filter(|(track, _)| {
            track.artist.eq_ignore_ascii_case("The Beatles")
                || track.artist.eq_ignore_ascii_case("Beatles")
        })
        .collect();

    if !beatles_suggestions.is_empty() {
        log::info!("\n=== Beatles-specific Suggestions ===");
        log::info!(
            "Found {} Beatles track suggestions",
            beatles_suggestions.len()
        );

        // Verify compilation handling
        for (track, _edit) in &beatles_suggestions {
            let none_string = "<none>".to_string();
            let original_album = track.album.as_ref().unwrap_or(&none_string);
            let original_lower = original_album.to_lowercase();

            // Check if it's from a known compilation
            if original_lower.contains("greatest")
                || original_lower.contains("1967-1970")
                || original_lower.contains("1962-1966")
                || original_lower == "1"
                || original_lower.contains("collection")
                || original_lower.contains("best of")
            {
                log::info!(
                    "✓ Compilation album '{}' got suggestion for track '{}'",
                    original_album,
                    track.name
                );
            }
        }
    } else {
        log::info!("\n=== No Beatles tracks found ===");
        log::info!("The VCR cassette will record The Beatles artist tracks from Last.fm");
        log::info!(
            "If no tracks are found, ensure The Beatles have scrobbled tracks in your Last.fm account"
        );
    }

    // CRITICAL: Verify no bootleg suggestions
    assert!(
        bootleg_suggestions.is_empty(),
        "Found {} bootleg suggestions (should be 0):\n{}",
        bootleg_suggestions.len(),
        bootleg_suggestions.join("\n")
    );

    log::info!("\n✅ SUCCESS: Beatles artist dry run processing completed!");
    log::info!("CompilationToCanonicalProvider processed Beatles tracks correctly");
}

#[test_log::test(tokio::test)]
async fn log_all_beatles_tracks() {
    // Create VCR client for recording/replaying Beatles artist tracks from Last.fm
    let lastfm_client = create_lastfm_vcr_client("beatles_full_artist_processing")
        .await
        .expect("Failed to create VCR test client");

    // Set up storage
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));

    // Create minimal provider (we don't need suggestions, just want to fetch tracks)
    let action_provider = OrScrubActionProvider::new();

    // Create configuration with dry_run ENABLED (no edits needed)
    let mut config = ScrobbleScrubberConfig::default();
    config.scrubber.dry_run = true;
    config.scrubber.require_confirmation = false;

    log::info!("Fetching all Beatles tracks from Last.fm");

    // Create scrubber
    let mut scrubber = ScrobbleScrubber::with_direct_provider(
        storage.clone(),
        lastfm_client,
        action_provider,
        config,
    );

    // Subscribe to events to collect all tracks
    let mut event_receiver = scrubber.subscribe_events();

    // Collect all tracks
    let all_tracks = Arc::new(Mutex::new(Vec::new()));
    let all_tracks_clone = all_tracks.clone();

    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            match &event.event_type {
                ScrubberEventType::TrackProcessed { track, .. } => {
                    all_tracks_clone.lock().await.push(track.clone());
                }
                ScrubberEventType::TrackSkipped { track, .. } => {
                    all_tracks_clone.lock().await.push(track.clone());
                }
                _ => {}
            }
        }
    });

    // Process all tracks for The Beatles artist
    let _ = scrubber.process_artist("The Beatles").await;

    // Give event handler time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Get all collected tracks
    let mut tracks = all_tracks.lock().await.clone();

    log::info!("\n=== ALL BEATLES TRACKS (sorted by play count) ===");
    log::info!("Total tracks found: {}", tracks.len());

    // Sort tracks by play count (descending)
    tracks.sort_by(|a, b| b.playcount.cmp(&a.playcount));

    // Log each track on one line with album
    for track in tracks.iter() {
        let album = track.album.as_deref().unwrap_or("<No Album>");
        log::info!(
            "[{}] \"{}\" - \"{}\" - \"{}\"",
            track.playcount,
            track.name,
            track.artist,
            album
        );
    }
}
