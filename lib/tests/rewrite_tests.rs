use lastfm_edit::Track;
use scrobble_scrubber::rewrite::{
    any_rules_apply, apply_all_rules, create_no_op_edit, RewriteRule, SdRule,
};

#[test]
fn test_sd_rule_regex() {
    // With new behavior: if pattern matches, entire string is replaced
    let rule = SdRule::new(r"(\d{4}) Remaster", "$1 Version");
    assert_eq!(
        rule.apply("Song - 2023 Remaster").unwrap(),
        "2023 Version" // Entire string replaced with capture group expansion
    );
}

#[test]
fn test_sd_rule_exact_match() {
    // With regex behavior: if pattern matches exactly, entire string is replaced
    let rule = SdRule::new("feat\\.", "featuring");
    assert_eq!(
        rule.apply("Artist feat. Someone").unwrap(),
        "featuring" // Entire string replaced with regex replacement
    );
}

#[test]
fn test_rewrite_rule_application() {
    let track = Track {
        name: "Song - 2023 Remaster".to_string(),
        artist: "Artist ft. Someone".to_string(),
        album: None,
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
    assert_eq!(edit.track_name, "Clean Song"); // Entire string replaced
    assert_eq!(edit.artist_name, "Clean Artist"); // Entire string replaced
    assert_eq!(edit.timestamp, 1234567890);
}

#[test]
fn test_capture_groups() {
    // This test still works the same way with whole-string replacement
    let rule = SdRule::new(r"(.+) ft\. (.+)", "$1 feat. $2");
    assert_eq!(
        rule.apply("Artist ft. Someone").unwrap(),
        "Artist feat. Someone" // Entire string replaced with capture group expansion
    );
}

#[test]
fn test_no_changes() {
    let track = Track {
        name: "Clean Song".to_string(),
        artist: "Clean Artist".to_string(),
        album: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let rule = RewriteRule::new().with_track_name(SdRule::new(r" - \d{4} Remaster", ""));

    assert!(!rule.applies_to(&track).unwrap());

    let mut edit = create_no_op_edit(&track);
    let changed = rule.apply(&mut edit).unwrap();

    assert!(!changed);
}

#[test]
fn test_multiple_rules_application() {
    let track = Track {
        name: "Song - 2023 Remaster  ".to_string(), // Extra spaces
        artist: "Artist ft. Someone".to_string(),
        album: None,
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
    assert_eq!(edit.track_name, "Song"); // Entire string replaced
    assert_eq!(edit.artist_name, "Artist feat. Someone"); // Entire string replaced
}

#[test]
fn test_applies_to_check() {
    let track = Track {
        name: "Song - 2023 Remaster".to_string(),
        artist: "Clean Artist".to_string(),
        album: None,
        timestamp: Some(1234567890),
        playcount: 0,
    };

    let rule_that_applies =
        RewriteRule::new().with_track_name(SdRule::new(r".*- \d{4} Remaster", "Clean"));

    let rule_that_doesnt_apply =
        RewriteRule::new().with_track_name(SdRule::new(r"Nonexistent", ""));

    assert!(rule_that_applies.applies_to(&track).unwrap());
    assert!(!rule_that_doesnt_apply.applies_to(&track).unwrap());
}

#[test]
fn test_applies_to_requires_all_non_empty_rules_to_match() {
    let track = Track {
        name: "Song - 2023 Remaster".to_string(),
        artist: "Artist ft. Someone".to_string(),
        album: Some("Album Name".to_string()),
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
        rule_all_match.applies_to(&track).unwrap(),
        "Rule should apply when all non-empty regexes match"
    );
    assert!(
        !rule_partial_match.applies_to(&track).unwrap(),
        "Rule should NOT apply when only some regexes match"
    );
    assert!(
        !rule_partial_match_2.applies_to(&track).unwrap(),
        "Rule should NOT apply when only some regexes match"
    );
    assert!(
        !rule_no_match.applies_to(&track).unwrap(),
        "Rule should NOT apply when no regexes match"
    );
    assert!(
        rule_with_empty_field.applies_to(&track).unwrap(),
        "Rule should apply when all present (non-None) regexes match"
    );
}

#[test]
fn test_applies_to_with_album_fields() {
    let track = Track {
        name: "Song Title".to_string(),
        artist: "Artist Name".to_string(),
        album: Some("Album - 2023 Edition".to_string()),
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
        rule_both_match.applies_to(&track).unwrap(),
        "Rule should apply when all non-empty regexes match including album"
    );
    assert!(
        !rule_partial.applies_to(&track).unwrap(),
        "Rule should NOT apply when album regex doesn't match"
    );
}

#[test]
fn test_chris_thile_multiple_conditions_must_all_match() {
    // Test case: Chris Thile track from "Not All Who Wander Are Lost" album
    // Should NOT match a rule that requires both artist "^Chris Thile$" AND album "^Sleep With One Eye Open"
    // because the album doesn't match (only artist matches)
    let track = Track {
        name: "Some Song".to_string(),
        artist: "Chris Thile".to_string(),
        album: Some("Not All Who Wander Are Lost".to_string()),
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Rule with both artist and album conditions - both must match for rule to apply
    let rule_both_conditions = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Chris Thile$", "Chris Thile (Modified)")) // This matches and modifies
        .with_album_name(SdRule::new(r"^Sleep With One Eye Open", "Sleep With One Eye Open (Modified)")); // This does NOT match

    // Rule with only artist condition (for comparison)
    let rule_artist_only = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Chris Thile$", "Chris Thile (Modified)")); // This matches and modifies

    // Rule with only album condition (for comparison)  
    let rule_album_only = RewriteRule::new()
        .with_album_name(SdRule::new(r"^Sleep With One Eye Open", "Sleep With One Eye Open (Modified)")); // This does NOT match

    assert!(
        !rule_both_conditions.applies_to(&track).unwrap(),
        "Rule should NOT apply when artist matches but album doesn't - ALL conditions must match"
    );
    
    assert!(
        rule_artist_only.applies_to(&track).unwrap(),
        "Rule should apply when only artist condition is present and it matches"
    );
    
    assert!(
        !rule_album_only.applies_to(&track).unwrap(),
        "Rule should NOT apply when only album condition is present and it doesn't match"  
    );
}

#[test]
fn test_all_conditions_must_match_comprehensive() {
    // Test various combinations to thoroughly exercise the "ALL must match" requirement
    
    // Test track with specific values
    let track = Track {
        name: "Yesterday".to_string(),
        artist: "The Beatles".to_string(),
        album: Some("Help!".to_string()),
        timestamp: Some(1234567890),
        playcount: 42,
    };

    // Test 1: Two conditions, both match - should apply
    let rule_both_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)"))
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The"));
    
    assert!(
        rule_both_match.applies_to(&track).unwrap(),
        "Rule should apply when both track name and artist match"
    );

    // Test 2: Two conditions, only first matches - should NOT apply
    let rule_first_only = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^Pink Floyd$", "Floyd, Pink")); // doesn't match
    
    assert!(
        !rule_first_only.applies_to(&track).unwrap(),
        "Rule should NOT apply when only track name matches but artist doesn't"
    );

    // Test 3: Two conditions, only second matches - should NOT apply  
    let rule_second_only = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Blackbird$", "Blackbird (Remastered)")) // doesn't match
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The")); // matches
    
    assert!(
        !rule_second_only.applies_to(&track).unwrap(),
        "Rule should NOT apply when only artist matches but track name doesn't"
    );

    // Test 4: Three conditions, all match - should apply
    let rule_three_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)"))
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The"))
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)"));
    
    assert!(
        rule_three_match.applies_to(&track).unwrap(),
        "Rule should apply when track name, artist, and album all match"
    );

    // Test 5: Three conditions, first two match, third doesn't - should NOT apply
    let rule_third_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The")) // matches  
        .with_album_name(SdRule::new(r"^Abbey Road$", "Abbey Road (Remastered)")); // doesn't match
    
    assert!(
        !rule_third_fails.applies_to(&track).unwrap(),
        "Rule should NOT apply when track name and artist match but album doesn't"
    );

    // Test 6: Three conditions, first and third match, second doesn't - should NOT apply
    let rule_middle_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^Led Zeppelin$", "Zeppelin, Led")) // doesn't match
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)")); // matches
    
    assert!(
        !rule_middle_fails.applies_to(&track).unwrap(),
        "Rule should NOT apply when track name and album match but artist doesn't"
    );

    // Test 7: Four conditions (including album_artist), all match - should apply
    let rule_four_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)"))
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The"))
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)"))
        .with_album_artist_name(SdRule::new(r"^$", "The Beatles")); // matches empty string
    
    assert!(
        rule_four_match.applies_to(&track).unwrap(),
        "Rule should apply when all four conditions match (including album_artist on empty string)"
    );

    // Test 8: Four conditions, album_artist fails - should NOT apply
    let rule_album_artist_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Yesterday$", "Yesterday (Remastered)")) // matches
        .with_artist_name(SdRule::new(r"^The Beatles$", "Beatles, The")) // matches
        .with_album_name(SdRule::new(r"^Help!$", "Help! (Remastered)")) // matches
        .with_album_artist_name(SdRule::new(r"^Various Artists$", "V.A.")); // doesn't match empty string
    
    assert!(
        !rule_album_artist_fails.applies_to(&track).unwrap(),
        "Rule should NOT apply when first three match but album_artist condition fails"
    );
}

