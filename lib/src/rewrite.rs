use lastfm_edit::{ScrobbleEdit, Track};
use serde::{Deserialize, Serialize};

/// Create a no-op `ScrobbleEdit` from a Track (no changes, just a baseline)
#[must_use]
pub fn create_no_op_edit(track: &Track) -> ScrobbleEdit {
    let album_name = track.album.clone().unwrap_or_default();
    ScrobbleEdit {
        track_name_original: track.name.clone(),
        album_name_original: album_name.clone(),
        artist_name_original: track.artist.clone(),
        album_artist_name_original: String::new(),
        track_name: track.name.clone(),
        album_name,
        artist_name: track.artist.clone(),
        album_artist_name: String::new(),
        timestamp: track.timestamp.unwrap_or(0),
        edit_all: false,
    }
}

/// Check if any of the rewrite rules would apply to the given track
pub fn any_rules_apply(rules: &[RewriteRule], track: &Track) -> Result<bool, RewriteError> {
    use log::trace;

    trace!(
        "Checking {rules_len} rules against track '{track_name}' by '{track_artist}'",
        rules_len = rules.len(),
        track_name = track.name,
        track_artist = track.artist
    );

    for (i, rule) in rules.iter().enumerate() {
        let rule_name = rule.name.as_deref().unwrap_or("unnamed");
        trace!("Checking rule {i} '{rule_name}' against track");

        if rule.applies_to(track)? {
            trace!(
                "Rule {i} '{rule_name}' applies to track '{track_name}' by '{track_artist}'",
                track_name = track.name,
                track_artist = track.artist
            );
            return Ok(true);
        }
    }

    trace!(
        "No rules apply to track '{track_name}' by '{track_artist}'",
        track_name = track.name,
        track_artist = track.artist
    );
    Ok(false)
}

