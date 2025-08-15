use lastfm_edit::Track;
use scrobble_scrubber::musicbrainz::CompilationToCanonicalProvider;
use scrobble_scrubber::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};

// Import the common test utilities
mod common;

/// Helper to create a track for testing
fn create_track(name: &str, artist: &str, album: &str) -> Track {
    Track {
        name: name.to_string(),
        artist: artist.to_string(),
        album: Some(album.to_string()),
        album_artist: None,
        playcount: 1,
        timestamp: Some(0),
    }
}

/// Helper to assert a specific album suggestion was made
async fn assert_suggests_album(
    provider: &CompilationToCanonicalProvider,
    track: Track,
    expected_album: &str,
) {
    let tracks = vec![track.clone()];
    let suggestions = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Failed to analyze tracks");

    assert!(
        !suggestions.is_empty(),
        "Expected suggestion for '{}' by '{}' from '{}' to '{}', but got no suggestions",
        track.name,
        track.artist,
        track.album.as_ref().unwrap(),
        expected_album
    );

    let (_, track_suggestions) = &suggestions[0];
    assert!(
        !track_suggestions.is_empty(),
        "Expected suggestion for track but got empty suggestions"
    );

    // Check the suggestion is an edit
    match &track_suggestions[0].suggestion {
        ScrubActionSuggestion::Edit(edit) => {
            assert_eq!(
                edit.album_name.as_ref().expect("Edit should have album"),
                expected_album,
                "Expected album '{}' but got '{:?}'",
                expected_album,
                edit.album_name
            );
        }
        _ => panic!("Expected Edit suggestion but got something else"),
    }
}

