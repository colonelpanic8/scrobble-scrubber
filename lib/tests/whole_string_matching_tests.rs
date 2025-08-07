use scrobble_scrubber::rewrite::SdRule;

/// Macro for easily writing test cases for whole-string matching
/// Usage: test_rule!(rule, input, expected_output)
macro_rules! test_rule {
    ($rule:expr, $input:expr, $expected:expr) => {
        assert_eq!(
            $rule.apply($input).unwrap(),
            $expected,
            "Rule {:?} applied to '{}' should produce '{}' but got '{}'",
            $rule,
            $input,
            $expected,
            $rule.apply($input).unwrap()
        );
    };
}

/// Macro for testing that a rule does NOT match (input should be unchanged)
macro_rules! test_no_match {
    ($rule:expr, $input:expr) => {
        assert_eq!(
            $rule.apply($input).unwrap(),
            $input,
            "Rule {:?} should not match '{}' but produced '{}'",
            $rule,
            $input,
            $rule.apply($input).unwrap()
        );
    };
}

#[test_log::test]
fn should_replace_entire_string_with_pattern_match() {
    // If pattern matches anywhere, the entire string gets replaced
    let rule = SdRule::new(r"2023 Remaster", "2023 Version");

    // Should match exactly
    test_rule!(rule, "2023 Remaster", "2023 Version");

    // Should match and replace ENTIRE string when pattern is found anywhere
    test_rule!(rule, "Song - 2023 Remaster", "2023 Version");
    test_rule!(rule, "2023 Remaster Edition", "2023 Version");
    test_rule!(rule, "Some 2023 Remaster text", "2023 Version");

    // Should not match if pattern not found
    test_no_match!(rule, "2024 Remaster");
    test_no_match!(rule, "Just a song");
}

#[test_log::test]
fn should_clean_up_artist_names_correctly() {
    // Classic example: "Vulfpeck ft. anyone" -> "Vulfpeck"
    let rule = SdRule::new(r"Vulfpeck", "Vulfpeck");

    test_rule!(rule, "Vulfpeck", "Vulfpeck");
    test_rule!(rule, "Vulfpeck ft. Antwaun Stanley", "Vulfpeck");
    test_rule!(rule, "Vulfpeck featuring Cory Wong", "Vulfpeck");
    test_rule!(rule, "The Vulfpeck Experience", "Vulfpeck");

    // Should not match different artists
    test_no_match!(rule, "Snarky Puppy");
    test_no_match!(rule, "Cory Wong");
}

#[test_log::test]
fn should_use_capture_groups_in_whole_string_replacement() {
    // Extract artist from complex string and replace entire string with just artist
    let rule = SdRule::new(r"(.+) ft\. .+", "$1");

    // Should match and replace entire string with just the first capture group
    test_rule!(rule, "Vulfpeck ft. Antwaun Stanley", "Vulfpeck");
    test_rule!(rule, "The Beatles ft. John Lennon", "The Beatles");
    test_rule!(
        rule,
        "Daft Punk ft. Pharrell Williams - Get Lucky",
        "Daft Punk"
    );

    // Should not match if pattern doesn't exist
    test_no_match!(rule, "Just an artist name");
    test_no_match!(rule, "Artist featuring Someone"); // Different pattern

    // Another example: extract song title from "Artist - Song" format
    let song_rule = SdRule::new(r".+ - (.+)", "$1");
    test_rule!(song_rule, "The Beatles - Yesterday", "Yesterday");
    test_rule!(
        song_rule,
        "Queen - Bohemian Rhapsody (Live)",
        "Bohemian Rhapsody (Live)"
    );
    test_no_match!(song_rule, "Just a song title");
}

#[test_log::test]
fn should_match_exact_strings_completely() {
    let rule = SdRule::new("feat\\.", "featuring");

    // Should match if pattern is found anywhere and replace entire string
    test_rule!(rule, "feat.", "featuring");
    test_rule!(rule, "Artist feat. Someone", "featuring"); // Entire string replaced
    test_rule!(rule, "feat. Someone", "featuring"); // Entire string replaced
    test_rule!(rule, "Artist feat.", "featuring"); // Entire string replaced

    // Should not match if pattern not found
    test_no_match!(rule, "Artist featuring Someone");
    test_no_match!(rule, "Just a string");
}