/// Check if any of the rewrite rules' patterns match the given track (regardless of whether they would modify it)
pub fn any_rules_match(rules: &[RewriteRule], track: &Track) -> Result<bool, RewriteError> {
    for rule in rules {
        if rule.matches(track)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Apply all rewrite rules to a `ScrobbleEdit`, returning true if any changes were made
pub fn apply_all_rules(
    rules: &[RewriteRule],
    edit: &mut ScrobbleEdit,
) -> Result<bool, RewriteError> {
    let mut any_changes = false;
    for rule in rules {
        let changed = rule.apply(edit)?;
        if changed {
            any_changes = true;
            let rule_name = rule.name.as_deref().unwrap_or("unnamed rule");
            log::info!(
                "Applied rewrite rule '{}' to track '{}' by '{}'",
                rule_name,
                edit.track_name_original,
                edit.artist_name_original
            );
        }
    }
    Ok(any_changes)
}

/// A single find-and-replace transformation with whole-string replacement behavior
///
/// ## Behavior
///
/// When a pattern matches anywhere in the input string, the **entire input string**
/// is replaced with the replacement text (not just the matched portion).
///
/// ## Pattern Matching
///
/// - Uses regular expressions for pattern matching
/// - Pattern can match anywhere in the input string
/// - If pattern matches, entire string is replaced
///
/// ## Replacement Syntax
///
/// The replacement string supports the following substitutions:
///
/// ### Numbered Capture Groups
/// - `$0` - The entire match
/// - `$1` - First capture group
/// - `$2` - Second capture group
/// - `$n` - nth capture group
///
/// ### Named Capture Groups
/// - `${name}` - Named capture group
/// - Example: `(?P<artist>.+)` can be referenced as `${artist}`
///
/// ### Literal Characters and Escaping
/// - `\$` - Literal dollar sign (escaped)
/// - `$$` - Literal dollar sign (alternative syntax)
/// - `\{` - Literal left brace (escaped)
/// - `\}` - Literal right brace (escaped)
/// - `\\` - Literal backslash (escaped)
///
/// ## Examples
///
/// ```rust
/// use scrobble_scrubber::rewrite::SdRule;
///
/// // Basic replacement: "Vulfpeck ft. anyone" -> "Vulfpeck"
/// let rule = SdRule::new("Vulfpeck", "Vulfpeck");
/// assert_eq!(rule.apply("Vulfpeck ft. Antwaun Stanley").unwrap(), "Vulfpeck");
///
/// // Capture groups: extract artist from "Artist - Song"
/// let rule = SdRule::new(r"(.+) - .+", "$1");
/// assert_eq!(rule.apply("The Beatles - Yesterday").unwrap(), "The Beatles");
///
/// // Named capture groups: reformat track info
/// let rule = SdRule::new(
///     r"(?P<artist>.+) - (?P<song>.+)",
///     "${song} by ${artist}"
/// );
/// assert_eq!(rule.apply("Queen - Bohemian Rhapsody").unwrap(), "Bohemian Rhapsody by Queen");
/// ```
///
/// ## Regex Flags
///
/// - `i` - Case insensitive matching
/// - `m` - Multiline mode (default)
/// - `s` - Dot matches newline
/// - `c` - Case sensitive (explicit)
/// - `e` - Single line mode (disables multiline)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdRule {
    /// The pattern to search for (always treated as regex)
    pub find: String,
    /// The replacement string (supports capture group substitution)
    pub replace: String,
    /// Regex flags (e.g., "i" for case insensitive)
    pub flags: Option<String>,
}

impl SdRule {
    /// Create a new rule (always regex-based)
    #[must_use]
    pub fn new(find: &str, replace: &str) -> Self {
        Self {
            find: find.to_string(),
            replace: replace.to_string(),
            flags: None,
        }
    }

    /// Add regex flags
    #[must_use]
    pub fn with_flags(mut self, flags: &str) -> Self {
        self.flags = Some(flags.to_string());
        self
    }

    /// Apply this rule to a string, returning the result
    /// If the pattern matches anywhere in the input, the entire string is replaced
    pub fn apply(&self, input: &str) -> Result<String, RewriteError> {
        // Always use regex mode - if pattern matches anywhere, replace entire string
        // But allow capture group substitution from the original input
        let mut regex_builder = regex::RegexBuilder::new(&self.find);
        regex_builder.multi_line(true);

        // Apply flags if present
        if let Some(flags) = &self.flags {
            for c in flags.chars() {
                match c {
                    'c' => {
                        regex_builder.case_insensitive(false);
                    }
                    'i' => {
                        regex_builder.case_insensitive(true);
                    }
                    'm' => {}
                    'e' => {
                        regex_builder.multi_line(false);
                    }
                    's' => {
                        if !flags.contains('m') {
                            regex_builder.multi_line(false);
                        }
                        regex_builder.dot_matches_new_line(true);
                    }
                    _ => {}
                }
            }
        }

        let regex = regex_builder.build().map_err(RewriteError::RegexError)?;

        if let Some(captures) = regex.captures(input) {
            // Pattern matches - replace entire string, expanding capture groups
            let mut result = self.replace.clone();

            // Handle escaped characters first (convert to placeholders)
            let escaped_dollar_placeholder = "\u{E000}ESCAPED_DOLLAR\u{E000}"; // Use private use area
            let escaped_lbrace_placeholder = "\u{E000}ESCAPED_LBRACE\u{E000}";
            let escaped_rbrace_placeholder = "\u{E000}ESCAPED_RBRACE\u{E000}";
            let escaped_backslash_placeholder = "\u{E000}ESCAPED_BACKSLASH\u{E000}";

            // Handle backslashes first to prevent double-processing
            result = result.replace(r"\\", escaped_backslash_placeholder);
            result = result.replace(r"\$", escaped_dollar_placeholder);
            result = result.replace("$$", escaped_dollar_placeholder);
            result = result.replace(r"\{", escaped_lbrace_placeholder);
            result = result.replace(r"\}", escaped_rbrace_placeholder);

            // Replace numbered capture group references ($0, $1, $2, etc.)
            for i in 0..captures.len() {
                let placeholder = format!("${i}");
                if let Some(capture) = captures.get(i) {
                    result = result.replace(&placeholder, capture.as_str());
                }
            }

            // Replace named capture group references (${name})
            for name in regex.capture_names().flatten() {
                let placeholder = format!("${{{name}}}");
                if let Some(capture) = captures.name(name) {
                    result = result.replace(&placeholder, capture.as_str());
                }
            }

            // Restore escaped characters
            result = result.replace(escaped_dollar_placeholder, "$");
            result = result.replace(escaped_lbrace_placeholder, "{");
            result = result.replace(escaped_rbrace_placeholder, "}");
            result = result.replace(escaped_backslash_placeholder, "\\");

            Ok(result)
        } else {
            // Pattern doesn't match - return input unchanged
            Ok(input.to_string())
        }
    }

    /// Check if this rule would modify the input string
    pub fn would_modify(&self, input: &str) -> Result<bool, RewriteError> {
        let result = self.apply(input)?;
        Ok(result != input)
    }

    /// Check if this rule's pattern matches the input string (regardless of whether it would modify it)
    pub fn matches(&self, input: &str) -> Result<bool, RewriteError> {
        let mut regex_builder = regex::RegexBuilder::new(&self.find);
        regex_builder.multi_line(true);

        // Apply flags if present
        if let Some(flags) = &self.flags {
            for c in flags.chars() {
                match c {
                    'c' => {
                        regex_builder.case_insensitive(false);
                    }
                    'i' => {
                        regex_builder.case_insensitive(true);
                    }
                    'm' => {}
                    'e' => {
                        regex_builder.multi_line(false);
                    }
                    's' => {
                        if !flags.contains('m') {
                            regex_builder.multi_line(false);
                        }
                        regex_builder.dot_matches_new_line(true);
                    }
                    _ => {}
                }
            }
        }

        let regex = regex_builder.build().map_err(RewriteError::RegexError)?;
        Ok(regex.is_match(input))
    }
}

/// A comprehensive rewrite rule that can transform fields of a scrobble
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteRule {
    /// Optional name for this rule
    pub name: Option<String>,
    /// Optional transformation for track name
    pub track_name: Option<SdRule>,
    /// Optional transformation for album name
    pub album_name: Option<SdRule>,
    /// Optional transformation for artist name
    pub artist_name: Option<SdRule>,
    /// Optional transformation for album artist name
    pub album_artist_name: Option<SdRule>,
    /// Whether this rule requires user confirmation before applying
    pub requires_confirmation: bool,
}

impl RewriteRule {
    /// Create a new empty rewrite rule
    #[must_use]
    pub const fn new() -> Self {
        Self {
            name: None,
            track_name: None,
            album_name: None,
            artist_name: None,
            album_artist_name: None,
            requires_confirmation: false,
        }
    }

    /// Set name for this rule
    #[must_use]
    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set transformation for track name
    #[must_use]
    pub fn with_track_name(mut self, rule: SdRule) -> Self {
        self.track_name = Some(rule);
        self
    }

    /// Set transformation for album name
    #[must_use]
    pub fn with_album_name(mut self, rule: SdRule) -> Self {
        self.album_name = Some(rule);
        self
    }

    /// Set transformation for artist name
    #[must_use]
    pub fn with_artist_name(mut self, rule: SdRule) -> Self {
        self.artist_name = Some(rule);
        self
    }

    /// Set transformation for album artist name
    #[must_use]
    pub fn with_album_artist_name(mut self, rule: SdRule) -> Self {
        self.album_artist_name = Some(rule);
        self
    }

    /// Set whether this rule requires confirmation
    #[must_use]
    pub const fn with_confirmation_required(mut self, requires_confirmation: bool) -> Self {
        self.requires_confirmation = requires_confirmation;
        self
    }

    /// Check if this rule would apply to the given track (without creating an edit)
    ///
    /// A rule applies when:
    /// - All None fields are considered as always matching
    /// - All Some fields must match their respective track fields
    /// - A rule with all None fields is treated as always matching (acts as a catch-all)
    /// - If any Some field doesn't match, the rule doesn't apply
    pub fn applies_to(&self, track: &Track) -> Result<bool, RewriteError> {
        use log::trace;

        let rule_name = self.name.as_deref().unwrap_or("unnamed");
        trace!("Evaluating rule '{rule_name}' against track: '{track_name}' by '{track_artist}' (album: '{album_name}')",
               track_name = track.name, track_artist = track.artist, album_name = track.album.as_deref().unwrap_or(""));

        // Check track name transformation if present
        if let Some(rule) = &self.track_name {
            let would_modify = rule.would_modify(&track.name)?;
            trace!("Rule '{rule_name}' track_name check: pattern='{find}' -> '{replace}', track_name='{track_name}', would_modify={would_modify}",
                   find = rule.find, replace = rule.replace, track_name = track.name);
            if !would_modify {
                trace!("Rule '{rule_name}' does not apply: track name '{track_name}' would not be modified by pattern '{find}'",
                       track_name = track.name, find = rule.find);
                return Ok(false);
            }
        }

        // Check artist name transformation if present
        if let Some(rule) = &self.artist_name {
            let would_modify = rule.would_modify(&track.artist)?;
            trace!("Rule '{rule_name}' artist_name check: pattern='{find}' -> '{replace}', artist_name='{artist_name}', would_modify={would_modify}",
                   find = rule.find, replace = rule.replace, artist_name = track.artist);
            if !would_modify {
                trace!("Rule '{rule_name}' does not apply: artist name '{artist_name}' would not be modified by pattern '{find}'",
                       artist_name = track.artist, find = rule.find);
                return Ok(false);
            }
        }

        // Check album name transformation if present
        if let Some(rule) = &self.album_name {
            let album_name = track.album.as_deref().unwrap_or("");
            let would_modify = rule.would_modify(album_name)?;
            trace!("Rule '{rule_name}' album_name check: pattern='{find}' -> '{replace}', album_name='{album_name}', would_modify={would_modify}",
                   find = rule.find, replace = rule.replace);
            if !would_modify {
                trace!("Rule '{rule_name}' does not apply: album name '{album_name}' would not be modified by pattern '{find}'",
                       find = rule.find);
                return Ok(false);
            }
        }

        // Check album artist name transformation if present (always empty for Track)
        if let Some(rule) = &self.album_artist_name {
            let would_modify = rule.would_modify("")?;
            trace!("Rule '{rule_name}' album_artist_name check: pattern='{find}' -> '{replace}', album_artist_name='', would_modify={would_modify}",
                   find = rule.find, replace = rule.replace);
            if !would_modify {
                trace!("Rule '{rule_name}' does not apply: album artist name '' would not be modified by pattern '{find}'",
                       find = rule.find);
                return Ok(false);
            }
        }

        // Rule applies if all present rules would modify their fields
        // Rules with all None fields are treated as always matching (catch-all)
        trace!(
            "Rule '{rule_name}' applies to track '{track_name}' by '{track_artist}'",
            track_name = track.name,
            track_artist = track.artist
        );
        Ok(true)
    }

    /// Check if this rule's patterns match the given track (regardless of whether it would modify it)
    ///
    /// A rule matches when:
    /// - All None fields are considered as always matching
    /// - All Some fields must match their respective track fields (pattern matching only)
    /// - A rule with all None fields is treated as always matching (acts as a catch-all)
    /// - If any Some field's pattern doesn't match, the rule doesn't match
    pub fn matches(&self, track: &Track) -> Result<bool, RewriteError> {
        // Check track name pattern if present
        if let Some(rule) = &self.track_name {
            if !rule.matches(&track.name)? {
                return Ok(false);
            }
        }

        // Check artist name pattern if present
        if let Some(rule) = &self.artist_name {
            if !rule.matches(&track.artist)? {
                return Ok(false);
            }
        }

        // Check album name pattern if present
        if let Some(rule) = &self.album_name {
            let album_name = track.album.as_deref().unwrap_or("");
            if !rule.matches(album_name)? {
                return Ok(false);
            }
        }

        // Check album artist name pattern if present (always empty for Track)
        if let Some(rule) = &self.album_artist_name {
            if !rule.matches("")? {
                return Ok(false);
            }
        }

        // Rule matches if all present rule patterns match their fields
        // Rules with all None fields are treated as always matching (catch-all)
        Ok(true)
    }

    /// Apply this rule to an existing `ScrobbleEdit`, modifying it in place
    /// Returns true if any changes were made
    pub fn apply(&self, edit: &mut ScrobbleEdit) -> Result<bool, RewriteError> {
        let mut has_changes = false;

        // Apply track name transformation if present
        if let Some(rule) = &self.track_name {
            let current_value = &edit.track_name;
            let new_value = rule.apply(current_value)?;
            if new_value != *current_value {
                edit.track_name = new_value;
                has_changes = true;
            }
        }

        // Apply artist name transformation if present
        if let Some(rule) = &self.artist_name {
            let current_value = &edit.artist_name;
            let new_value = rule.apply(current_value)?;
            if new_value != *current_value {
                edit.artist_name = new_value;
                has_changes = true;
            }
        }

        // Apply album name transformation if present
        if let Some(rule) = &self.album_name {
            let current_value = &edit.album_name;
            let new_value = rule.apply(current_value)?;
            if new_value != *current_value {
                edit.album_name = new_value;
                has_changes = true;
            }
        }

        // Apply album artist name transformation if present
        if let Some(rule) = &self.album_artist_name {
            let current_value = &edit.album_artist_name;
            let new_value = rule.apply(current_value)?;
            if new_value != *current_value {
                edit.album_artist_name = new_value;
                has_changes = true;
            }
        }

        Ok(has_changes)
    }
}

impl Default for RewriteRule {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during rewrite operations
#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),
    #[error("Invalid replacement capture: {0}")]
    InvalidReplaceCapture(String),
    #[error("Invalid UTF-8 in result: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

/// Default rewrite rules for common cleanup tasks
#[must_use]
pub fn default_rules() -> Vec<RewriteRule> {
    vec![
        // Remove remaster suffixes from track names
        RewriteRule::new()
            .with_track_name(SdRule::new(
                r" - \d{4} [Rr]emaster| - [Rr]emaster \d{4}| - [Rr]emaster| \(\d{4} [Rr]emaster\)| \([Rr]emaster \d{4}\)| \([Rr]emaster\)",
                ""
            )),

        // Normalize featuring formats in artist names
        RewriteRule::new()
            .with_artist_name(SdRule::new(r" [Ff]t\. | [Ff]eaturing ", " feat. ")),

        // Clean up extra whitespace in track names
        RewriteRule::new()
            .with_track_name(SdRule::new(r"\s+", " "))
            .with_artist_name(SdRule::new(r"\s+", " ")),

        // Remove leading/trailing whitespace
        RewriteRule::new()
            .with_track_name(SdRule::new(r"^\s+|\s+$", ""))
            .with_artist_name(SdRule::new(r"^\s+|\s+$", "")),

        // Remove explicit content warnings
        RewriteRule::new()
            .with_track_name(SdRule::new(r" \(Explicit\)$| - Explicit$", "")),
    ]
}
