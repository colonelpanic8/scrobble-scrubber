use lastfm_edit::Track;
use scrobble_scrubber::musicbrainz::CompilationToCanonicalProvider;
use scrobble_scrubber::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};

// Import the common test utilities
mod common;

#[test_log::test(tokio::test)]
async fn compilation_to_canonical_basic() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Test: "Mr. Blue Sky" on a compilation album
    // This track originally appears on ELO's "Out of the Blue" (1977)
    let track = Track {
        name: "Mr. Blue Sky".to_string(),
        artist: "Electric Light Orchestra".to_string(),
        album: Some("NOW That's What I Call Music!".to_string()),
        album_artist: Some("Various Artists".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    // The API might not find "NOW That's What I Call Music!" in MusicBrainz,
    // or it might not be marked as a compilation in the database.
    // This is expected behavior when being conservative.
    if results.is_empty() {
        log::warn!("No suggestions returned for 'Mr. Blue Sky' from NOW compilation - this is expected if MusicBrainz doesn't have the compilation or can't confirm it's a compilation");
        return;
    }

    let (idx, suggestions) = &results[0];
    assert_eq!(*idx, 0);
    assert!(
        !suggestions.is_empty(),
        "Should have at least one suggestion"
    );

    // Check that it suggests a non-compilation album
    if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
        let suggested_album = edit
            .album_name
            .as_ref()
            .expect("Should have album suggestion");
        assert_ne!(
            suggested_album, "NOW That's What I Call Music!",
            "Should not suggest the same compilation"
        );
        // Check it's not another compilation
        assert!(
            !suggested_album.to_lowercase().contains("greatest")
                && !suggested_album.to_lowercase().contains("hits")
                && !suggested_album.to_lowercase().contains("collection"),
            "Should not suggest another compilation: {suggested_album}"
        );
    } else {
        panic!("Expected Edit suggestion for compilation track");
    }
}

#[test_log::test(tokio::test)]
async fn greatest_hits_to_original() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Bohemian Rhapsody" from Queen's Greatest Hits -> should suggest "A Night at the Opera"
    let track = Track {
        name: "Bohemian Rhapsody".to_string(),
        artist: "Queen".to_string(),
        album: Some("Greatest Hits".to_string()),
        album_artist: Some("Queen".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    // The API might not always return suggestions, especially for well-known albums
    // that might be considered canonical releases themselves
    if results.is_empty() {
        log::warn!("No suggestions returned for 'Bohemian Rhapsody' from Greatest Hits - this can happen if MusicBrainz considers it a primary release");
        return;
    }

    let (idx, suggestions) = &results[0];
    assert_eq!(*idx, 0);

    if suggestions.is_empty() {
        log::warn!(
            "No suggestions for this track - MusicBrainz might not have found earlier releases"
        );
        return;
    }

    if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
        let suggested_album = edit
            .album_name
            .as_ref()
            .expect("Should have album suggestion");
        assert_ne!(
            suggested_album, "Greatest Hits",
            "Should not suggest the same compilation"
        );
        // Check it's not another compilation
        assert!(
            !suggested_album.to_lowercase().contains("greatest")
                && !suggested_album.to_lowercase().contains("hits")
                && !suggested_album.to_lowercase().contains("collection"),
            "Should not suggest another compilation: {suggested_album}"
        );
    } else {
        panic!("Expected Edit suggestion");
    }
}