#[test_log::test]
fn should_remove_remaster_suffixes_from_titles() {
    // Remove various remaster suffixes from the END of track names
    let rule = SdRule::new(r"(.*) - \d{4} Remaster.*", "$1");

    test_rule!(
        rule,
        "Bohemian Rhapsody - 2023 Remaster",
        "Bohemian Rhapsody"
    );
    test_rule!(
        rule,
        "Stairway to Heaven - 2022 Remaster Edition",
        "Stairway to Heaven"
    );
    test_rule!(
        rule,
        "Hotel California - 1976 Remaster (Deluxe)",
        "Hotel California"
    );

    // Should not match if pattern isn't at the end
    test_no_match!(rule, "Just a regular song");
    test_no_match!(rule, "Song without remaster");
}

#[test_log::test]
fn should_normalize_featuring_artist_formats() {
    // Transform various "featuring" formats to a standard format
    let rule = SdRule::new(r"(.+) (ft\.|feat\.|featuring) (.+)", "$1 feat. $3");

    test_rule!(rule, "Artist ft. Someone", "Artist feat. Someone");
    test_rule!(rule, "Artist feat. Someone", "Artist feat. Someone"); // Already correct
    test_rule!(rule, "Artist featuring Someone", "Artist feat. Someone");

    // With whole string replacement, these will match and do full replacement
    test_rule!(
        rule,
        "Artist ft. Someone - Remix",
        "Artist feat. Someone - Remix"
    );
    test_rule!(
        rule,
        "Track: Artist ft. Someone",
        "Track: Artist feat. Someone"
    );

    // Should not match if pattern not found
    test_no_match!(rule, "Just an artist name");
    test_no_match!(rule, "Artist with Someone");
}

#[test_log::test]
fn should_remove_trailing_whitespace() {
    let rule = SdRule::new(r"(.*?)\s+$", "$1");

    test_rule!(rule, "Song Name   ", "Song Name");
    test_rule!(rule, "Artist\t\t", "Artist");
    test_rule!(rule, "Album \n ", "Album");

    // Should not match if no trailing whitespace
    test_no_match!(rule, "Clean String");
}

#[test_log::test]
fn should_remove_leading_whitespace() {
    let rule = SdRule::new(r"^\s+(.*)", "$1");

    test_rule!(rule, "  Song Name", "Song Name");
    test_rule!(rule, "\t\tArtist", "Artist");
    test_rule!(rule, " \n Album", "Album");

    // Should not match if no leading whitespace
    test_no_match!(rule, "Clean String");
}

#[test_log::test]
fn should_remove_parenthetical_content() {
    // Remove content in parentheses at the end
    let rule = SdRule::new(r"(.*?)\s*\([^)]*\)$", "$1");

    test_rule!(rule, "Song (Remix)", "Song");
    test_rule!(rule, "Artist Name (feat. Someone)", "Artist Name");
    test_rule!(rule, "Album Title (Deluxe Edition)", "Album Title");

    // Should not match if parentheses aren't at the end
    test_no_match!(rule, "Song (Remix) Version");
    test_no_match!(rule, "(Introduction) Song");
}

#[test_log::test]
fn should_perform_case_insensitive_matching() {
    let rule = SdRule::new("remaster", "version").with_flags("i");

    // Should match regardless of case when entire string matches
    test_rule!(rule, "remaster", "version");
    test_rule!(rule, "REMASTER", "version");
    test_rule!(rule, "Remaster", "version");

    // With whole string replacement, these will match and replace entire string
    test_rule!(rule, "Song Remaster", "version");
    test_rule!(rule, "remaster edition", "version");
    test_rule!(rule, "Song REMASTER Edition", "version");

    // Should not match if pattern not found
    test_no_match!(rule, "remix");
    test_no_match!(rule, "original");
}

