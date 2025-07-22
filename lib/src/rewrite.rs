use lastfm_edit::{ScrobbleEdit, Track};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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
    for rule in rules {
        if rule.applies_to(track)? {
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
        }
    }
    Ok(any_changes)
}

/// A single find-and-replace transformation using sd-style syntax
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdRule {
    /// The pattern to search for (regex by default, or literal if `is_literal` is true)
    pub find: String,
    /// The replacement string (supports $1, $2, ${named}, etc.)
    pub replace: String,
    /// Whether to use literal string matching instead of regex
    pub is_literal: bool,
    /// Regex flags (e.g., "i" for case insensitive)
    pub flags: Option<String>,
    /// Maximum number of replacements (0 = unlimited)
    pub max_replacements: usize,
}

impl SdRule {
    /// Create a new regex-based rule
    #[must_use]
    pub fn new_regex(find: &str, replace: &str) -> Self {
        Self {
            find: find.to_string(),
            replace: replace.to_string(),
            is_literal: false,
            flags: None,
            max_replacements: 0,
        }
    }

    /// Create a new literal string rule
    #[must_use]
    pub fn new_literal(find: &str, replace: &str) -> Self {
        Self {
            find: find.to_string(),
            replace: replace.to_string(),
            is_literal: true,
            flags: None,
            max_replacements: 0,
        }
    }

    /// Add regex flags
    #[must_use]
    pub fn with_flags(mut self, flags: &str) -> Self {
        self.flags = Some(flags.to_string());
        self
    }

    /// Set maximum number of replacements
    #[must_use]
    pub const fn with_max_replacements(mut self, max: usize) -> Self {
        self.max_replacements = max;
        self
    }

    /// Apply this rule to a string, returning the result
    pub fn apply(&self, input: &str) -> Result<String, RewriteError> {
        if self.is_literal {
            // Simple string replacement for literal mode
            if self.max_replacements == 0 {
                Ok(input.replace(&self.find, &self.replace))
            } else {
                Ok(input.replacen(&self.find, &self.replace, self.max_replacements))
            }
        } else {
            // Regex replacement using sd-style logic
            let replacer = SdReplacer::new(
                self.find.clone(),
                self.replace.clone(),
                self.flags.clone(),
                self.max_replacements,
            )?;

            Ok(replacer.replace(input.as_bytes()).into_owned())
                .and_then(|bytes| String::from_utf8(bytes).map_err(RewriteError::InvalidUtf8))
        }
    }

    /// Check if this rule would modify the input string
    pub fn would_modify(&self, input: &str) -> Result<bool, RewriteError> {
        let result = self.apply(input)?;
        Ok(result != input)
    }
}

/// sd-style replacer adapted from the sd crate
struct SdReplacer {
    regex: Regex,
    replace_with: Vec<u8>,
    max_replacements: usize,
}

impl SdReplacer {
    fn new(
        look_for: String,
        replace_with: String,
        flags: Option<String>,
        max_replacements: usize,
    ) -> Result<Self, RewriteError> {
        // Validate replacement string first
        validate_replace(&replace_with)?;

        let replace_with = unescape(&replace_with).into_bytes();

        let mut regex_builder = regex::RegexBuilder::new(&look_for);
        regex_builder.multi_line(true);

        if let Some(flags) = flags {
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
                    'w' => {
                        regex_builder = regex::RegexBuilder::new(&format!("\\b{look_for}\\b"));
                    }
                    _ => {}
                }
            }
        }

        let regex = regex_builder.build().map_err(RewriteError::RegexError)?;

        Ok(Self {
            regex,
            replace_with,
            max_replacements,
        })
    }

    fn replace<'a>(&self, content: &'a [u8]) -> Cow<'a, [u8]> {
        let content_str = std::str::from_utf8(content).unwrap_or("");
        let limit = if self.max_replacements == 0 {
            None
        } else {
            Some(self.max_replacements)
        };

        let replace_with = std::str::from_utf8(&self.replace_with).unwrap_or("");
        let result = match limit {
            None => self.regex.replace_all(content_str, replace_with),
            Some(n) => self.regex.replacen(content_str, n, replace_with),
        };

        Cow::Owned(result.as_bytes().to_vec())
    }
}

