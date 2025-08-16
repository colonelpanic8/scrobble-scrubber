use lastfm_edit::{ScrobbleEdit, Track};
use scrobble_scrubber::rewrite::{
    any_rules_apply, apply_all_rules, create_no_op_edit, RewriteRule, SdRule,
};

#[test_log::test]
fn should_replace_using_regex_pattern() {
    // With new behavior: if pattern matches, entire string is replaced
    let rule = SdRule::new(r"(\d{4}) Remaster", "$1 Version");
    assert_eq!(
        rule.apply("Song - 2023 Remaster").unwrap(),
        "2023 Version" // Entire string replaced with capture group expansion
    );
}

#[test_log::test]
fn should_match_exact_strings() {
    // With regex behavior: if pattern matches exactly, entire string is replaced
    let rule = SdRule::new("feat\\.", "featuring");
    assert_eq!(
        rule.apply("Artist feat. Someone").unwrap(),
        "featuring" // Entire string replaced with regex replacement
    );
}

#[test_log::test]
fn should_apply_rewrite_rules_to_track() {
    let track = Track {
        name: "Song - 2023 Remaster".to_string(),
        artist: "Artist ft. Someone".to_string(),
        album: None,
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Updated rules for whole-string replacement behavior
    let rule = RewriteRule::new()
        .with_track_name(SdRule::new(r".*- \d{4} Remaster", "Clean Song"))
        .with_artist_name(SdRule::new(r".* ft\. .*", "Clean Artist"));

    let mut edit = create_no_op_edit(&track);
    let changed = rule.apply(&mut edit).unwrap();

    assert!(changed);
    assert_eq!(edit.track_name, Some("Clean Song".to_string())); // Entire string replaced
    assert_eq!(edit.artist_name, "Clean Artist"); // Entire string replaced
    assert_eq!(edit.timestamp, Some(1234567890));
}

#[test_log::test]
fn should_use_capture_groups_in_replacement() {
    // This test still works the same way with whole-string replacement
    let rule = SdRule::new(r"(.+) ft\. (.+)", "$1 feat. $2");
    assert_eq!(
        rule.apply("Artist ft. Someone").unwrap(),
        "Artist feat. Someone" // Entire string replaced with capture group expansion
    );
}

#[test_log::test]
fn should_leave_track_unchanged_when_no_match() {
    let track = Track {
        name: "Clean Song".to_string(),
        artist: "Clean Artist".to_string(),
        album: None,
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let rule = RewriteRule::new().with_track_name(SdRule::new(r" - \d{4} Remaster", ""));

    assert!(!rule.matches(&track).unwrap());

    let mut edit = create_no_op_edit(&track);
    let changed = rule.apply(&mut edit).unwrap();

    assert!(!changed);
}

#[test_log::test]
fn should_apply_multiple_rules_sequentially() {
    let track = Track {
        name: "Song - 2023 Remaster  ".to_string(), // Extra spaces
        artist: "Artist ft. Someone".to_string(),
        album: None,
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Updated rules for whole-string replacement behavior
    let rules = vec![
        RewriteRule::new().with_track_name(SdRule::new(r".*- \d{4} Remaster.*", "Song")),
        RewriteRule::new().with_artist_name(SdRule::new(r".* ft\. .*", "Artist feat. Someone")),
    ];

    // Check that rules apply
    assert!(any_rules_apply(&rules, &track).unwrap());

    // Apply all rules
    let mut edit = create_no_op_edit(&track);
    let changed = apply_all_rules(&rules, &mut edit).unwrap();

    assert!(changed);
    assert_eq!(edit.track_name, Some("Song".to_string())); // Entire string replaced
    assert_eq!(edit.artist_name, "Artist feat. Someone"); // Entire string replaced
}

#[test_log::test]
fn should_check_if_rule_applies_to_track() {
    let track = Track {
        name: "Song - 2023 Remaster".to_string(),
        artist: "Clean Artist".to_string(),
        album: None,
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let rule_that_applies =
        RewriteRule::new().with_track_name(SdRule::new(r".*- \d{4} Remaster", "Clean"));

    let rule_that_doesnt_apply =
        RewriteRule::new().with_track_name(SdRule::new(r"Nonexistent", ""));

    assert!(rule_that_applies.matches(&track).unwrap());
    assert!(!rule_that_doesnt_apply.matches(&track).unwrap());
}

#[test_log::test]
fn should_require_all_non_empty_rules_to_match() {
    let track = Track {
        name: "Song - 2023 Remaster".to_string(),
        artist: "Artist ft. Someone".to_string(),
        album: Some("Album Name".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Rule where both track_name and artist_name regexes match - should apply
    let rule_all_match = RewriteRule::new()
        .with_track_name(SdRule::new(r".*- \d{4} Remaster", "Clean Song"))
        .with_artist_name(SdRule::new(r".* ft\. .*", "Clean Artist"));

    // Rule where track_name matches but artist_name doesn't - should NOT apply
    let rule_partial_match = RewriteRule::new()
        .with_track_name(SdRule::new(r".*- \d{4} Remaster", "Clean Song"))
        .with_artist_name(SdRule::new(r"NonexistentPattern", "Clean Artist"));

    // Rule where artist_name matches but track_name doesn't - should NOT apply
    let rule_partial_match_2 = RewriteRule::new()
        .with_track_name(SdRule::new(r"NonexistentPattern", "Clean Song"))
        .with_artist_name(SdRule::new(r".* ft\. .*", "Clean Artist"));

    // Rule where neither matches - should NOT apply
    let rule_no_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"NonexistentPattern1", "Clean Song"))
        .with_artist_name(SdRule::new(r"NonexistentPattern2", "Clean Artist"));

    // Rule with some fields matching and some empty (empty fields should be ignored)
    let rule_with_empty_field =
        RewriteRule::new().with_track_name(SdRule::new(r".*- \d{4} Remaster", "Clean Song"));
    // No artist_name rule - this empty field should be ignored

    assert!(
        rule_all_match.matches(&track).unwrap(),
        "Rule should apply when all non-empty regexes match"
    );
    assert!(
        !rule_partial_match.matches(&track).unwrap(),
        "Rule should NOT apply when only some regexes match"
    );
    assert!(
        !rule_partial_match_2.matches(&track).unwrap(),
        "Rule should NOT apply when only some regexes match"
    );
    assert!(
        !rule_no_match.matches(&track).unwrap(),
        "Rule should NOT apply when no regexes match"
    );
    assert!(
        rule_with_empty_field.matches(&track).unwrap(),
        "Rule should apply when all present (non-None) regexes match"
    );
}

#[test_log::test]
fn handles_album_fields_in_rule_matching() {
    let track = Track {
        name: "Song Title".to_string(),
        artist: "Artist Name".to_string(),
        album: Some("Album - 2023 Edition".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Rule where track_name and album_name both match - should apply
    let rule_both_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"Song.*", "New Song"))
        .with_album_name(SdRule::new(r".*- \d{4} Edition", "New Album"));

    // Rule where track_name matches but album_name doesn't - should NOT apply
    let rule_partial = RewriteRule::new()
        .with_track_name(SdRule::new(r"Song.*", "New Song"))
        .with_album_name(SdRule::new(r"Nonexistent", "New Album"));

    assert!(
        rule_both_match.matches(&track).unwrap(),
        "Rule should apply when all non-empty regexes match including album"
    );
    assert!(
        !rule_partial.matches(&track).unwrap(),
        "Rule should NOT apply when album regex doesn't match"
    );
}

#[test_log::test]
fn should_match_all_conditions_for_chris_thile_case() {
    // Test case: Chris Thile track from "Not All Who Wander Are Lost" album
    // Should NOT match a rule that requires both artist "^Chris Thile$" AND album "^Sleep With One Eye Open"
    // because the album doesn't match (only artist matches)
    let track = Track {
        name: "Some Song".to_string(),
        artist: "Chris Thile".to_string(),
        album: Some("Not All Who Wander Are Lost".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Rule with both artist and album conditions - both must match for rule to apply
    let rule_both_conditions = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Chris Thile$", "Chris Thile (Modified)")) // This matches and modifies
        .with_album_name(SdRule::new(
            r"^Sleep With One Eye Open",
            "Sleep With One Eye Open (Modified)",
        )); // This does NOT match

    // Rule with only artist condition (for comparison)
    let rule_artist_only = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Chris Thile$", "Chris Thile (Modified)")); // This matches and modifies

    // Rule with only album condition (for comparison)
    let rule_album_only = RewriteRule::new().with_album_name(SdRule::new(
        r"^Sleep With One Eye Open",
        "Sleep With One Eye Open (Modified)",
    )); // This does NOT match

    assert!(
        !rule_both_conditions.matches(&track).unwrap(),
        "Rule should NOT apply when artist matches but album doesn't - ALL conditions must match"
    );

    assert!(
        rule_artist_only.matches(&track).unwrap(),
        "Rule should apply when only artist condition is present and it matches"
    );

    assert!(
        !rule_album_only.matches(&track).unwrap(),
        "Rule should NOT apply when only album condition is present and it doesn't match"
    );
}

#[test_log::test]
fn should_match_all_conditions_comprehensively() {
    // Test various combinations to thoroughly exercise the "ALL must match" requirement

    // Test track with specific values
    let track = Track {
        name: "Yesterday".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Help!".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 42,
    };

    // Test 1: Two conditions, both match - should apply
    let rule_both_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)"))
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The"));

    assert!(
        rule_both_match.matches(&track).unwrap(),
        "Rule should apply when both track name and artist match"
    );

    // Test 2: Two conditions, only first matches - should NOT apply
    let rule_first_only = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^Pink Floyd$", "Floyd, Pink")); // doesn't match

    assert!(
        !rule_first_only.matches(&track).unwrap(),
        "Rule should NOT apply when only track name matches but artist doesn't"
    );

    // Test 3: Two conditions, only second matches - should NOT apply
    let rule_second_only = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Blackbird$", "Blackbird (Remastered)")) // doesn't match
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The")); // matches

    assert!(
        !rule_second_only.matches(&track).unwrap(),
        "Rule should NOT apply when only artist matches but track name doesn't"
    );

    // Test 4: Three conditions, all match - should apply
    let rule_three_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)"))
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The"))
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)"));

    assert!(
        rule_three_match.matches(&track).unwrap(),
        "Rule should apply when track name, artist, and album all match"
    );

    // Test 5: Three conditions, first two match, third doesn't - should NOT apply
    let rule_third_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The")) // matches
        .with_album_name(SdRule::new(r"^Abbey Road$", "Abbey Road (Remastered)")); // doesn't match

    assert!(
        !rule_third_fails.matches(&track).unwrap(),
        "Rule should NOT apply when track name and artist match but album doesn't"
    );

    // Test 6: Three conditions, first and third match, second doesn't - should NOT apply
    let rule_middle_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^Led Zeppelin$", "Zeppelin, Led")) // doesn't match
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)")); // matches

    assert!(
        !rule_middle_fails.matches(&track).unwrap(),
        "Rule should NOT apply when track name and album match but artist doesn't"
    );

    // Test 7: Four conditions (including album_artist), all match - should apply
    let rule_four_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)"))
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The"))
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)"))
        .with_album_artist_name(SdRule::new(r"^$", "The Beatles")); // matches empty string

    assert!(
        rule_four_match.matches(&track).unwrap(),
        "Rule should apply when all four conditions match (including album_artist on empty string)"
    );

    // Test 8: Four conditions, album_artist fails - should NOT apply
    let rule_album_artist_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The")) // matches
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)")) // matches
        .with_album_artist_name(SdRule::new(r"^Various Artists$", "V.A.")); // doesn't match empty string

    assert!(
        !rule_album_artist_fails.matches(&track).unwrap(),
        "Rule should NOT apply when first three match but album_artist condition fails"
    );
}

