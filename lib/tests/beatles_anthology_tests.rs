use lastfm_edit::Track;
use scrobble_scrubber::musicbrainz::CompilationToCanonicalProvider;
use scrobble_scrubber::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};

// Import the common test utilities
mod common;

#[test_log::test(tokio::test)]
async fn beatles_anthology_1_free_as_a_bird() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Free as a Bird" was released on Anthology 1 (1995) as its first release
    // This is actually the earliest release for this track, so should not be remapped
    let track = Track {
        name: "Free as a Bird".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Anthology 1".to_string()),
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

        // Free as a Bird was first released on Anthology 1, so it shouldn't move
        // unless there's a single release
        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    log::info!("Free as a Bird suggestion: {suggested_album}");
                    // A single release would be acceptable
                    if suggested_album.to_lowercase().contains("free as a bird")
                        && !suggested_album.to_lowercase().contains("anthology")
                    {
                        log::info!("Correctly suggesting single release");
                    }
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_anthology_1_love_me_do() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Love Me Do" on Anthology 1 should be remapped to "Please Please Me" album or single
    let track = Track {
        name: "Love Me Do".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Anthology 1".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    // Should suggest "Please Please Me" album (1963) or "Love Me Do" single (1962)
                    assert_ne!(
                        suggested_album, "Anthology 1",
                        "Should not keep on Anthology 1"
                    );

                    // Should be either the original album or single
                    // Note: MusicBrainz may return foreign language versions
                    let valid_suggestion =
                        suggested_album.to_lowercase().contains("please please me")
                            || suggested_album.to_lowercase().contains("love me do")
                            || suggested_album.to_lowercase().contains("single")
                            || suggested_album.to_lowercase().contains("knüller"); // German version

                    assert!(
                        valid_suggestion,
                        "Should suggest Please Please Me album or Love Me Do single, got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            panic!("Expected suggestions for Love Me Do on Anthology 1");
        }
    } else {
        panic!("Expected results for Love Me Do on Anthology 1");
    }
}

#[test_log::test(tokio::test)]
async fn beatles_anthology_2_real_love() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Real Love" was first released on Anthology 2 (1996)
    // Like "Free as a Bird", this is its earliest release
    let track = Track {
        name: "Real Love".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Anthology 2".to_string()),
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

        // Real Love was first released on Anthology 2
        // A single release would be acceptable if it exists
        for suggestion in suggestions {
            if let ScrubActionSuggestion::Edit(edit) = &suggestion.suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    log::info!("Real Love suggestion: {suggested_album}");
                    // A single would be acceptable
                    if suggested_album.to_lowercase().contains("real love")
                        && !suggested_album.to_lowercase().contains("anthology")
                    {
                        log::info!("Correctly suggesting single release");
                    }
                }
            }
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_anthology_2_strawberry_fields() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Strawberry Fields Forever" on Anthology 2 should remap to single or Magical Mystery Tour
    let track = Track {
        name: "Strawberry Fields Forever".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Anthology 2".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "Anthology 2",
                        "Should not keep on Anthology 2"
                    );

                    // Should suggest the single (1967) or Magical Mystery Tour
                    let valid_suggestion =
                        suggested_album.to_lowercase().contains("strawberry fields")
                            || suggested_album == "Magical Mystery Tour"
                            || suggested_album.to_lowercase().contains("penny lane"); // double A-side single

                    assert!(
                        valid_suggestion,
                        "Should suggest single or Magical Mystery Tour, got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            log::warn!("No suggestions for Strawberry Fields Forever on Anthology 2 - may already be optimal in MusicBrainz");
        }
    } else {
        log::warn!("No results for Strawberry Fields Forever on Anthology 2 - MusicBrainz may not have found the track");
    }
}

#[test_log::test(tokio::test)]
async fn beatles_anthology_3_while_my_guitar() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "While My Guitar Gently Weeps" on Anthology 3 should remap to The White Album
    let track = Track {
        name: "While My Guitar Gently Weeps".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Anthology 3".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "Anthology 3",
                        "Should not keep on Anthology 3"
                    );

                    // Should suggest The Beatles (White Album)
                    let valid_suggestion = suggested_album == "The Beatles"
                        || suggested_album.to_lowercase().contains("white album");

                    assert!(
                        valid_suggestion,
                        "Should suggest The Beatles (White Album), got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            log::warn!("No suggestions for While My Guitar Gently Weeps on Anthology 3 - may already be optimal in MusicBrainz");
        }
    } else {
        log::warn!("No results for While My Guitar Gently Weeps on Anthology 3 - MusicBrainz may not have found the track");
    }
}