#[test]
fn test_partial_regex_matches_still_require_all_conditions() {
    // Test that even partial regex matches must satisfy ALL conditions
    
    let track = Track {
        name: "Don't Stop Me Now".to_string(),
        artist: "Queen".to_string(),
        album: Some("Jazz".to_string()),
        timestamp: Some(1234567890),
        playcount: 10,
    };

    // Test 1: Both regexes would partially match but we need ALL to apply
    let rule_partial_matches = RewriteRule::new()
        .with_track_name(SdRule::new(r"Don't", "Do Not")) // would match part of track name
        .with_artist_name(SdRule::new(r"Que", "Q")); // would match part of artist name
    
    assert!(
        rule_partial_matches.applies_to(&track).unwrap(),
        "Rule should apply when both partial matches occur (whole-string replacement behavior)"
    );

    // Test 2: One partial match, one no match
    let rule_mixed = RewriteRule::new()
        .with_track_name(SdRule::new(r"Don't", "Do Not")) // matches part of track name
        .with_artist_name(SdRule::new(r"Beatles", "The Beatles")); // doesn't match anything in artist
    
    assert!(
        !rule_mixed.applies_to(&track).unwrap(),
        "Rule should NOT apply when only one condition has a match"
    );

    // Test 3: Case sensitivity matters for ALL conditions
    let rule_case_sensitive = RewriteRule::new()
        .with_track_name(SdRule::new(r"don't", "do not")) // wrong case, won't match
        .with_artist_name(SdRule::new(r"queen", "QUEEN")); // wrong case, won't match
    
    assert!(
        !rule_case_sensitive.applies_to(&track).unwrap(),
        "Rule should NOT apply when case doesn't match for any condition"
    );

    // Test 4: One case matches, one doesn't
    let rule_mixed_case = RewriteRule::new()
        .with_track_name(SdRule::new(r"Don't", "Do Not")) // correct case, matches
        .with_artist_name(SdRule::new(r"queen", "QUEEN")); // wrong case, doesn't match
    
    assert!(
        !rule_mixed_case.applies_to(&track).unwrap(),
        "Rule should NOT apply when only one condition matches case correctly"
    );
}

