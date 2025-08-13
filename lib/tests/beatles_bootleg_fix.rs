use lastfm_edit::Track;
use scrobble_scrubber::compilation_to_canonical_provider::CompilationToCanonicalProvider;
use scrobble_scrubber::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};

// Import the common test utilities
mod common;

// VCR test to verify we don't suggest bootlegs for Beatles tracks
mb_live_test!(
    async fn test_beatles_abbey_road_no_bootlegs() {

    let provider = CompilationToCanonicalProvider::new();

    // Test with Beatles tracks from Abbey Road that were previously suggesting bootlegs
    let tracks = vec![
        Track {
            name: "Come Together".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("Abbey Road".to_string()),
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000000),
            playcount: 1,
        },
        Track {
            name: "Something".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("Abbey Road".to_string()),
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000100),
            playcount: 1,
        },
        Track {
            name: "Maxwell's Silver Hammer".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("Abbey Road".to_string()),
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000200),
            playcount: 1,
        },
        Track {
            name: "Oh! Darling".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("Abbey Road".to_string()),
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000300),
            playcount: 1,
        },
        Track {
            name: "Octopus's Garden".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("Abbey Road".to_string()),
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000400),
            playcount: 1,
        },
        Track {
            name: "I Want You (She's So Heavy)".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("Abbey Road".to_string()),
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000500),
            playcount: 1,
        },
    ];

    let results = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Provider should not error");

    // Abbey Road is the original 1969 release, so we should NOT get suggestions
    // to move to bootlegs like "Get Back .... Continued!" or "Abbey Road Working Version"

    // Log any suggestions we get for debugging
    for (idx, suggestions) in &results {
        let track = &tracks[*idx];
        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(album) = &edit.album_name {
                    log::info!(
                        "Track '{}' suggestion: '{}' -> '{}'",
                        track.name,
                        track.album.as_ref().unwrap_or(&"<none>".to_string()),
                        album
                    );

                    // Ensure we're not suggesting bootlegs
                    assert!(
                        !album.to_lowercase().contains("working version"),
                        "Should not suggest 'Working Version' bootleg for '{}'",
                        track.name
                    );
                    assert!(
                        !album.to_lowercase().contains("get back .... continued"),
                        "Should not suggest 'Get Back .... Continued!' bootleg for '{}'",
                        track.name
                    );
                    assert!(
                        !album.to_lowercase().contains("bootleg"),
                        "Should not suggest any bootleg for '{}'",
                        track.name
                    );
                    assert!(
                        !album.to_lowercase().contains("demo"),
                        "Should not suggest demo releases for '{}'",
                        track.name
                    );
                    assert!(
                        !album.to_lowercase().contains("outtake"),
                        "Should not suggest outtake releases for '{}'",
                        track.name
                    );
                }
            }
        }
    }

    // Since Abbey Road is already the canonical 1969 release, we expect no suggestions
    // or only suggestions for earlier official releases if they exist
    if results.is_empty() {
        log::info!(
            "No suggestions for Abbey Road tracks (as expected - already on canonical release)"
        );
    } else {
        log::info!(
            "Got {} suggestions - verifying they are all official releases",
            results.len()
        );
    }
    }
);

// Test with a Beatles compilation to verify we still suggest the original album
mb_live_test!(
    async fn test_beatles_compilation_to_original() {

    let provider = CompilationToCanonicalProvider::new();

    // Test with Beatles tracks from a greatest hits compilation
    let tracks = vec![
        Track {
            name: "Come Together".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("1967-1970".to_string()), // The Blue Album compilation
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000000),
            playcount: 1,
        },
        Track {
            name: "Let It Be".to_string(),
            artist: "The Beatles".to_string(),
            album: Some("1967-1970".to_string()),
            album_artist: Some("The Beatles".to_string()),
            timestamp: Some(1600000100),
            playcount: 1,
        },
    ];

    let results = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Provider should not error");

    // These tracks are from a compilation, so we expect suggestions
    // But they should be to official albums, not bootlegs
    for (idx, suggestions) in &results {
        let track = &tracks[*idx];
        assert!(
            !suggestions.is_empty(),
            "Should have suggestions for compilation track '{}'",
            track.name
        );

        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(album) = &edit.album_name {
                    log::info!(
                        "Track '{}' suggestion: '{}' -> '{}'",
                        track.name,
                        track.album.as_ref().unwrap_or(&"<none>".to_string()),
                        album
                    );

                    // Ensure we're not suggesting bootlegs
                    assert!(
                        !album.to_lowercase().contains("bootleg"),
                        "Should not suggest bootleg for '{}'",
                        track.name
                    );
                    assert!(
                        !album.to_lowercase().contains("working version"),
                        "Should not suggest working version for '{}'",
                        track.name
                    );
                    assert!(
                        !album.to_lowercase().contains("demo"),
                        "Should not suggest demo for '{}'",
                        track.name
                    );

                    // We expect suggestions like "Abbey Road" for "Come Together"
                    // and "Let It Be" for "Let It Be"
                }
            }
        }
    }
    }
);