#[test_log::test]
fn should_require_all_conditions_even_with_partial_matches() {
    // Test that even partial regex matches must satisfy ALL conditions

    let track = Track {
        name: "Don't Stop Me Now".to_string(),
        artist: "Queen".to_string(),
        album: Some("Jazz".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 10,
    };

    // Test 1: Both regexes would partially match but we need ALL to apply
    let rule_partial_matches = RewriteRule::new()
        .with_track_name(SdRule::new(r"Don't", "Do Not")) // would match part of track name
        .with_artist_name(SdRule::new(r"Que", "Q")); // would match part of artist name

    assert!(
        rule_partial_matches.matches(&track).unwrap(),
        "Rule should apply when both partial matches occur (whole-string replacement behavior)"
    );

    // Test 2: One partial match, one no match
    let rule_mixed = RewriteRule::new()
        .with_track_name(SdRule::new(r"Don't", "Do Not")) // matches part of track name
        .with_artist_name(SdRule::new(r"Beatles", "The Beatles")); // doesn't match anything in artist

    assert!(
        !rule_mixed.matches(&track).unwrap(),
        "Rule should NOT apply when only one condition has a match"
    );

    // Test 3: Case sensitivity matters for ALL conditions
    let rule_case_sensitive = RewriteRule::new()
        .with_track_name(SdRule::new(r"don't", "do not")) // wrong case, won't match
        .with_artist_name(SdRule::new(r"queen", "QUEEN")); // wrong case, won't match

    assert!(
        !rule_case_sensitive.matches(&track).unwrap(),
        "Rule should NOT apply when case doesn't match for any condition"
    );

    // Test 4: One case matches, one doesn't
    let rule_mixed_case = RewriteRule::new()
        .with_track_name(SdRule::new(r"Don't", "Do Not")) // correct case, matches
        .with_artist_name(SdRule::new(r"queen", "QUEEN")); // wrong case, doesn't match

    assert!(
        !rule_mixed_case.matches(&track).unwrap(),
        "Rule should NOT apply when only one condition matches case correctly"
    );
}

#[test_log::test]
fn handles_edge_cases_requiring_all_conditions() {
    // Test edge cases for the ALL conditions requirement

    let track = Track {
        name: "".to_string(), // empty track name
        artist: "Unknown Artist".to_string(),
        album: None, // no album
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Test 1: Rule matching empty track name and existing artist
    let rule_empty_track = RewriteRule::new()
        .with_track_name(SdRule::new(r"^$", "Untitled")) // matches empty string
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")); // matches

    assert!(
        rule_empty_track.matches(&track).unwrap(),
        "Rule should apply when both empty track name and artist conditions match"
    );

    // Test 2: Rule trying to match non-empty track name (should fail) and artist (should pass)
    let rule_non_empty_track = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Something$", "Something Else")) // doesn't match empty string
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")); // matches

    assert!(
        !rule_non_empty_track.matches(&track).unwrap(),
        "Rule should NOT apply when track name condition fails even if artist matches"
    );

    // Test 3: Rule matching empty album (None becomes empty string)
    let rule_empty_album = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")) // matches
        .with_album_name(SdRule::new(r"^$", "Unknown Album")); // matches empty string (None album)

    assert!(
        rule_empty_album.matches(&track).unwrap(),
        "Rule should apply when artist matches and album condition matches empty string"
    );

    // Test 4: Rule trying to match non-empty album when track has None
    let rule_non_empty_album = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")) // matches
        .with_album_name(SdRule::new(r"^Some Album$", "Another Album")); // doesn't match empty string

    assert!(
        !rule_non_empty_album.matches(&track).unwrap(),
        "Rule should NOT apply when artist matches but album condition fails on None/empty"
    );
}

#[test_log::test]
fn should_match_complex_regex_patterns_completely() {
    // Test complex regex patterns where ALL conditions must match

    let track = Track {
        name: "Track 01 - Song Title (feat. Artist B)".to_string(),
        artist: "Artist A feat. Artist B".to_string(),
        album: Some("Album Name [Deluxe Edition] (2023)".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 5,
    };

    // Test 1: Complex patterns that all match
    let rule_complex_all_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track \d+ - .+ \(feat\. .+\)", "Song Title")) // matches complex track pattern
        .with_artist_name(SdRule::new(r".+ feat\. .+", "Artist A & Artist B")) // matches featuring pattern
        .with_album_name(SdRule::new(r".+ \[.+\] \(\d{4}\)", "Album Name")); // matches deluxe edition pattern

    assert!(
        rule_complex_all_match.matches(&track).unwrap(),
        "Rule should apply when all complex regex patterns match"
    );

    // Test 2: Two complex patterns match, one doesn't
    let rule_complex_partial = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track \d+ - .+ \(feat\. .+\)", "Song Title")) // matches
        .with_artist_name(SdRule::new(r".+ feat\. .+", "Artist A & Artist B")) // matches
        .with_album_name(SdRule::new(r".+ \[.+\] \(\d{3}\)", "Album Name")); // doesn't match (wrong year pattern)

    assert!(
        !rule_complex_partial.matches(&track).unwrap(),
        "Rule should NOT apply when only some complex patterns match"
    );

    // Test 3: Capture groups in ALL patterns - all must match for rule to apply
    let rule_with_captures = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track (\d+) - (.+) \(feat\. (.+)\)", "$2")) // matches and captures
        .with_artist_name(SdRule::new(r"(.+) feat\. (.+)", "$1 & $2")) // matches and captures
        .with_album_name(SdRule::new(r"(.+) \[(.+)\] \((\d{4})\)", "$1 ($3)")); // matches and captures

    assert!(
        rule_with_captures.matches(&track).unwrap(),
        "Rule should apply when all capture group patterns match"
    );

    // Test 4: One capture pattern fails
    let rule_capture_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track (\d+) - (.+) \(feat\. (.+)\)", "$2")) // matches
        .with_artist_name(SdRule::new(r"(.+) feat\. (.+)", "$1 & $2")) // matches
        .with_album_name(SdRule::new(r"(.+) \[(.+)\] \((\d{2})\)", "$1 ($3)")); // doesn't match (2-digit year)

    assert!(
        !rule_capture_fails.matches(&track).unwrap(),
        "Rule should NOT apply when one capture pattern fails even if others match"
    );
}

#[test_log::test]
fn should_demonstrate_exact_match_behavior_for_chris_thile() {
    // Test the exact scenario the user reported: tracks with artist 'Chris Thile'
    // should match 'Chris Thil' and 'hris Thile' but not 'Chris Thile'
    // This test verifies that the core regex matching logic works correctly

    let track = Track {
        name: "Some Song".to_string(),
        artist: "Chris Thile".to_string(),
        album: Some("Not All Who Wander Are Lost".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Test 1: Partial match 'Chris Thil' should match
    let rule_partial_end =
        RewriteRule::new().with_artist_name(SdRule::new("Chris Thil", "Chris Thile (Modified)"));

    assert!(
        rule_partial_end.matches(&track).unwrap(),
        "Rule with pattern 'Chris Thil' should match artist 'Chris Thile' (partial match)"
    );

    // Test 2: Partial match 'hris Thile' should match
    let rule_partial_start =
        RewriteRule::new().with_artist_name(SdRule::new("hris Thile", "Chris Thile (Modified)"));

    assert!(
        rule_partial_start.matches(&track).unwrap(),
        "Rule with pattern 'hris Thile' should match artist 'Chris Thile' (partial match)"
    );

    // Test 3: Exact match 'Chris Thile' should match - THIS IS THE KEY TEST
    let rule_exact =
        RewriteRule::new().with_artist_name(SdRule::new("Chris Thile", "Chris Thile (Modified)"));

    assert!(
        rule_exact.matches(&track).unwrap(),
        "Rule with pattern 'Chris Thile' should match artist 'Chris Thile' (exact match) - CORE ISSUE"
    );

    // Test 4: Pattern with anchors '^Chris Thile$' should match exactly
    let rule_anchored =
        RewriteRule::new().with_artist_name(SdRule::new("^Chris Thile$", "Chris Thile (Modified)"));

    assert!(
        rule_anchored.matches(&track).unwrap(),
        "Rule with pattern '^Chris Thile$' should match artist 'Chris Thile' (anchored exact match)"
    );

    // Test 5: Pattern that should NOT match
    let rule_no_match =
        RewriteRule::new().with_artist_name(SdRule::new("John Doe", "John Doe (Modified)"));

    assert!(
        !rule_no_match.matches(&track).unwrap(),
        "Rule with pattern 'John Doe' should NOT match artist 'Chris Thile'"
    );
}

#[test_log::test]
fn should_use_anchored_regex_for_exact_matching() {
    // Test that anchored regex patterns (^...$) provide super exact matching behavior

    let track = Track {
        name: "Chris Thile".to_string(),
        artist: "Chris Thile".to_string(),
        album: Some("Not All Who Wander Are Lost".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Test 1: Anchored pattern '^Chris Thile$' should match exactly
    let rule_exact_anchored = RewriteRule::new()
        .with_artist_name(SdRule::new("^Chris Thile$", "Chris Thile (Exact Match)"));

    assert!(
        rule_exact_anchored.matches(&track).unwrap(),
        "Anchored pattern '^Chris Thile$' should match artist 'Chris Thile' exactly"
    );

    // Test 2: Anchored pattern '^Chris Thil$' should NOT match (missing 'e')
    let rule_partial_anchored =
        RewriteRule::new().with_artist_name(SdRule::new("^Chris Thil$", "Chris Thile (Modified)"));

    assert!(
        !rule_partial_anchored.matches(&track).unwrap(),
        "Anchored pattern '^Chris Thil$' should NOT match artist 'Chris Thile' (missing 'e')"
    );

    // Test 3: Anchored pattern '^hris Thile$' should NOT match (missing 'C')
    let rule_partial_start_anchored =
        RewriteRule::new().with_artist_name(SdRule::new("^hris Thile$", "Chris Thile (Modified)"));

    assert!(
        !rule_partial_start_anchored.matches(&track).unwrap(),
        "Anchored pattern '^hris Thile$' should NOT match artist 'Chris Thile' (missing 'C')"
    );

    // Test 4: Start anchor only '^Chris Thile' should match
    let rule_start_anchor = RewriteRule::new()
        .with_artist_name(SdRule::new("^Chris Thile", "Chris Thile (Start Anchored)"));

    assert!(
        rule_start_anchor.matches(&track).unwrap(),
        "Start-anchored pattern '^Chris Thile' should match artist 'Chris Thile'"
    );

    // Test 5: End anchor only 'Chris Thile$' should match
    let rule_end_anchor = RewriteRule::new()
        .with_artist_name(SdRule::new("Chris Thile$", "Chris Thile (End Anchored)"));

    assert!(
        rule_end_anchor.matches(&track).unwrap(),
        "End-anchored pattern 'Chris Thile$' should match artist 'Chris Thile'"
    );

    // Test 6: Verify super exact behavior - anchored pattern with extra chars should NOT match
    let track_longer = Track {
        name: "Some Song".to_string(),
        artist: "Chris Thile and Friends".to_string(),
        album: Some("Album".to_string()),
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let rule_exact_anchored_longer =
        RewriteRule::new().with_artist_name(SdRule::new("^Chris Thile$", "Chris Thile (Exact)"));

    assert!(
        !rule_exact_anchored_longer.matches(&track_longer).unwrap(),
        "Anchored pattern '^Chris Thile$' should NOT match 'Chris Thile and Friends' (super exact)"
    );

    // Test 7: Non-anchored pattern should match the longer string
    let rule_non_anchored_longer =
        RewriteRule::new().with_artist_name(SdRule::new("Chris Thile", "Chris Thile (Modified)"));

    assert!(
        rule_non_anchored_longer.matches(&track_longer).unwrap(),
        "Non-anchored pattern 'Chris Thile' should match 'Chris Thile and Friends'"
    );
}

#[test_log::test]
fn handles_queen_track_with_chris_thile_rules() {
    // Replicate the exact issue from the log:
    // [2025-07-24T23:44:25Z INFO  scrobble_scrubber::scrubber] Applying edit to track 'You And I - Remastered 2011' by 'Queen':
    // track: 'You And I - Remastered 2011' -> 'You And I', album artist: 'unknown' -> 'Chris Thile & Michael Daves'

    let queen_track = Track {
        name: "You And I - Remastered 2011".to_string(),
        artist: "Queen".to_string(),
        album: None, // This would be empty/None, which means album_artist would be treated as empty/unknown
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Recreate the actual rules from the CLI output:

    // Rule 1: Remove Dash Remaster of any sort
    let rule1 = RewriteRule::new()
        .with_name("Remove Dash Remaster of any sort")
        .with_track_name(SdRule::new(r"(.+) -.*Remaster.*", "$1"));

    // Rule 5: Rule #5 - This is the suspected culprit
    let rule5 = RewriteRule::new()
        .with_name("Rule #5")
        .with_artist_name(SdRule::new("^Chris Thile$", "Chris Thile & Micheal Daves"))
        .with_album_name(SdRule::new("Sleep With One Eye Open", "$0"))
        .with_album_artist_name(SdRule::new(".*", "Chris Thile & Micheal Daves"));

    // Rule 6: Rule #6
    let rule6 = RewriteRule::new()
        .with_name("Rule #6")
        .with_artist_name(SdRule::new("^Chris Thile & Micheal Daves$", "$0"))
        .with_album_artist_name(SdRule::new(".*", "Chris Thile & Micheal Daves"));

    // Rule 7: Rule #7 (note the corrected spelling of "Michael")
    let rule7 = RewriteRule::new()
        .with_name("Rule #7")
        .with_artist_name(SdRule::new(
            "^chris thile & micheal daves$",
            "Chris Thile & Michael Daves",
        ))
        .with_album_artist_name(SdRule::new(".*", "Chris Thile & Michael Daves"));

    println!("Testing individual rules against Queen track:");

    // Test each rule individually
    println!(
        "Rule 1 (remaster removal) matches: {}",
        rule1.matches(&queen_track).unwrap()
    );
    println!(
        "Rule 5 (Chris Thile) matches: {}",
        rule5.matches(&queen_track).unwrap()
    );
    println!(
        "Rule 6 (Chris Thile & Micheal) matches: {}",
        rule6.matches(&queen_track).unwrap()
    );
    println!(
        "Rule 7 (chris thile lowercase) matches: {}",
        rule7.matches(&queen_track).unwrap()
    );

    // Apply the rules to see what happens
    let rules = vec![rule1.clone(), rule5.clone(), rule6.clone(), rule7.clone()];
    let mut edit = create_no_op_edit(&queen_track);

    println!("Original edit: {edit:?}");
    let changed = apply_all_rules(&rules, &mut edit).unwrap();
    println!("After applying rules - changed: {changed}, edit: {edit:?}");

    // This test is to understand the issue, not assert specific behavior
    // We expect:
    // 1. Rule 1 should match and remove "- Remastered 2011"
    // 2. Rules 5, 6, 7 should NOT match because artist != "Chris Thile" variants

    // Rule 1 should match (remaster removal)
    assert!(
        rule1.matches(&queen_track).unwrap(),
        "Rule 1 should match Queen track for remaster removal"
    );

    // Rules 5, 6, 7 should NOT match because artist conditions don't match
    assert!(
        !rule5.matches(&queen_track).unwrap(),
        "Rule 5 should NOT match - artist is Queen, not Chris Thile"
    );
    assert!(
        !rule6.matches(&queen_track).unwrap(),
        "Rule 6 should NOT match - artist is Queen, not Chris Thile & Micheal Daves"
    );
    assert!(
        !rule7.matches(&queen_track).unwrap(),
        "Rule 7 should NOT match - artist is Queen, not chris thile & micheal daves"
    );

    // ✅ FIXED: Rules 5, 6, 7 now correctly don't apply because they don't match
    // The apply() method now checks matches() first before applying any field transformations

    // Verify that only the expected changes occurred:
    // - track_name should be changed from "You And I - Remastered 2011" to "You And I" (rule 1)
    // - album_artist_name should remain "Queen" (no Chris Thile rules should apply)
    assert_eq!(
        edit.track_name,
        Some("You And I".to_string()),
        "Track name should be cleaned by rule 1"
    );
    assert_eq!(
        edit.album_artist_name,
        Some("Queen".to_string()),
        "Album artist should remain Queen - no Chris Thile rules should apply"
    );

    println!("✅ FIXED: Only rule 1 applied as expected. Rules 5, 6, 7 correctly did not apply.");
}

#[test_log::test]
fn should_have_consistent_apply_and_matches_behavior() {
    // This test demonstrates the bug where apply() doesn't check matches() first
    let track = Track {
        name: "Some Song".to_string(),
        artist: "Queen".to_string(), // This won't match the artist condition
        album: None,
        album_artist: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Rule with artist condition that WON'T match + album_artist condition that WILL match
    let rule = RewriteRule::new()
        .with_name("Inconsistent Rule")
        .with_artist_name(SdRule::new("^Chris Thile$", "Modified Artist")) // Won't match "Queen"
        .with_album_artist_name(SdRule::new(".*", "Modified Album Artist")); // Will match anything

    let mut edit = create_no_op_edit(&track);
    println!("Before: {edit:?}");

    // The rule should NOT match because artist condition fails
    assert!(
        !rule.matches(&track).unwrap(),
        "Rule should not match because artist != Chris Thile"
    );

    // The rule should NOT match the ScrobbleEdit either because artist condition fails
    assert!(
        !rule.matches_scrobble_edit(&edit).unwrap(),
        "Rule should not match ScrobbleEdit because artist != Chris Thile"
    );

    // Test using apply_all_rules which properly filters rules before applying
    let changed = apply_all_rules(std::slice::from_ref(&rule), &mut edit).unwrap();
    println!("After: {edit:?}");
    println!("Changed: {changed}");

    // ✅ FIXED: apply_all_rules now correctly filters rules and returns false when rule doesn't match
    assert!(
        !changed,
        "Rule should not apply any changes when it doesn't match"
    );
    assert_eq!(
        edit.album_artist_name,
        Some("Queen".to_string()),
        "Album artist should remain unchanged when rule doesn't match"
    );

    println!("✅ FIXED: Rule correctly did not apply changes because it doesn't match.");
}

#[test_log::test]
fn handles_none_values_semantically_in_scrobble_edit() {
    // Test the semantic None handling in matches_scrobble_edit

    // Case 1: Rule field None, ScrobbleEdit field None -> MATCH
    let rule1 = RewriteRule::new()
        .with_name("No constraints rule")
        .with_artist_name(SdRule::new("^Artist$", "Modified Artist"));
    // track_name, album_name, album_artist_name are None in the rule

    let edit1 = ScrobbleEdit {
        track_name_original: Some("Song".to_string()),
        album_name_original: None,
        artist_name_original: "Artist".to_string(),
        album_artist_name_original: None,
        track_name: None, // Rule doesn't care about this field
        album_name: None, // Rule doesn't care about this field
        artist_name: "Artist".to_string(),
        album_artist_name: None, // Rule doesn't care about this field
        timestamp: Some(1234567890),
        edit_all: false,
    };

    assert!(
        rule1.matches_scrobble_edit(&edit1).unwrap(),
        "Rule with None fields should match ScrobbleEdit with None fields"
    );

    // Case 2: Rule field None, ScrobbleEdit field Some -> MATCH
    let edit2 = ScrobbleEdit {
        track_name_original: Some("Song".to_string()),
        album_name_original: None,
        artist_name_original: "Artist".to_string(),
        album_artist_name_original: None,
        track_name: Some("Song".to_string()), // Rule doesn't care about this field
        album_name: Some("Album".to_string()), // Rule doesn't care about this field
        artist_name: "Artist".to_string(),
        album_artist_name: Some("Album Artist".to_string()), // Rule doesn't care about this field
        timestamp: Some(1234567890),
        edit_all: false,
    };

    assert!(
        rule1.matches_scrobble_edit(&edit2).unwrap(),
        "Rule with None fields should match ScrobbleEdit with Some fields"
    );

    // Case 3: Rule field Some, ScrobbleEdit field None -> NO MATCH
    let rule3 = RewriteRule::new()
        .with_name("Requires track name rule")
        .with_track_name(SdRule::new("^Song$", "Modified Song"))
        .with_artist_name(SdRule::new("^Artist$", "Modified Artist"));

    let edit3 = ScrobbleEdit {
        track_name_original: Some("Song".to_string()),
        album_name_original: None,
        artist_name_original: "Artist".to_string(),
        album_artist_name_original: None,
        track_name: None, // Rule requires this field but it's None
        album_name: None,
        artist_name: "Artist".to_string(),
        album_artist_name: None,
        timestamp: Some(1234567890),
        edit_all: false,
    };

    assert!(
        !rule3.matches_scrobble_edit(&edit3).unwrap(),
        "Rule with Some field should NOT match ScrobbleEdit with None field"
    );

    // Case 4: Rule field Some, ScrobbleEdit field Some -> Check pattern match
    let edit4 = ScrobbleEdit {
        track_name_original: Some("Song".to_string()),
        album_name_original: None,
        artist_name_original: "Artist".to_string(),
        album_artist_name_original: None,
        track_name: Some("Song".to_string()),
        album_name: None,
        artist_name: "Artist".to_string(),
        album_artist_name: None,
        timestamp: Some(1234567890),
        edit_all: false,
    };

    assert!(
        rule3.matches_scrobble_edit(&edit4).unwrap(),
        "Rule with Some field should match ScrobbleEdit with matching Some field"
    );

    // Case 5: Rule field Some, ScrobbleEdit field Some but pattern doesn't match
    let edit5 = ScrobbleEdit {
        track_name_original: Some("Song".to_string()),
        album_name_original: None,
        artist_name_original: "Artist".to_string(),
        album_artist_name_original: None,
        track_name: Some("Different Song".to_string()), // Doesn't match pattern
        album_name: None,
        artist_name: "Artist".to_string(),
        album_artist_name: None,
        timestamp: Some(1234567890),
        edit_all: false,
    };

    assert!(
        !rule3.matches_scrobble_edit(&edit5).unwrap(),
        "Rule with Some field should NOT match ScrobbleEdit with non-matching Some field"
    );

    println!("✅ All semantic None handling tests passed!");
}

#[test_log::test]
fn handles_dot_star_special_case_in_scrobble_edit() {
    // Test that .* pattern matches None fields (special case)

    // Rule with .* pattern should match None fields
    let rule_dot_star = RewriteRule::new()
        .with_name("Dot star rule")
        .with_artist_name(SdRule::new("^Artist$", "Modified Artist"))
        .with_album_artist_name(SdRule::new(".*", "Some Album Artist")); // .* should match None

    let edit_with_none = ScrobbleEdit {
        track_name_original: Some("Song".to_string()),
        album_name_original: None,
        artist_name_original: "Artist".to_string(),
        album_artist_name_original: None,
        track_name: Some("Song".to_string()),
        album_name: None,
        artist_name: "Artist".to_string(),
        album_artist_name: None, // This is None, but .* should match it
        timestamp: Some(1234567890),
        edit_all: false,
    };

    assert!(
        rule_dot_star
            .matches_scrobble_edit(&edit_with_none)
            .unwrap(),
        "Rule with .* pattern should match ScrobbleEdit with None album_artist_name"
    );

    // Rule with specific pattern should NOT match None fields
    let rule_specific = RewriteRule::new()
        .with_name("Specific pattern rule")
        .with_artist_name(SdRule::new("^Artist$", "Modified Artist"))
        .with_album_artist_name(SdRule::new("^Album Artist$", "Some Album Artist")); // Specific pattern should NOT match None

    assert!(
        !rule_specific
            .matches_scrobble_edit(&edit_with_none)
            .unwrap(),
        "Rule with specific pattern should NOT match ScrobbleEdit with None album_artist_name"
    );

    // .* pattern should also match Some fields
    let edit_with_some = ScrobbleEdit {
        track_name_original: Some("Song".to_string()),
        album_name_original: None,
        artist_name_original: "Artist".to_string(),
        album_artist_name_original: None,
        track_name: Some("Song".to_string()),
        album_name: None,
        artist_name: "Artist".to_string(),
        album_artist_name: Some("Any Album Artist".to_string()), // This has a value
        timestamp: Some(1234567890),
        edit_all: false,
    };

    assert!(
        rule_dot_star
            .matches_scrobble_edit(&edit_with_some)
            .unwrap(),
        "Rule with .* pattern should match ScrobbleEdit with Some album_artist_name"
    );

    println!("✅ Dot star pattern special case tests passed!");
}