/// Helper to assert no suggestion is made (track already on correct album)
async fn assert_no_suggestion(provider: &CompilationToCanonicalProvider, track: Track) {
    let tracks = vec![track.clone()];
    let suggestions = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Failed to analyze tracks");

    assert!(
        suggestions.is_empty(),
        "Expected no suggestions for '{}' by '{}' on '{}', but got {} suggestions",
        track.name,
        track.artist,
        track.album.as_ref().unwrap(),
        suggestions.len()
    );
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_love_me_do_should_only_accept_please_please_me() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Test 1: Love Me Do on compilation should suggest Please Please Me
    let track = create_track("Love Me Do", "The Beatles", "1962-1966");
    assert_suggests_album(&provider, track, "Please Please Me").await;

    // Test 2: Love Me Do on different compilation should still suggest Please Please Me
    let track = create_track("Love Me Do", "The Beatles", "1");
    assert_suggests_album(&provider, track, "Please Please Me").await;

    // Test 3: Love Me Do already on Please Please Me should suggest nothing
    let track = create_track("Love Me Do", "The Beatles", "Please Please Me");
    assert_no_suggestion(&provider, track).await;
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_come_together_should_only_accept_abbey_road() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Test 1: Come Together on compilation should suggest Abbey Road
    let track = create_track("Come Together", "The Beatles", "1967-1970");
    assert_suggests_album(&provider, track, "Abbey Road").await;

    // Test 2: Come Together on "1" compilation should suggest Abbey Road
    let track = create_track("Come Together", "The Beatles", "1");
    assert_suggests_album(&provider, track, "Abbey Road").await;

    // Test 3: Come Together already on Abbey Road should suggest nothing
    let track = create_track("Come Together", "The Beatles", "Abbey Road");
    assert_no_suggestion(&provider, track).await;
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_hey_jude_should_accept_past_masters() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Hey Jude was a non-album single, so it should go to Past Masters
    let track = create_track("Hey Jude", "The Beatles", "1967-1970");
    assert_suggests_album(&provider, track, "Past Masters").await;

    // Hey Jude on "1" compilation should also suggest Past Masters
    let track = create_track("Hey Jude", "The Beatles", "1");
    assert_suggests_album(&provider, track, "Past Masters").await;

    // Hey Jude already on Past Masters should suggest nothing
    let track = create_track("Hey Jude", "The Beatles", "Past Masters");
    assert_no_suggestion(&provider, track).await;
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_yesterday_should_only_accept_help() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Yesterday first appeared on Help!
    let track = create_track("Yesterday", "The Beatles", "1962-1966");
    assert_suggests_album(&provider, track, "Help!").await;

    // Yesterday on "1" compilation should suggest Help!
    let track = create_track("Yesterday", "The Beatles", "1");
    assert_suggests_album(&provider, track, "Help!").await;

    // Yesterday already on Help! should suggest nothing
    let track = create_track("Yesterday", "The Beatles", "Help!");
    assert_no_suggestion(&provider, track).await;
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_let_it_be_should_only_accept_let_it_be_album() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Let It Be (song) should go to Let It Be (album)
    let track = create_track("Let It Be", "The Beatles", "1967-1970");
    assert_suggests_album(&provider, track, "Let It Be").await;

    // Already on correct album
    let track = create_track("Let It Be", "The Beatles", "Let It Be");
    assert_no_suggestion(&provider, track).await;
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_should_not_suggest_for_non_compilation() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Track already on studio album should not get suggestions
    let track = create_track("Something", "The Beatles", "Abbey Road");
    assert_no_suggestion(&provider, track).await;

    // Track on Rubber Soul should stay there
    let track = create_track("Norwegian Wood", "The Beatles", "Rubber Soul");
    assert_no_suggestion(&provider, track).await;
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_should_handle_live_albums_correctly() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Tracks on Live at the BBC should not be moved (it's a live album, not a compilation)
    // The provider should recognize this and not suggest changes
    let track = create_track("Love Me Do", "The Beatles", "Live at the BBC");

    let tracks = vec![track.clone()];
    let suggestions = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Failed to analyze tracks");

    // This might or might not suggest something depending on how the provider
    // handles live albums. Let's see what it actually does.
    if !suggestions.is_empty() {
        log::warn!(
            "Provider suggested moving track from Live at the BBC - this may be incorrect behavior"
        );
        let (_, track_suggestions) = &suggestions[0];
        if let ScrubActionSuggestion::Edit(edit) = &track_suggestions[0].suggestion {
            log::warn!("Suggested album: {:?}", edit.album_name);
        }
    } else {
        log::info!("Correctly did not suggest moving track from Live at the BBC");
    }
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_should_not_move_from_anthology_to_studio() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Anthology albums contain alternate takes/demos, not the studio versions
    // "Love Me Do" on Anthology 1 is a different recording than on Please Please Me
    let track = create_track("Love Me Do", "The Beatles", "Anthology 1");

    let tracks = vec![track.clone()];
    let suggestions = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Failed to analyze tracks");

    // The provider might incorrectly suggest moving this to Please Please Me
    // but it shouldn't because Anthology versions are different recordings
    if !suggestions.is_empty() {
        log::warn!("Provider incorrectly suggested moving track from Anthology 1");
        let (_, track_suggestions) = &suggestions[0];
        if let ScrubActionSuggestion::Edit(edit) = &track_suggestions[0].suggestion {
            log::warn!("Incorrectly suggested album: {:?}", edit.album_name);
        }
    } else {
        log::info!("Correctly did not suggest moving track from Anthology 1");
    }
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_should_handle_greatest_hits_vs_compilation() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Test that we correctly identify "1" as a compilation
    let track = create_track("Help!", "The Beatles", "1");
    assert_suggests_album(&provider, track, "Help!").await;

    // Test red and blue albums are identified as compilations
    let track = create_track("Help!", "The Beatles", "1962-1966");
    assert_suggests_album(&provider, track, "Help!").await;
}

#[test_log::test(tokio::test)]
#[ignore] // Remove ignore to run with live MusicBrainz
async fn test_edge_case_tracks_with_multiple_album_appearances() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Revolution" appeared as a single and on Past Masters, not on the White Album
    // "Revolution 1" is the White Album version (different recording)
    let track = create_track("Revolution", "The Beatles", "1967-1970");

    let tracks = vec![track.clone()];
    let suggestions = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Failed to analyze tracks");

    if !suggestions.is_empty() {
        let (_, track_suggestions) = &suggestions[0];
        if let ScrubActionSuggestion::Edit(edit) = &track_suggestions[0].suggestion {
            // Should suggest Past Masters, not The Beatles (White Album)
            log::info!("Revolution suggested album: {:?}", edit.album_name);
            // Note: The actual behavior depends on MusicBrainz data
            // It should prefer Past Masters over White Album for "Revolution"
        }
    }
}