#[test]
fn test_edge_cases_all_conditions_must_match() {
    // Test edge cases for the ALL conditions requirement
    
    let track = Track {
        name: "".to_string(), // empty track name
        artist: "Unknown Artist".to_string(),
        album: None, // no album
        timestamp: Some(1234567890),
        playcount: 0,
    };

    // Test 1: Rule matching empty track name and existing artist
    let rule_empty_track = RewriteRule::new()
        .with_track_name(SdRule::new(r"^$", "Untitled")) // matches empty string
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")); // matches
    
    assert!(
        rule_empty_track.applies_to(&track).unwrap(),
        "Rule should apply when both empty track name and artist conditions match"
    );

    // Test 2: Rule trying to match non-empty track name (should fail) and artist (should pass)
    let rule_non_empty_track = RewriteRule::new()
        .with_track_name(SdRule::new(r"^Something$", "Something Else")) // doesn't match empty string
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")); // matches
    
    assert!(
        !rule_non_empty_track.applies_to(&track).unwrap(),
        "Rule should NOT apply when track name condition fails even if artist matches"
    );

    // Test 3: Rule matching empty album (None becomes empty string)
    let rule_empty_album = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")) // matches
        .with_album_name(SdRule::new(r"^$", "Unknown Album")); // matches empty string (None album)
    
    assert!(
        rule_empty_album.applies_to(&track).unwrap(),
        "Rule should apply when artist matches and album condition matches empty string"
    );

    // Test 4: Rule trying to match non-empty album when track has None
    let rule_non_empty_album = RewriteRule::new()
        .with_artist_name(SdRule::new(r"^Unknown Artist$", "Various Artists")) // matches
        .with_album_name(SdRule::new(r"^Some Album$", "Another Album")); // doesn't match empty string
    
    assert!(
        !rule_non_empty_album.applies_to(&track).unwrap(),
        "Rule should NOT apply when artist matches but album condition fails on None/empty"
    );
}