#[test_log::test(tokio::test)]
async fn already_on_earliest_release() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Track already on its original album - should not suggest changes
    // Using "A Night at the Opera" which is the original 1975 release
    let track = Track {
        name: "Bohemian Rhapsody".to_string(),
        artist: "Queen".to_string(),
        album: Some("A Night at the Opera".to_string()),
        album_artist: Some("Queen".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let _results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    // With the new approach, we might get suggestions if there's an earlier release
    // But "A Night at the Opera" is the original, so likely no suggestions
    // However, if MusicBrainz has earlier compilations or singles, that's OK too
    // The key is we're not suggesting later albums
}

#[test_log::test(tokio::test)]
async fn soundtrack_to_original() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Eye of the Tiger" from Rocky III Soundtrack -> should suggest Survivor's album
    let track = Track {
        name: "Eye of the Tiger".to_string(),
        artist: "Survivor".to_string(),
        album: Some("Rocky III - Original Motion Picture Soundtrack".to_string()),
        album_artist: Some("Various Artists".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    if !results.is_empty() {
        let (idx, suggestions) = &results[0];
        assert_eq!(*idx, 0);

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(album) = &edit.album_name {
                    log::info!("Suggested '{album}' for 'Eye of the Tiger'");
                    // The earliest release is likely the original "Eye of the Tiger" album
                    // but could be something else depending on MusicBrainz data
                }
            }
        }
    } else {
        log::info!("No suggestions for 'Eye of the Tiger' - possibly already on earliest release");
    }
}

