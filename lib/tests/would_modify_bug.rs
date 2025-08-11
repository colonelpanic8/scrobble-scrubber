use lastfm_edit::Track;
use scrobble_scrubber::rewrite::{any_rules_apply, RewriteRule, SdRule};

/// Tests that demonstrate the bug with `would_modify` checks in `applies_to` method.
///
/// The bug: If a rule has a pattern like ".*" that would set album_artist to a constant
/// value "Some Constant", and some tracks already have "Some Constant" as their
/// album_artist, the `would_modify` check returns false, causing the rule to not apply
/// even though it should.

#[test_log::test]
fn should_apply_rule_even_when_album_artist_already_matches_target() {
    // Create two tracks: one without album_artist, one that already has the target value
    let track_without_album_artist = Track {
        name: "Song 1".to_string(),
        artist: "Artist 1".to_string(),
        album: Some("Album 1".to_string()),
        album_artist: None, // This will be treated as empty string ""
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let track_with_target_album_artist = Track {
        name: "Song 2".to_string(),
        artist: "Artist 2".to_string(),
        album: Some("Album 2".to_string()),
        album_artist: Some("Target Artist".to_string()), // Already has the target value
        timestamp: Some(1234567891),
        playcount: 0,
    };

    // Rule that sets album_artist to a constant value for any track
    let rule = RewriteRule::new().with_album_artist_name(SdRule::new(".*", "Target Artist"));

    // The bug: The rule should apply to BOTH tracks because the pattern ".*" matches both
    // But currently it only applies to the first track because would_modify returns false
    // for the second track (since "Target Artist" == "Target Artist", no change would occur)

    // This should pass (currently works)
    assert!(
        rule.matches(&track_without_album_artist).unwrap(),
        "Rule should apply to track without album_artist (pattern matches, would modify)"
    );

    // This should pass but currently FAILS due to the bug
    assert!(
        rule.matches(&track_with_target_album_artist).unwrap(),
        "BUG: Rule should apply to track that already has target album_artist (pattern matches, even if no change)"
    );
}

#[test_log::test]
fn should_apply_rule_even_when_artist_name_already_matches_target() {
    // Similar test for artist name
    let track_different_artist = Track {
        name: "Song 1".to_string(),
        artist: "Old Artist".to_string(),
        album: Some("Album 1".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let track_already_target_artist = Track {
        name: "Song 2".to_string(),
        artist: "New Artist".to_string(), // Already has the target value
        album: Some("Album 2".to_string()),
        album_artist: None,
        timestamp: Some(1234567891),
        playcount: 0,
    };

    // Rule that normalizes all artist names to "New Artist"
    let rule = RewriteRule::new().with_artist_name(SdRule::new(".*", "New Artist"));

    // This should pass (currently works)
    assert!(
        rule.matches(&track_different_artist).unwrap(),
        "Rule should apply to track with different artist (pattern matches, would modify)"
    );

    // This should pass but currently FAILS due to the bug
    assert!(
        rule.matches(&track_already_target_artist).unwrap(),
        "BUG: Rule should apply to track that already has target artist (pattern matches, even if no change)"
    );
}

#[test_log::test]
fn should_normalize_track_name_regardless_of_existing_value() {
    // Test case where we normalize track names to title case
    let track_lowercase = Track {
        name: "song title".to_string(),
        artist: "Artist".to_string(),
        album: Some("Album".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let track_already_titlecase = Track {
        name: "Song Title".to_string(), // Already in target format
        artist: "Artist".to_string(),
        album: Some("Album".to_string()),
        album_artist: None,
        timestamp: Some(1234567891),
        playcount: 0,
    };

    // Rule that sets all track names to "Song Title" (normalization)
    let rule = RewriteRule::new().with_track_name(SdRule::new(".*", "Song Title"));

    // This should pass (currently works)
    assert!(
        rule.matches(&track_lowercase).unwrap(),
        "Rule should apply to lowercase track (pattern matches, would modify)"
    );

    // This should pass but currently FAILS due to the bug
    assert!(
        rule.matches(&track_already_titlecase).unwrap(),
        "BUG: Rule should apply to track already in title case (pattern matches, even if no change)"
    );
}

#[test_log::test]
fn handles_multiple_fields_with_mixed_scenarios() {
    // Test a rule that affects multiple fields where some tracks need changes and others don't
    let track_needs_all_changes = Track {
        name: "old song".to_string(),
        artist: "old artist".to_string(),
        album: Some("old album".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let track_needs_some_changes = Track {
        name: "New Song".to_string(),         // Already correct
        artist: "old artist".to_string(),     // Needs change
        album: Some("New Album".to_string()), // Already correct
        album_artist: None,
        timestamp: Some(1234567891),
        playcount: 0,
    };

    let track_needs_no_changes = Track {
        name: "New Song".to_string(),         // Already correct
        artist: "New Artist".to_string(),     // Already correct
        album: Some("New Album".to_string()), // Already correct
        album_artist: None,
        timestamp: Some(1234567892),
        playcount: 0,
    };

    // Rule that normalizes all fields
    let rule = RewriteRule::new()
        .with_track_name(SdRule::new(".*", "New Song"))
        .with_artist_name(SdRule::new(".*", "New Artist"))
        .with_album_name(SdRule::new(".*", "New Album"));

    // This should pass (currently works) - all fields need changes
    assert!(
        rule.matches(&track_needs_all_changes).unwrap(),
        "Rule should apply when all fields need changes"
    );

    // This should pass (currently works) - some fields need changes
    assert!(
        rule.matches(&track_needs_some_changes).unwrap(),
        "Rule should apply when some fields need changes"
    );

    // This should pass but currently FAILS due to the bug - pattern matches all fields but no changes needed
    assert!(
        rule.matches(&track_needs_no_changes).unwrap(),
        "BUG: Rule should apply even when no fields need changes (all patterns match)"
    );
}

#[test_log::test]
fn should_demonstrate_excessive_logging_issue() {
    // This test demonstrates the excessive logging issue
    // Every call to applies_to generates multiple trace logs per rule
    let tracks = vec![
        Track {
            name: "Song 1".to_string(),
            artist: "Artist 1".to_string(),
            album: Some("Album 1".to_string()),
            album_artist: None,
            timestamp: Some(1234567890),
            playcount: 0,
        },
        Track {
            name: "Song 2".to_string(),
            artist: "Artist 2".to_string(),
            album: Some("Album 2".to_string()),
            album_artist: None,
            timestamp: Some(1234567891),
            playcount: 0,
        },
        Track {
            name: "Song 3".to_string(),
            artist: "Artist 3".to_string(),
            album: Some("Album 3".to_string()),
            album_artist: None,
            timestamp: Some(1234567892),
            playcount: 0,
        },
    ];

    let rules = vec![
        RewriteRule::new().with_track_name(SdRule::new(".*", "Normalized")),
        RewriteRule::new().with_artist_name(SdRule::new(".*", "Normalized")),
        RewriteRule::new().with_album_name(SdRule::new(".*", "Normalized")),
    ];

    // This will generate excessive trace logging:
    // For each track (3) x each rule (3) x each field check = 9+ trace messages
    // Plus additional traces for each would_modify check
    for track in &tracks {
        let _applies = any_rules_apply(&rules, track).unwrap();
        // Each call generates multiple trace logs per rule per field
    }

    // Instead, we should collect results and log once:
    // "Applied rules: [rule1, rule2] | Skipped rules: [rule3] for track 'Song 1'"
}

#[test_log::test]
fn should_use_matches_instead_of_would_modify_check() {
    // This test shows how the fix should work: use matches() instead of would_modify()
    let track = Track {
        name: "Test Song".to_string(),
        artist: "Target Artist".to_string(), // Already has target value
        album: Some("Test Album".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let rule = RewriteRule::new().with_artist_name(SdRule::new(".*", "Target Artist"));

    // The fix: matches should return true (pattern matches)
    let sd_rule = SdRule::new(".*", "Target Artist");
    assert!(
        sd_rule.matches("Target Artist").unwrap(),
        "matches should return true (pattern matches regardless of output)"
    );

    // The rule should apply based on pattern matching, not change detection
    // This will currently fail but should pass after the fix
    assert!(
        rule.matches(&track).unwrap(),
        "Rule should apply based on pattern matching, not change detection"
    );
}

#[test_log::test]
fn handles_dot_star_pattern_with_constant_value() {
    // This is the EXACT example the user gave that would have failed previously:
    // "A simple example, is where we have a .* for say album artist, and we set it to some
    // constant value, but some tracks already have that constant value."

    // Track that doesn't have the constant value yet
    let track_without_constant = Track {
        name: "Song 1".to_string(),
        artist: "Artist 1".to_string(),
        album: Some("Album 1".to_string()),
        album_artist: None, // Will be treated as empty string ""
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Track that ALREADY HAS the constant value - this is the key test case
    let track_with_constant = Track {
        name: "Song 2".to_string(),
        artist: "Artist 2".to_string(),
        album: Some("Album 2".to_string()),
        album_artist: Some("Some Constant".to_string()), // Already has the target value!
        timestamp: Some(1234567891),
        playcount: 0,
    };

    // Rule with .* pattern that sets album_artist to "Some Constant"
    // This is the exact scenario from the user's example
    let rule = RewriteRule::new().with_album_artist_name(SdRule::new(".*", "Some Constant"));

    // Both tracks should match because the pattern ".*" matches both empty string and "Some Constant"
    assert!(
        rule.matches(&track_without_constant).unwrap(),
        "Rule should match track without the constant value (pattern .* matches empty string)"
    );

    // This is the critical test - with the old would_modify logic, this would have FAILED
    // because "Some Constant" -> "Some Constant" produces no change, so would_modify returned false
    assert!(
        rule.matches(&track_with_constant).unwrap(),
        "CRITICAL: Rule should match track that already has the constant value! Pattern .* matches 'Some Constant', even though no change would occur. This would have failed with the old would_modify logic."
    );

    // Verify the rule works with any_rules_apply too
    let rules = vec![rule];
    assert!(
        any_rules_apply(&rules, &track_without_constant).unwrap(),
        "any_rules_apply should return true for track without constant"
    );

    assert!(
        any_rules_apply(&rules, &track_with_constant).unwrap(),
        "any_rules_apply should return true for track with constant (this was the bug!)"
    );
}