#[test]
fn test_complex_regex_patterns_all_must_match() {
    // Test complex regex patterns where ALL conditions must match
    
    let track = Track {
        name: "Track 01 - Song Title (feat. Artist B)".to_string(),
        artist: "Artist A feat. Artist B".to_string(),
        album: Some("Album Name [Deluxe Edition] (2023)".to_string()),
        timestamp: Some(1234567890),
        playcount: 5,
    };

    // Test 1: Complex patterns that all match
    let rule_complex_all_match = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track \d+ - .+ \(feat\. .+\)", "Song Title")) // matches complex track pattern
        .with_artist_name(SdRule::new(r".+ feat\. .+", "Artist A & Artist B")) // matches featuring pattern
        .with_album_name(SdRule::new(r".+ \[.+\] \(\d{4}\)", "Album Name")); // matches deluxe edition pattern
    
    assert!(
        rule_complex_all_match.applies_to(&track).unwrap(),
        "Rule should apply when all complex regex patterns match"
    );

    // Test 2: Two complex patterns match, one doesn't
    let rule_complex_partial = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track \d+ - .+ \(feat\. .+\)", "Song Title")) // matches
        .with_artist_name(SdRule::new(r".+ feat\. .+", "Artist A & Artist B")) // matches
        .with_album_name(SdRule::new(r".+ \[.+\] \(\d{3}\)", "Album Name")); // doesn't match (wrong year pattern)
    
    assert!(
        !rule_complex_partial.applies_to(&track).unwrap(),
        "Rule should NOT apply when only some complex patterns match"
    );

    // Test 3: Capture groups in ALL patterns - all must match for rule to apply
    let rule_with_captures = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track (\d+) - (.+) \(feat\. (.+)\)", "$2")) // matches and captures
        .with_artist_name(SdRule::new(r"(.+) feat\. (.+)", "$1 & $2")) // matches and captures
        .with_album_name(SdRule::new(r"(.+) \[(.+)\] \((\d{4})\)", "$1 ($3)")); // matches and captures
    
    assert!(
        rule_with_captures.applies_to(&track).unwrap(),
        "Rule should apply when all capture group patterns match"
    );

    // Test 4: One capture pattern fails
    let rule_capture_fails = RewriteRule::new()
        .with_track_name(SdRule::new(r"Track (\d+) - (.+) \(feat\. (.+)\)", "$2")) // matches
        .with_artist_name(SdRule::new(r"(.+) feat\. (.+)", "$1 & $2")) // matches
        .with_album_name(SdRule::new(r"(.+) \[(.+)\] \((\d{2})\)", "$1 ($3)")); // doesn't match (2-digit year)
    
    assert!(
        !rule_capture_fails.applies_to(&track).unwrap(),
        "Rule should NOT apply when one capture pattern fails even if others match"
    );
}