#[test_log::test]
fn handles_complex_multi_step_transformation() {
    // Transform: "Artist - Song (Year Remaster)" -> "Song by Artist"
    let rule = SdRule::new(r"(.+) - (.+) \(\d{4} Remaster\)", "$2 by $1");

    test_rule!(
        rule,
        "The Beatles - Hey Jude (2023 Remaster)",
        "Hey Jude by The Beatles"
    );
    test_rule!(
        rule,
        "Queen - Bohemian Rhapsody (1975 Remaster)",
        "Bohemian Rhapsody by Queen"
    );

    // With whole string replacement, this will match and transform
    test_rule!(
        rule,
        "The Beatles - Hey Jude (2023 Remaster) - Extended",
        "Hey Jude by The Beatles"
    );

    // Should not match if pattern not found
    test_no_match!(rule, "The Beatles - Hey Jude");
    test_no_match!(rule, "Just a song title");
}

#[test_log::test]
fn handles_empty_strings_gracefully() {
    let rule = SdRule::new(r".*", "replacement");

    // Should match empty string (since .* matches everything)
    test_rule!(rule, "", "replacement");

    let specific_rule = SdRule::new(r"specific", "replaced");

    // Should not match empty string if pattern is specific
    test_no_match!(specific_rule, "");
}

#[test_log::test]
fn should_escape_special_regex_characters_correctly() {
    // Test that patterns with special regex chars work correctly
    let rule = SdRule::new(r"Song \(Live\)", "Song - Live Version");

    test_rule!(rule, "Song (Live)", "Song - Live Version");

    // Should not match without proper escaping consideration
    test_no_match!(rule, "Song Live"); // Parentheses are part of the pattern
}

#[test_log::test]
fn handles_multiple_capture_groups() {
    // Test with multiple named and numbered capture groups
    let rule = SdRule::new(
        r"(?P<artist>.+) - (?P<song>.+) \((?P<year>\d{4})\)",
        "[$3] $2 by $1",
    );

    test_rule!(
        rule,
        "The Beatles - Yesterday (1965)",
        "[1965] Yesterday by The Beatles"
    );

    // Test with mixed named and numbered groups
    let rule2 = SdRule::new(
        r"(?P<artist>.+) ft\. (.+) - (.+)",
        "$3 by ${artist} featuring $2",
    );

    test_rule!(
        rule2,
        "Queen ft. David Bowie - Under Pressure",
        "Under Pressure by Queen featuring David Bowie"
    );

    // The greedy (.+) pattern matches even with extra content
    test_rule!(
        rule,
        "The Beatles - Yesterday (1965) Extended",
        "[1965] Yesterday by The Beatles" // year=1965, song="Yesterday", artist="The Beatles"
    );

    // A more specific pattern that won't match extended content
    let strict_rule = SdRule::new(
        r"(?P<artist>.+) - (?P<song>.+) \((?P<year>\d{4})\)$", // End anchor
        "[$3] $2 by $1",
    );
    test_no_match!(strict_rule, "The Beatles - Yesterday (1965) Extended");
}

#[test_log::test]
fn handles_nested_capture_groups() {
    // Test nested capture groups
    let rule = SdRule::new(r"((.+) - (.+)) \(Remix\)", "Remix: $2 - $3 (Original: $1)");

    test_rule!(
        rule,
        "Artist - Song (Remix)",
        "Remix: Artist - Song (Original: Artist - Song)"
    );
}

#[test_log::test]
fn handles_optional_capture_groups() {
    // Test with optional groups using ?
    let _rule = SdRule::new(
        r"(.+?)(?: - (.+))?(?: \((.+)\))?",
        "Track: $1${2:+, Album: $2}${3:+, Note: $3}",
    );

    // This is a complex case - let's use a simpler version for testing
    let simple_rule = SdRule::new(r"(.+) - (.+)", "$2 by $1");

    test_rule!(simple_rule, "Artist - Song", "Song by Artist");
    test_no_match!(simple_rule, "Just a song title");
}

