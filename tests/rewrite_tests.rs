use lastfm_edit::Track;
use scrobble_scrubber::rewrite::{
    any_rules_apply, apply_all_rules, create_no_op_edit, RewriteRule, SdRule,
};

#[test]
fn test_sd_rule_regex() {
    let rule = SdRule::new_regex(r"(\d{4}) Remaster", "$1 Version");
    assert_eq!(
        rule.apply("Song - 2023 Remaster").unwrap(),
        "Song - 2023 Version"
    );
}

#[test]
fn test_sd_rule_literal() {
    let rule = SdRule::new_literal("feat.", "featuring");
    assert_eq!(
        rule.apply("Artist feat. Someone").unwrap(),
        "Artist featuring Someone"
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

    let rule = RewriteRule::new()
        .with_track_name(SdRule::new_regex(r" - \d{4} Remaster", ""))
        .with_artist_name(SdRule::new_regex(r" ft\. ", " feat. "));

    let mut edit = create_no_op_edit(&track);
    let changed = rule.apply(&mut edit).unwrap();

    assert!(changed);
    assert_eq!(edit.track_name, "Song");
    assert_eq!(edit.artist_name, "Artist feat. Someone");
    assert_eq!(edit.timestamp, 1234567890);
}

#[test]
fn test_capture_groups() {
    let rule = SdRule::new_regex(r"(.+) ft\. (.+)", "$1 feat. $2");
    assert_eq!(
        rule.apply("Artist ft. Someone").unwrap(),
        "Artist feat. Someone"
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

    let rules = vec![
        RewriteRule::new().with_track_name(SdRule::new_regex(r" - \d{4} Remaster", "")),
        RewriteRule::new().with_artist_name(SdRule::new_regex(r" ft\. ", " feat. ")),
        RewriteRule::new().with_track_name(SdRule::new_regex(r"\s+$", "")), // Remove trailing spaces
    ];

    // Check that rules apply
    assert!(any_rules_apply(&rules, &track).unwrap());

    // Apply all rules
    let mut edit = create_no_op_edit(&track);
    let changed = apply_all_rules(&rules, &mut edit).unwrap();

    assert!(changed);
    assert_eq!(edit.track_name, "Song"); // Remaster removed and spaces trimmed
    assert_eq!(edit.artist_name, "Artist feat. Someone"); // ft. -> feat.
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
        RewriteRule::new().with_track_name(SdRule::new_regex(r" - \d{4} Remaster", ""));

    let rule_that_doesnt_apply =
        RewriteRule::new().with_track_name(SdRule::new_regex(r"Nonexistent", ""));

    assert!(rule_that_applies.applies_to(&track).unwrap());
    assert!(!rule_that_doesnt_apply.applies_to(&track).unwrap());
}
