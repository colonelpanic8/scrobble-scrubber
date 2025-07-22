use lastfm_edit::Track;
use scrobble_scrubber::rewrite::{
    any_rules_apply, apply_all_rules, create_no_op_edit, RewriteRule, SdRule,
};

#[test]
fn test_sd_rule_regex() {
    // With new behavior: if pattern matches, entire string is replaced
    let rule = SdRule::new_regex(r"(\d{4}) Remaster", "$1 Version");
    assert_eq!(
        rule.apply("Song - 2023 Remaster").unwrap(),
        "2023 Version" // Entire string replaced with capture group expansion
    );
}

#[test]
fn test_sd_rule_literal() {
    // With new behavior: if literal pattern is found, entire string is replaced
    let rule = SdRule::new_literal("feat.", "featuring");
    assert_eq!(
        rule.apply("Artist feat. Someone").unwrap(),
        "featuring" // Entire string replaced with literal replacement
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
        .with_track_name(SdRule::new_regex(r".*- \d{4} Remaster", "Clean Song"))
        .with_artist_name(SdRule::new_regex(r".* ft\. .*", "Clean Artist"));

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
    let rule = SdRule::new_regex(r"(.+) ft\. (.+)", "$1 feat. $2");
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

    let rule = RewriteRule::new().with_track_name(SdRule::new_regex(r" - \d{4} Remaster", ""));

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
        RewriteRule::new().with_track_name(SdRule::new_regex(r".*- \d{4} Remaster.*", "Song")),
        RewriteRule::new()
            .with_artist_name(SdRule::new_regex(r".* ft\. .*", "Artist feat. Someone")),
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
        RewriteRule::new().with_track_name(SdRule::new_regex(r".*- \d{4} Remaster", "Clean"));

    let rule_that_doesnt_apply =
        RewriteRule::new().with_track_name(SdRule::new_regex(r"Nonexistent", ""));

    assert!(rule_that_applies.applies_to(&track).unwrap());
    assert!(!rule_that_doesnt_apply.applies_to(&track).unwrap());
}
