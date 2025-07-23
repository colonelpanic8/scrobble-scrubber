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