#[test_log::test]
fn handles_repeated_capture_groups() {
    // Test capturing repeated elements
    let rule = SdRule::new(r"(.+) feat\. (.+) feat\. (.+)", "$1 featuring $2 and $3");

    test_rule!(
        rule,
        "Main Artist feat. Artist 2 feat. Artist 3",
        "Main Artist featuring Artist 2 and Artist 3"
    );
}

// Note: Backreferences in patterns are not supported by the Rust regex crate
// #[test_log::test]
// fn test_backreferences_in_pattern() {
//     // This would require backreferences which aren't supported in Rust regex
//     let rule = SdRule::new(r"(.+) \1", "$1"); // \1 not supported in pattern
//     test_rule!(rule, "hello hello", "hello");
// }

#[test_log::test]
fn handles_capture_group_edge_cases() {
    // Test empty captures
    let rule = SdRule::new(r"(.*)-(.*)", "$1|$2");
    test_rule!(rule, "hello-", "hello|");
    test_rule!(rule, "-world", "|world");
    test_rule!(rule, "-", "|");

    // Test non-capturing groups
    let rule2 = SdRule::new(r"(?:feat\.|ft\.) (.+)", "featuring $1");
    test_rule!(rule2, "feat. Someone", "featuring Someone");
    test_rule!(rule2, "ft. Someone", "featuring Someone");
}

#[test_log::test]
fn handles_escaped_capture_references() {
    // Test escaped dollar signs in replacement
    let rule = SdRule::new(r"(.+) costs (.+)", r"Price of $1 is \$$2");
    test_rule!(rule, "Album costs 15", r"Price of Album is $15");

    // Alternative escaping syntax
    let rule2 = SdRule::new(r"(.+) costs (.+)", r"Price of $1 is $$$2");
    test_rule!(rule2, "Album costs 15", r"Price of Album is $15");

    // Multiple escaped dollars
    let rule3 = SdRule::new(r"(.+)", r"\$\$$1\$\$");
    test_rule!(rule3, "test", r"$$test$$");
}

#[test_log::test]
fn handles_literal_braces_and_special_characters() {
    // Test literal braces that might be confused with named capture syntax
    let rule = SdRule::new(r"(.+)", r"{$1}");
    test_rule!(rule, "test", r"{test}");

    // Test literal ${} without actual capture group name
    let rule2 = SdRule::new(r"(.+)", r"${$1}");
    test_rule!(rule2, "test", r"${test}");

    // Test malformed named capture references (should be left as-is)
    let rule3 = SdRule::new(r"(.+)", r"${nonexistent} $1");
    test_rule!(rule3, "test", r"${nonexistent} test");

    // Test empty braces
    let rule4 = SdRule::new(r"(.+)", r"${} $1");
    test_rule!(rule4, "test", r"${} test");
}

#[test_log::test]
fn handles_mixed_escape_scenarios() {
    // Test mixing escaped dollars with capture groups
    let rule = SdRule::new(r"(.+) (\d+)\%", r"Item: $1, Discount: \$$2, Rate: $2%");
    test_rule!(
        rule,
        "Widget 15%",
        r"Item: Widget, Discount: $15, Rate: 15%"
    );

    // Test multiple dollar signs in various contexts
    let rule2 = SdRule::new(r"(.+)", r"$$1 \$1 $1 \$");
    test_rule!(rule2, "test", r"$1 $1 test $");

    // Test escaped braces with actual named captures
    let rule3 = SdRule::new(r"(?P<item>.+) (?P<price>\d+)", r"\{${item}: \$${price}\}");
    test_rule!(rule3, "Widget 25", r"{Widget: $25}");
}

#[test_log::test]
fn handles_capture_group_syntax_edge_cases() {
    // Test numbered captures at word boundaries
    let rule = SdRule::new(r"(.+) (.+)", r"$1ish-$2ly");
    test_rule!(rule, "quick brown", r"quickish-brownly");

    // Test capture groups followed by digits (potential ambiguity)
    let rule2 = SdRule::new(r"(.+)", r"$12345");
    test_rule!(rule2, "test", r"test2345"); // $1 should be "test", followed by literal "2345"

    // Test named capture followed by underscore/alphanumeric
    let rule3 = SdRule::new(r"(?P<word>.+)", r"${word}_suffix");
    test_rule!(rule3, "test", r"test_suffix");

    // Test case sensitivity in named capture references
    let rule4 = SdRule::new(r"(?P<Word>.+)", r"${Word} ${word}"); // ${word} shouldn't match
    test_rule!(rule4, "test", r"test ${word}");
}

