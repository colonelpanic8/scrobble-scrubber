use scrobble_scrubber::rewrite::{RewriteRule, SdRule};
use scrobble_scrubber::scrub_action_provider::{
    RewriteRulesScrubActionProvider, ScrubActionProvider,
};
use std::collections::HashMap;

// Run by default. Opt out with SCROBBLE_SCRUBBER_SKIP_LIVE_MB_TESTS=1
fn live_mb_tests_disabled() -> bool {
    std::env::var("SCROBBLE_SCRUBBER_SKIP_LIVE_MB_TESTS")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

/// Test data for a track that should or should not be renamed
struct TrackTestCase {
    track_name: String,
    should_be_renamed: bool,
    expected_album: Option<String>,
}

/// Helper function to test MusicBrainz confirmation rules on albums
async fn test_mb_confirmation_rule(
    rule: RewriteRule,
    artist: &str,
    original_album: &str,
    test_cases: Vec<TrackTestCase>,
) {
    if live_mb_tests_disabled() {
        log::warn!(
            "Skipping live MusicBrainz test (unset SCROBBLE_SCRUBBER_SKIP_LIVE_MB_TESTS to run)"
        );
        return;
    }

    let provider = RewriteRulesScrubActionProvider::from_rules(vec![rule]);

    // Build tracks from test cases
    let tracks: Vec<lastfm_edit::Track> = test_cases
        .iter()
        .enumerate()
        .map(|(idx, tc)| lastfm_edit::Track {
            name: tc.track_name.clone(),
            artist: artist.to_string(),
            album: Some(original_album.to_string()),
            album_artist: None,
            playcount: 1,
            timestamp: Some(1_600_000_000 + idx as u64 * 100),
        })
        .collect();

    let results = provider
        .analyze_tracks(&tracks, None, None)
        .await
        .expect("analysis should succeed");

    // Convert results to a map for easier assertions
    let mut map = HashMap::new();
    for (idx, suggestions) in results {
        map.insert(idx, suggestions);
    }

    // Check each test case
    for (idx, tc) in test_cases.iter().enumerate() {
        if tc.should_be_renamed {
            let suggestions = map.get(&idx).expect(&format!(
                "Track '{}' should produce a suggestion",
                tc.track_name
            ));
            assert!(
                !suggestions.is_empty(),
                "Expected at least one suggestion for '{}'",
                tc.track_name
            );

            // Find an Edit suggestion and verify album changed
            let mut found = false;
            for s in suggestions {
                if let scrobble_scrubber::scrub_action_provider::ScrubActionSuggestion::Edit(edit) =
                    &s.suggestion
                {
                    if let Some(expected) = &tc.expected_album {
                        if edit.album_name.as_deref() == Some(expected.as_str()) {
                            found = true;
                            break;
                        }
                    }
                }
            }
            if let Some(expected) = &tc.expected_album {
                assert!(
                    found,
                    "Expected album to be rewritten to '{}' for '{}'",
                    expected, tc.track_name
                );
            }
        } else {
            assert!(
                !map.contains_key(&idx),
                "Track '{}' should not be rewritten from '{}' to '{:?}'",
                tc.track_name,
                original_album,
                tc.expected_album
            );
        }
    }
}

#[test_log::test(tokio::test)]
async fn elliott_smith_xo() {
    // Rule: remove "(Deluxe Edition)" from album names, but only when MB confirms the (artist, title, album) exists
    let rule = RewriteRule::new()
        .with_album_name(SdRule::new(r"^(.*) \(Deluxe Edition\)$", "$1").with_flags("i"))
        .with_musicbrainz_confirmation_required(true);

    test_mb_confirmation_rule(
        rule,
        "Elliott Smith",
        "XO (Deluxe Edition)",
        vec![
            TrackTestCase {
                track_name: "Miss Misery".to_string(),
                should_be_renamed: false,
                expected_album: Some("XO".to_string()),
            },
            TrackTestCase {
                track_name: "Independence Day".to_string(),
                should_be_renamed: true,
                expected_album: Some("XO".to_string()),
            },
        ],
    )
    .await;
}

#[test_log::test(tokio::test)]
async fn sublime() {
    // Rule: remove any parenthetical that contains "Deluxe Edition" (case-insensitive), MB-confirmed
    let rule = RewriteRule::new()
        .with_album_name(
            SdRule::new(r"^(.+?) \((?:.*[Dd]eluxe [Ee]dition.*)\)$", "$1").with_flags("i"),
        )
        .with_musicbrainz_confirmation_required(true);

    test_mb_confirmation_rule(
        rule,
        "Sublime",
        "Sublime (10th Anniversary Edition / Deluxe Edition)",
        vec![
            // Based on the earlier output, the 1990 demo has these tracks:
            // "Don't Push", "Ball & Chain", "Slow Ride", "Date Rape Stylee"
            TrackTestCase {
                track_name: "Ball & Chain".to_string(),
                should_be_renamed: true,
                expected_album: Some("Sublime".to_string()),
            },
            // "Garden Grove" is NOT on the 1990 demo, only on later releases
            TrackTestCase {
                track_name: "Garden Grove".to_string(),
                should_be_renamed: false,
                expected_album: Some("Sublime".to_string()),
            },
        ],
    )
    .await;
}

#[test_log::test(tokio::test)]
async fn jeff_buckley_grace() {
    // Rule: remove "(Legacy Edition)" from album names, but only when MB confirms
    let rule = RewriteRule::new()
        .with_album_name(SdRule::new(r"^(.*) \(Legacy Edition\)$", "$1").with_flags("i"))
        .with_musicbrainz_confirmation_required(true);

    test_mb_confirmation_rule(
        rule,
        "Jeff Buckley",
        "Grace (Legacy Edition)",
        vec![
            TrackTestCase {
                track_name: "Grace".to_string(),
                should_be_renamed: true,
                expected_album: Some("Grace".to_string()),
            },
            TrackTestCase {
                track_name: "I Want Someone Badly".to_string(),
                should_be_renamed: false,
                expected_album: Some("Grace".to_string()),
            },
        ],
    )
    .await;
}

#[test_log::test(tokio::test)]
async fn nirvana_nevermind() {
    // Rule: remove "(20th Anniversary Edition)" and similar from album names
    let rule = RewriteRule::new()
        .with_album_name(
            SdRule::new(r"^(.*) \(\d+\w+ Anniversary Edition\)$", "$1").with_flags("i"),
        )
        .with_musicbrainz_confirmation_required(true);

    test_mb_confirmation_rule(
        rule,
        "Nirvana",
        "Nevermind (20th Anniversary Edition)",
        vec![
            TrackTestCase {
                track_name: "Smells Like Teen Spirit".to_string(),
                should_be_renamed: true,
                expected_album: Some("Nevermind".to_string()),
            },
            TrackTestCase {
                track_name: "Sappy (Early Demo)".to_string(),
                should_be_renamed: false,
                expected_album: Some("Nevermind".to_string()),
            },
        ],
    )
    .await;
}