/// A comprehensive rewrite rule that can transform fields of a scrobble
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteRule {
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
            track_name: None,
            album_name: None,
            artist_name: None,
            album_artist_name: None,
            requires_confirmation: false,
        }
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
    pub fn applies_to(&self, track: &Track) -> Result<bool, RewriteError> {
        // Check track name transformation if present
        if let Some(rule) = &self.track_name {
            if rule.would_modify(&track.name)? {
                return Ok(true);
            }
        }

        // Check artist name transformation if present
        if let Some(rule) = &self.artist_name {
            if rule.would_modify(&track.artist)? {
                return Ok(true);
            }
        }

        // Check album name transformation if present
        if let Some(rule) = &self.album_name {
            let album_name = track.album.as_deref().unwrap_or("");
            if rule.would_modify(album_name)? {
                return Ok(true);
            }
        }

        // Check album artist name transformation if present (always empty for Track)
        if let Some(rule) = &self.album_artist_name {
            if rule.would_modify("")? {
                return Ok(true);
            }
        }

        Ok(false)
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
            .with_track_name(SdRule::new_regex(
                r" - \d{4} [Rr]emaster| - [Rr]emaster \d{4}| - [Rr]emaster| \(\d{4} [Rr]emaster\)| \([Rr]emaster \d{4}\)| \([Rr]emaster\)",
                ""
            )),

        // Normalize featuring formats in artist names
        RewriteRule::new()
            .with_artist_name(SdRule::new_regex(r" [Ff]t\. | [Ff]eaturing ", " feat. ")),

        // Clean up extra whitespace in track names
        RewriteRule::new()
            .with_track_name(SdRule::new_regex(r"\s+", " "))
            .with_artist_name(SdRule::new_regex(r"\s+", " ")),

        // Remove leading/trailing whitespace
        RewriteRule::new()
            .with_track_name(SdRule::new_regex(r"^\s+|\s+$", ""))
            .with_artist_name(SdRule::new_regex(r"^\s+|\s+$", "")),

        // Remove explicit content warnings
        RewriteRule::new()
            .with_track_name(SdRule::new_regex(r" \(Explicit\)$| - Explicit$", "")),
    ]
}

// Simplified unescape function adapted from sd
fn unescape(input: &str) -> String {
    let mut chars = input.chars();
    let mut s = String::new();

    while let Some(c) = chars.next() {
        if c != '\\' {
            s.push(c);
            continue;
        }

        if let Some(next_char) = chars.next() {
            let escaped = match next_char {
                'n' => Some('\n'),
                'r' => Some('\r'),
                't' => Some('\t'),
                '\'' => Some('\''),
                '\"' => Some('\"'),
                '\\' => Some('\\'),
                _ => None,
            };

            if let Some(escaped_char) = escaped {
                s.push(escaped_char);
            } else {
                s.push('\\');
                s.push(next_char);
            }
        } else {
            s.push('\\');
        }
    }

    s
}

// Simplified replacement validation adapted from sd
fn validate_replace(s: &str) -> Result<(), RewriteError> {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some('$') => {
                    chars.next(); // consume the second $
                }
                Some('{') => {
                    chars.next(); // consume the {
                                  // Find the closing }
                    let mut found_close = false;
                    for inner_c in chars.by_ref() {
                        if inner_c == '}' {
                            found_close = true;
                            break;
                        }
                    }
                    if !found_close {
                        return Err(RewriteError::InvalidReplaceCapture(
                            "Unclosed capture group brace".to_string(),
                        ));
                    }
                }
                Some(first) if first.is_ascii_alphanumeric() || *first == '_' => {
                    // Valid capture group start
                    chars.next();
                    while let Some(c) = chars.peek() {
                        if c.is_ascii_alphanumeric() || *c == '_' {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                _ => {
                    // Invalid capture group
                    return Err(RewriteError::InvalidReplaceCapture(
                        "Invalid capture group syntax".to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}