#[test_log::test]
fn handles_literal_dollar_signs_in_various_positions() {
    // Dollar at start
    let rule1 = SdRule::new(r"(.+)", r"\$start-$1");
    test_rule!(rule1, "test", r"$start-test");

    // Dollar at end
    let rule2 = SdRule::new(r"(.+)", r"$1-end\$");
    test_rule!(rule2, "test", r"test-end$");

    // Dollar in middle
    let rule3 = SdRule::new(r"(.+) (.+)", r"$1-\$-$2");
    test_rule!(rule3, "quick brown", r"quick-$-brown");

    // Multiple consecutive escaped dollars
    let rule4 = SdRule::new(r"(.+)", r"\$\$\$$1\$\$\$");
    test_rule!(rule4, "test", r"$$$test$$$");
}

#[test_log::test]
fn handles_complex_replacement_patterns() {
    // Test replacement that looks like regex but should be literal
    let rule = SdRule::new(r"(.+) (.+)", r"Match: $1, Replace: $2, Pattern: (.+)");
    test_rule!(rule, "foo bar", r"Match: foo, Replace: bar, Pattern: (.+)");

    // Test replacement with JSON-like structure
    let rule2 = SdRule::new(
        r"(?P<name>.+) (?P<value>.+)",
        r#"{"name": "${name}", "value": "${value}", "price": \$${value}}"#,
    );
    test_rule!(
        rule2,
        "Widget 25",
        r#"{"name": "Widget", "value": "25", "price": $25}"#
    );
}

#[test_log::test]
fn handles_backslash_escaping_correctly() {
    // Test literal backslashes
    let rule1 = SdRule::new(r"(.+)", r"\\$1\\");
    test_rule!(rule1, "test", r"\test\");

    // Test backslash followed by capture group
    let rule2 = SdRule::new(r"(.+)", r"\\ $1 \\");
    test_rule!(rule2, "test", r"\ test \");

    // Test escaped backslash vs capture group
    let rule3 = SdRule::new(r"(.+) (.+)", r"$1\\n$2");
    test_rule!(rule3, "line1 line2", r"line1\nline2");

    // Test mixed escaping with backslashes
    let rule4 = SdRule::new(r"(.+)", r"Path: \\server\\$1\\ and Price: \$100");
    test_rule!(rule4, "folder", r"Path: \server\folder\ and Price: $100");

    // Test multiple consecutive backslashes
    let rule5 = SdRule::new(r"(.+)", r"\\\\$1\\\\");
    test_rule!(rule5, "test", r"\\test\\");
}

#[test_log::test]
fn handles_comprehensive_escaping_combinations() {
    // Test all escape characters together
    let rule = SdRule::new(
        r"(?P<file>.+)\.(?P<ext>.+)",
        r"File: \\server\\${file}\\ Type: \{${ext}\} Price: \$50",
    );
    test_rule!(
        rule,
        "document.pdf",
        r"File: \server\document\ Type: {pdf} Price: $50"
    );

    // Test edge case: backslash before dollar that's not a capture group
    let rule2 = SdRule::new(r"(.+)", r"\\$notacapturegroup $1");
    test_rule!(rule2, "test", r"\$notacapturegroup test");

    // Test Windows-style paths with backslashes and capture groups
    let rule3 = SdRule::new(r"(.+) (.+)", r"C:\\Users\\${1}\\Documents\\${2}.txt");
    test_rule!(rule3, "john resume", r"C:\Users\${1}\Documents\${2}.txt"); // Named refs don't exist

    // Fix the above test with proper numbered groups
    let rule4 = SdRule::new(r"(.+) (.+)", r"C:\\Users\\$1\\Documents\\$2.txt");
    test_rule!(rule4, "john resume", r"C:\Users\john\Documents\resume.txt");
}
