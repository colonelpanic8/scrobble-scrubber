use lastfm_edit::{ScrobbleEdit, Track};
use serde::{Deserialize, Serialize};

/// Create a no-op `ScrobbleEdit` from a Track (no changes, just a baseline)
#[must_use]
pub fn create_no_op_edit(track: &Track) -> ScrobbleEdit {
    ScrobbleEdit {
        track_name_original: Some(track.name.clone()),
        album_name_original: track.album.clone(),
        artist_name_original: track.artist.clone(),
        album_artist_name_original: track.album_artist.clone(),
        track_name: Some(track.name.clone()),
        album_name: track.album.clone(),
        artist_name: track.artist.clone(),
        album_artist_name: track
            .album_artist
            .clone()
            .or_else(|| Some(track.artist.clone())),
        timestamp: track.timestamp,
        edit_all: true,
    }
}

/// Check if any of the rewrite rules would apply to the given track
/// This checks if any rule patterns match the track
pub fn any_rules_apply(rules: &[RewriteRule], track: &Track) -> Result<bool, RewriteError> {
    for rule in rules {
        if rule.matches(track)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check if any of the rewrite rules' patterns match the given track
pub fn any_rules_match(rules: &[RewriteRule], track: &Track) -> Result<bool, RewriteError> {
    for rule in rules {
        if rule.matches(track)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Apply all rewrite rules to a `ScrobbleEdit`, returning true if any changes were made
/// Rules are filtered to only apply those that match the ScrobbleEdit using semantic None handling
pub fn apply_all_rules(
    rules: &[RewriteRule],
    edit: &mut ScrobbleEdit,
) -> Result<bool, RewriteError> {
    let mut any_changes = false;
    for rule in rules {
        // Filter: only apply rules that match the ScrobbleEdit with semantic None handling
        if rule.matches_scrobble_edit(edit)? {
            let changed = rule.apply(edit)?;
            if changed {
                any_changes = true;
                let rule_name = rule.name.as_deref().unwrap_or("unnamed rule");
                log::debug!(
                    "Applied rewrite rule '{}' to track '{}' by '{}'",
                    rule_name,
                    edit.track_name_original.as_deref().unwrap_or("unknown"),
                    &edit.artist_name_original
                );
            }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Whether this rule requires MusicBrainz confirmation of the rewritten metadata
    /// If true, the rewritten values must be validated against MusicBrainz before the edit is suggested/applied
    #[serde(default)]
    pub requires_musicbrainz_confirmation: bool,
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
            requires_musicbrainz_confirmation: false,
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

    /// Set whether this rule requires MusicBrainz confirmation
    #[must_use]
    pub const fn with_musicbrainz_confirmation_required(
        mut self,
        requires_musicbrainz_confirmation: bool,
    ) -> Self {
        self.requires_musicbrainz_confirmation = requires_musicbrainz_confirmation;
        self
    }

    /// Check if this rule's patterns match the given track (regardless of whether it would modify it)
    ///
    /// A rule matches when:
    /// - All None fields are considered as always matching
    /// - All Some fields must match their respective track fields (pattern matching only)
    /// - A rule with all None fields is treated as always matching (acts as a catch-all)
    /// - If any Some field's pattern doesn't match, the rule doesn't match
    pub fn matches(&self, track: &Track) -> Result<bool, RewriteError> {
        let rule_name = self.name.as_deref().unwrap_or("unnamed");
        let mut matched_fields = Vec::new();
        let mut failed_fields = Vec::new();
        let mut checked_fields = Vec::new();

        // Check track name pattern if present
        if let Some(rule) = &self.track_name {
            checked_fields.push("track_name");
            if rule.matches(&track.name)? {
                let track_name = &track.name;
                matched_fields.push(format!("track_name('{track_name}')"));
            } else {
                let track_name = &track.name;
                let pattern = &rule.find;
                failed_fields.push(format!("track_name('{track_name}' ≠ pattern '{pattern}')"));
            }
        }

        // Check artist name pattern if present
        if let Some(rule) = &self.artist_name {
            checked_fields.push("artist_name");
            if rule.matches(&track.artist)? {
                let artist_name = &track.artist;
                matched_fields.push(format!("artist_name('{artist_name}')"));
            } else {
                let artist_name = &track.artist;
                let pattern = &rule.find;
                failed_fields.push(format!(
                    "artist_name('{artist_name}' ≠ pattern '{pattern}')"
                ));
            }
        }

        // Check album name pattern if present
        if let Some(rule) = &self.album_name {
            checked_fields.push("album_name");
            let album_name = track.album.as_deref().unwrap_or("");
            if rule.matches(album_name)? {
                matched_fields.push(format!("album_name('{album_name}')"));
            } else {
                failed_fields.push(format!(
                    "album_name('{album_name}' ≠ pattern '{}')",
                    rule.find
                ));
            }
        }

        // Check album artist name pattern if present (always empty for Track)
        if let Some(rule) = &self.album_artist_name {
            checked_fields.push("album_artist_name");
            if rule.matches("")? {
                matched_fields.push("album_artist_name('')".to_string());
            } else {
                let pattern = &rule.find;
                failed_fields.push(format!("album_artist_name('' ≠ pattern '{pattern}')"));
            }
        }

        let rule_matches = failed_fields.is_empty();

        // Log comprehensive summary
        if checked_fields.is_empty() {
            let track_name = &track.name;
            let track_artist = &track.artist;
            log::debug!(
                "Rule '{rule_name}' matches track '{track_name}' by '{track_artist}' (catch-all rule with no patterns)"
            );
        } else if rule_matches {
            let track_name = &track.name;
            let track_artist = &track.artist;
            let matched = matched_fields.join(", ");
            log::debug!(
                "Rule '{rule_name}' matches track '{track_name}' by '{track_artist}' | Matched: [{matched}]"
            );
        } else {
            let track_name = &track.name;
            let track_artist = &track.artist;
            let matched = if matched_fields.is_empty() {
                "none".to_string()
            } else {
                matched_fields.join(", ")
            };
            let failed = failed_fields.join(", ");
            log::debug!(
                "Rule '{rule_name}' does not match track '{track_name}' by '{track_artist}' | Matched: [{matched}] | Failed: [{failed}]"
            );
        }

        Ok(rule_matches)
    }

    /// Helper function to check if a single field matches between rule and ScrobbleEdit
    /// with semantic None handling
    fn check_field_match(
        field_name: &str,
        rule: &SdRule,
        edit_field: Option<&str>,
        matched_fields: &mut Vec<String>,
        failed_fields: &mut Vec<String>,
    ) -> Result<(), RewriteError> {
        if let Some(field_value) = edit_field {
            if rule.matches(field_value)? {
                matched_fields.push(format!("{field_name}('{field_value}')"));
            } else {
                let pattern = &rule.find;
                failed_fields.push(format!(
                    "{field_name}('{field_value}' ≠ pattern '{pattern}')"
                ));
            }
        } else {
            // ScrobbleEdit field is None - check for special .* pattern that matches anything including None
            let pattern = &rule.find;
            if pattern == ".*" {
                // Special case: .* pattern matches None (conceptually "match anything, including nothing")
                matched_fields.push(format!("{field_name}(None - matched by .* pattern)"));
            } else {
                // Rule field is Some but ScrobbleEdit field is None - NO MATCH for other patterns
                failed_fields.push(format!("{field_name}(None ≠ pattern '{pattern}' - cannot match pattern against None value)"));
            }
        }
        Ok(())
    }

    /// Check if this rule's patterns match the given ScrobbleEdit with semantic None handling
    ///
    /// A rule matches when:
    /// - All None rule fields are considered as always matching (no constraint)
    /// - For Some rule fields, they must match their respective ScrobbleEdit fields
    /// - **Semantic None Handling**:
    ///   - If rule field is None and ScrobbleEdit field is None: **MATCH** (no constraint, no value)
    ///   - If rule field is None and ScrobbleEdit field is Some: **MATCH** (no constraint, any value OK)
    ///   - If rule field is Some and ScrobbleEdit field is None: **NO MATCH** (constraint exists but no value to match)
    ///   - If rule field is Some and ScrobbleEdit field is Some: **Check pattern match**
    /// - If any Some field's pattern doesn't match, the rule doesn't match
    pub fn matches_scrobble_edit(&self, edit: &ScrobbleEdit) -> Result<bool, RewriteError> {
        let rule_name = self.name.as_deref().unwrap_or("unnamed");
        let mut matched_fields = Vec::new();
        let mut failed_fields = Vec::new();
        let mut checked_fields = Vec::new();

        // Check track name pattern if present
        if let Some(rule) = &self.track_name {
            checked_fields.push("track_name");
            Self::check_field_match(
                "track_name",
                rule,
                edit.track_name.as_deref(),
                &mut matched_fields,
                &mut failed_fields,
            )?;
        }

        // Check artist name pattern if present
        if let Some(rule) = &self.artist_name {
            checked_fields.push("artist_name");
            // artist_name is always Some in ScrobbleEdit (it's not Option<String>)
            Self::check_field_match(
                "artist_name",
                rule,
                Some(&edit.artist_name),
                &mut matched_fields,
                &mut failed_fields,
            )?;
        }

        // Check album name pattern if present
        if let Some(rule) = &self.album_name {
            checked_fields.push("album_name");
            Self::check_field_match(
                "album_name",
                rule,
                edit.album_name.as_deref(),
                &mut matched_fields,
                &mut failed_fields,
            )?;
        }

        // Check album artist name pattern if present
        if let Some(rule) = &self.album_artist_name {
            checked_fields.push("album_artist_name");
            Self::check_field_match(
                "album_artist_name",
                rule,
                edit.album_artist_name.as_deref(),
                &mut matched_fields,
                &mut failed_fields,
            )?;
        }

        let rule_matches = failed_fields.is_empty();

        // Log comprehensive summary
        if checked_fields.is_empty() {
            log::debug!(
                "Rule '{rule_name}' matches ScrobbleEdit (catch-all rule with no patterns)"
            );
        } else if rule_matches {
            let matched = matched_fields.join(", ");
            log::debug!("Rule '{rule_name}' matches ScrobbleEdit | Matched: [{matched}]");
        } else {
            let matched = if matched_fields.is_empty() {
                "none".to_string()
            } else {
                matched_fields.join(", ")
            };
            let failed = failed_fields.join(", ");
            log::debug!("Rule '{rule_name}' does not match ScrobbleEdit | Matched: [{matched}] | Failed: [{failed}]");
        }

        Ok(rule_matches)
    }

    /// Apply this rule to an existing `ScrobbleEdit`, modifying it in place
    /// Returns true if any changes were made
    ///
    /// IMPORTANT: This method assumes the rule has already been checked to match the ScrobbleEdit.
    /// Rules should be filtered using matches_scrobble_edit() before calling apply().
    pub fn apply(&self, edit: &mut ScrobbleEdit) -> Result<bool, RewriteError> {
        let mut has_changes = false;

        // Apply track name transformation if present
        if let Some(rule) = &self.track_name {
            if let Some(current_value) = &edit.track_name {
                let new_value = rule.apply(current_value)?;
                if new_value != *current_value {
                    edit.track_name = Some(new_value);
                    has_changes = true;
                }
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
            if let Some(current_value) = &edit.album_name {
                let new_value = rule.apply(current_value)?;
                if new_value != *current_value {
                    edit.album_name = Some(new_value);
                    has_changes = true;
                }
            }
        }

        // Apply album artist name transformation if present
        if let Some(rule) = &self.album_artist_name {
            if let Some(current_value) = &edit.album_artist_name {
                let new_value = rule.apply(current_value)?;
                if new_value != *current_value {
                    edit.album_artist_name = Some(new_value);
                    has_changes = true;
                }
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

/// Load comprehensive default rewrite rules from the embedded JSON files
///
/// This loads the full set of remaster and special edition cleanup rules from the JSON files.
/// If the files cannot be loaded or parsed, falls back to the basic default_rules().
#[must_use]
pub fn load_comprehensive_default_rules() -> Vec<RewriteRule> {
    use crate::default_rules::load_all_default_rules;

    match load_all_default_rules() {
        Ok(rules) => {
            let rewrite_rules: Vec<RewriteRule> =
                rules.into_iter().map(|rule| rule.into()).collect();
            log::info!(
                "Loaded {} comprehensive default rewrite rules (including special editions)",
                rewrite_rules.len()
            );
            rewrite_rules
        }
        Err(e) => {
            log::warn!(
                "Failed to load comprehensive default rules: {e}, falling back to basic rules"
            );
            default_rules()
        }
    }
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