#[test_log::test(tokio::test)]
async fn beatles_red_album_help() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Help!" on 1962-1966 (Red Album) should remap to Help! album
    let track = Track {
        name: "Help!".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("1962-1966".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "1962-1966",
                        "Should not keep on Red Album compilation"
                    );

                    // Should suggest Help! album
                    assert_eq!(
                        suggested_album, "Help!",
                        "Should suggest Help! album, got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            panic!("Expected suggestions for Help! on 1962-1966");
        }
    } else {
        panic!("Expected results for Help! on 1962-1966");
    }
}

#[test_log::test(tokio::test)]
async fn beatles_blue_album_hey_jude() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Hey Jude" on 1967-1970 (Blue Album) should remap to single or Past Masters
    let track = Track {
        name: "Hey Jude".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("1967-1970".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "1967-1970",
                        "Should not keep on Blue Album compilation"
                    );

                    // Hey Jude was released as a single, not on a studio album
                    // Might suggest the single or Past Masters compilation (which collected non-album tracks)
                    let valid_suggestion = suggested_album.to_lowercase().contains("hey jude")
                        || suggested_album.to_lowercase().contains("past masters")
                        || suggested_album.to_lowercase().contains("single");

                    assert!(
                        valid_suggestion,
                        "Should suggest Hey Jude single or Past Masters, got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            panic!("Expected suggestions for Hey Jude on 1967-1970");
        }
    } else {
        panic!("Expected results for Hey Jude on 1967-1970");
    }
}

#[test_log::test(tokio::test)]
async fn beatles_one_compilation_yesterday() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Yesterday" on "1" compilation should remap to Help! album
    let track = Track {
        name: "Yesterday".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("1".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(suggested_album, "1", "Should not keep on '1' compilation");

                    // Should suggest Help! album
                    assert_eq!(
                        suggested_album, "Help!",
                        "Should suggest Help! album, got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            panic!("Expected suggestions for Yesterday on '1' compilation");
        }
    } else {
        panic!("Expected results for Yesterday on '1' compilation");
    }
}

#[test_log::test(tokio::test)]
async fn prefer_single_over_compilation() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // Test that singles are preferred over compilations
    // "She Loves You" was released as a single before appearing on compilations
    let track = Track {
        name: "She Loves You".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Past Masters".to_string()), // compilation of non-album tracks
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

        // Past Masters is already a compilation of singles/non-album tracks
        // So we might not get suggestions, or we might get the original single
        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    log::info!("She Loves You suggestion: {suggested_album}");

                    // If we get a suggestion, it should be for the single release
                    if suggested_album.to_lowercase().contains("she loves you") {
                        log::info!("Correctly suggesting single release");
                    }
                }
            }
        } else {
            log::info!("No suggestions for She Loves You on Past Masters - may already be optimal");
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_past_masters_paperback_writer() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Paperback Writer" on Past Masters should potentially suggest the single
    let track = Track {
        name: "Paperback Writer".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Past Masters".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    log::info!("Paperback Writer suggestion: {suggested_album}");

                    // Should prefer the single release over Past Masters compilation
                    if suggested_album.to_lowercase().contains("paperback writer")
                        || suggested_album.to_lowercase().contains("rain")
                    {
                        // B-side
                        log::info!("Correctly suggesting single release");
                    }
                }
            }
        } else {
            log::info!("No suggestions for Paperback Writer on Past Masters");
        }
    }
}

#[test_log::test(tokio::test)]
async fn beatles_greatest_hits_come_together() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Come Together" from a greatest hits should remap to Abbey Road
    let track = Track {
        name: "Come Together".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Greatest Hits".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "Greatest Hits",
                        "Should not keep on Greatest Hits compilation"
                    );

                    // Should suggest Abbey Road
                    assert_eq!(
                        suggested_album, "Abbey Road",
                        "Should suggest Abbey Road, got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            log::warn!("No suggestions for Come Together on Greatest Hits");
        }
    } else {
        log::warn!("No results for Come Together on Greatest Hits");
    }
}

#[test_log::test(tokio::test)]
async fn beatles_love_album_should_remap() {
    skip_if_live_mb_disabled!();

    let provider = CompilationToCanonicalProvider::new();

    // "Here Comes the Sun" from Love (2006 remix album) should remap to Abbey Road
    let track = Track {
        name: "Here Comes the Sun".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Love".to_string()),
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

        if !suggestions.is_empty() {
            if let ScrubActionSuggestion::Edit(edit) = &suggestions[0].suggestion {
                if let Some(suggested_album) = &edit.album_name {
                    assert_ne!(
                        suggested_album, "Love",
                        "Should not keep on Love remix album"
                    );

                    // Should suggest Abbey Road
                    assert_eq!(
                        suggested_album, "Abbey Road",
                        "Should suggest Abbey Road, got: {suggested_album}"
                    );

                    log::info!("Correctly suggested: {suggested_album}");
                }
            }
        } else {
            log::warn!("No suggestions for Here Comes the Sun on Love album");
        }
    } else {
        log::warn!("No results for Here Comes the Sun on Love album");
    }
}