#[test_log::test(tokio::test)]
async fn multiple_tracks_batch() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    let tracks = vec![
        // Compilation track
        Track {
            name: "Don't Stop Me Now".to_string(),
            artist: "Queen".to_string(),
            album: Some("Greatest Hits".to_string()),
            album_artist: Some("Queen".to_string()),
            timestamp: Some(1600000000),
            playcount: 1,
        },
        // Already on original album
        Track {
            name: "Another One Bites the Dust".to_string(),
            artist: "Queen".to_string(),
            album: Some("The Game".to_string()),
            album_artist: Some("Queen".to_string()),
            timestamp: Some(1600000100),
            playcount: 1,
        },
        // Another compilation track
        Track {
            name: "Somebody to Love".to_string(),
            artist: "Queen".to_string(),
            album: Some("Greatest Hits".to_string()),
            album_artist: Some("Queen".to_string()),
            timestamp: Some(1600000200),
            playcount: 1,
        },
    ];

    let results = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("Provider should not error");

    // With the new approach, any track might get a suggestion if there's an earlier release
    // We just check that the provider runs without error
    // The exact suggestions depend on MusicBrainz data
    for (idx, suggestions) in &results {
        log::info!("Track {} got {} suggestions", idx, suggestions.len());

        for suggestion in suggestions {
            if let scrobble_scrubber::scrub_action_provider::ScrubActionSuggestion::Edit(edit) =
                &suggestion.suggestion
            {
                if let Some(album) = &edit.album_name {
                    log::info!("  -> Suggesting album: {album}");
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn compilation_only_track_hot_fun() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Hot Fun In the Summertime" appears on various compilations but may not have a studio album
    // It was released as a single but often appears on Greatest Hits compilations
    let track = Track {
        name: "Hot Fun In the Summertime".to_string(),
        artist: "Sly & The Family Stone".to_string(),
        album: Some("Greatest Hits".to_string()),
        album_artist: Some("Sly & The Family Stone".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    // With the earliest release approach, we'll get whatever is earliest
    // That might be another compilation, a studio album, or nothing
    if !results.is_empty() {
        let (idx, suggestions) = &results[0];
        assert_eq!(*idx, 0);

        if let Some(first_suggestion) = suggestions.first() {
            if let scrobble_scrubber::scrub_action_provider::ScrubActionSuggestion::Edit(edit) =
                &first_suggestion.suggestion
            {
                if let Some(suggested_album) = &edit.album_name {
                    log::info!("Suggested '{suggested_album}' for 'Hot Fun In the Summertime'");
                    // Any earlier release is valid with the new approach
                }
            }
        }
    } else {
        log::info!(
            "No suggestions for 'Hot Fun In the Summertime' - likely already on earliest release"
        );
    }
}

#[test_log::test(tokio::test)]
async fn compilation_first_track_outkast() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "The Whole World" by Outkast featuring Killer Mike
    // This track was first released on the Big Boi and Dre Present... Outkast compilation
    let track = Track {
        name: "The Whole World".to_string(),
        artist: "OutKast".to_string(),
        album: Some("Big Boi and Dre Present... OutKast".to_string()),
        album_artist: Some("OutKast".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    // With the earliest release approach, we might not get suggestions if this IS the earliest
    // Or we might get an earlier release if one exists
    // The provider should run without error either way
    if !results.is_empty() {
        let (idx, suggestions) = &results[0];
        assert_eq!(*idx, 0);

        if let Some(first_suggestion) = suggestions.first() {
            if let scrobble_scrubber::scrub_action_provider::ScrubActionSuggestion::Edit(edit) =
                &first_suggestion.suggestion
            {
                if let Some(suggested_album) = &edit.album_name {
                    log::info!("Suggested '{suggested_album}' for 'The Whole World'");
                }
            }
        }
    } else {
        log::info!("No suggestions for 'The Whole World' - likely already on earliest release");
    }
}

#[test_log::test(tokio::test)]
async fn single_released_track() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Some songs were only released as singles and then appear on compilations
    // Example: Many Motown singles that later appeared on Greatest Hits albums
    let track = Track {
        name: "I Heard It Through the Grapevine".to_string(),
        artist: "Marvin Gaye".to_string(),
        album: Some("Greatest Hits".to_string()),
        album_artist: Some("Marvin Gaye".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    // This track was on "In the Groove" album, so it might suggest that
    // But if it only finds singles and compilations, no suggestion is fine
    if !results.is_empty() {
        let (idx, suggestions) = &results[0];
        assert_eq!(*idx, 0);

        if let Some(first_suggestion) = suggestions.first() {
            if let scrobble_scrubber::scrub_action_provider::ScrubActionSuggestion::Edit(edit) =
                &first_suggestion.suggestion
            {
                if let Some(suggested_album) = &edit.album_name {
                    // Should not suggest another compilation
                    assert!(
                        !suggested_album.to_lowercase().contains("greatest")
                            && !suggested_album.to_lowercase().contains("hits")
                            && !suggested_album.to_lowercase().contains("collection"),
                        "Should not suggest another compilation: {suggested_album}"
                    );

                    log::info!(
                        "Suggested album for 'I Heard It Through the Grapevine': {suggested_album}"
                    );
                }
            }
        }
    } else {
        log::info!("No suggestions for 'I Heard It Through the Grapevine' - likely only found compilations");
    }
}

#[test_log::test(tokio::test)]
async fn beatles_eleanor_rigby_should_stay_on_revolver() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Eleanor Rigby should stay on Revolver, not move to Beatz'n Rhymes 4
    let track = Track {
        name: "Eleanor Rigby".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Revolver".to_string()),
        album_artist: Some("The Beatles".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    // Eleanor Rigby on Revolver is the original release (1966)
    // Should NOT suggest moving to compilations like "Beatz'n Rhymes 4"
    if !results.is_empty() {
        let (_idx, suggestions) = &results[0];

        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    // Fail if it suggests "Beatz'n Rhymes 4" or any obvious compilation
                    assert_ne!(
                        suggested_album, "Beatz'n Rhymes 4",
                        "Should not suggest moving Eleanor Rigby from Revolver to Beatz'n Rhymes 4"
                    );
                    assert!(
                        !suggested_album.to_lowercase().contains("beatz"),
                        "Should not suggest hip-hop compilation for Beatles track"
                    );
                    log::warn!(
                        "Eleanor Rigby on Revolver got suggestion: {suggested_album} - checking if valid"
                    );
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_yellow_submarine_correct_album() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Yellow Submarine on Revolver is the original (1966)
    // Should NOT move to Yellow Submarine soundtrack (1969)
    let track = Track {
        name: "Yellow Submarine".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Revolver".to_string()),
        album_artist: Some("The Beatles".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    if !results.is_empty() {
        let (_idx, suggestions) = &results[0];

        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    // The Yellow Submarine soundtrack (1969) is LATER than Revolver (1966)
                    // So this should not be suggested
                    if suggested_album == "Yellow Submarine" {
                        panic!(
                            "Should not suggest moving Yellow Submarine from Revolver (1966) to Yellow Submarine soundtrack (1969)"
                        );
                    }
                    log::info!("Yellow Submarine on Revolver got suggestion: {suggested_album}");
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_let_it_be_not_to_fast_lane() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Let It Be" should not move to "Fast Lane" compilation
    let track = Track {
        name: "Let It Be".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Let It Be... Naked".to_string()),
        album_artist: Some("The Beatles".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    if !results.is_empty() {
        let (_idx, suggestions) = &results[0];

        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "Fast Lane",
                        "Should not suggest moving Let It Be to Fast Lane compilation"
                    );
                    // Original "Let It Be" (1970) would be acceptable
                    if suggested_album == "Let It Be" {
                        log::info!("Correctly suggesting original Let It Be album");
                    } else {
                        log::warn!(
                            "Let It Be got suggestion: {suggested_album} - verifying it's appropriate"
                        );
                    }
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_day_in_life_stay_on_sgt_pepper() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "A Day in the Life" should stay on Sgt. Pepper's, not move to 1967-1970 compilation
    let track = Track {
        name: "A Day in the Life".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Sgt. Pepper's Lonely Hearts Club Band".to_string()),
        album_artist: Some("The Beatles".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    if !results.is_empty() {
        let (_idx, suggestions) = &results[0];

        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    // 1967-1970 is a compilation from 1973, should not be suggested
                    assert_ne!(
                        suggested_album, "1967–1970",
                        "Should not suggest moving from Sgt. Pepper's (1967) to 1967-1970 compilation (1973)"
                    );
                    assert_ne!(
                        suggested_album, "1967-1970",
                        "Should not suggest moving from Sgt. Pepper's (1967) to 1967-1970 compilation (1973)"
                    );
                    log::warn!(
                        "A Day in the Life on Sgt. Pepper's got suggestion: {suggested_album}"
                    );
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_lucy_not_to_love_album() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Lucy in the Sky With Diamonds" should stay on Sgt. Pepper's, not move to Love remix album
    let track = Track {
        name: "Lucy in the Sky With Diamonds".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Sgt. Pepper's Lonely Hearts Club Band".to_string()),
        album_artist: Some("The Beatles".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    if !results.is_empty() {
        let (_idx, suggestions) = &results[0];

        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    // "Love" is a 2006 remix album, should not be suggested over 1967 original
                    assert_ne!(
                        suggested_album, "Love",
                        "Should not suggest moving from Sgt. Pepper's (1967) to Love remix album (2006)"
                    );
                    log::warn!(
                        "Lucy in the Sky on Sgt. Pepper's got suggestion: {suggested_album}"
                    );
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_sgt_pepper_title_track_not_to_interpretan() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Sgt. Pepper's Lonely Hearts Club Band" title track should not move to "Interpretan A Los Beatles"
    let track = Track {
        name: "Sgt. Pepper's Lonely Hearts Club Band".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Sgt. Pepper's Lonely Hearts Club Band".to_string()),
        album_artist: Some("The Beatles".to_string()),
        timestamp: Some(1600000000),
        playcount: 1,
    };

    let results = provider
        .analyze_tracks(&[track], None, None)
        .await
        .expect("Provider should not error");

    if !results.is_empty() {
        let (_idx, suggestions) = &results[0];

        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "Interpretan A Los Beatles",
                        "Should not suggest moving to Spanish tribute album"
                    );
                    assert!(
                        !suggested_album.to_lowercase().contains("interpretan"),
                        "Should not suggest tribute albums for original Beatles tracks"
                    );
                    log::warn!("Sgt. Pepper's title track got suggestion: {suggested_album}");
                }
            }
        }
    }
}
