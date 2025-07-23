/// Focused rewrite processing module for full string replacements
///
/// This module implements rewrite rules where:
/// 1. Regex patterns should match the entire input string (with captures)
/// 2. Replacements reconstruct the entire output string
/// 3. This ensures predictable, testable behavior
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum RewriteProcessorError {
    #[error("Regex compilation failed: {0}")]
    RegexError(#[from] regex::Error),
    #[error("Pattern must match entire input: got partial match for '{0}'")]
    PartialMatchError(String),
    #[error("Pattern must use anchors (^ and $) or match full string: '{0}'")]
    MissingAnchorsError(String),
}

/// A single transformation rule that operates on complete strings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformRule {
    /// The regex pattern - should match entire input with captures
    pub pattern: String,
    /// The replacement - reconstructs entire output using captures  
    pub replacement: String,
    /// Optional regex flags
    pub flags: Option<String>,
    /// Maximum number of applications (0 = unlimited)
    pub max_applications: usize,
}

impl TransformRule {
    /// Create a new regex-based transform rule
    pub fn new(pattern: &str, replacement: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
            flags: None,
            max_applications: 0,
        }
    }

    /// Add regex flags (i=case insensitive, m=multiline, s=dot matches newline)
    pub fn with_flags(mut self, flags: &str) -> Self {
        self.flags = Some(flags.to_string());
        self
    }

    /// Set maximum number of applications
    pub const fn with_max_applications(mut self, max: usize) -> Self {
        self.max_applications = max;
        self
    }
}

/// A processor that applies transform rules to strings
#[derive(Debug, Clone)]
pub struct RewriteProcessor {
    compiled_rules: HashMap<String, CompiledRule>,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    regex: Regex,
    replacement: String,
    max_applications: usize,
}

impl RewriteProcessor {
    /// Create a new processor with the given rules
    pub fn new(rules: Vec<TransformRule>) -> Result<Self, RewriteProcessorError> {
        let mut compiled_rules = HashMap::new();

        for rule in rules {
            // Validate that regex patterns use anchors for full string matching
            if !rule.pattern.starts_with('^') || !rule.pattern.ends_with('$') {
                return Err(RewriteProcessorError::MissingAnchorsError(
                    rule.pattern.clone(),
                ));
            }
            let (pattern, replacement) = (rule.pattern.clone(), rule.replacement.clone());

            let mut regex_builder = regex::RegexBuilder::new(&pattern);

            // Apply flags if specified
            if let Some(flags) = &rule.flags {
                for flag in flags.chars() {
                    match flag {
                        'i' => {
                            regex_builder.case_insensitive(true);
                        }
                        'm' => {
                            regex_builder.multi_line(true);
                        }
                        's' => {
                            regex_builder.dot_matches_new_line(true);
                        }
                        _ => {} // Ignore unknown flags
                    }
                }
            }

            let regex = regex_builder.build()?;

            let compiled = CompiledRule {
                regex,
                replacement,
                max_applications: rule.max_applications,
            };

            compiled_rules.insert(rule.pattern.clone(), compiled);
        }

        Ok(Self { compiled_rules })
    }

    /// Apply all rules to the input string, returning the transformed result
    pub fn process(&self, input: &str) -> Result<String, RewriteProcessorError> {
        let mut result = input.to_string();

        for compiled_rule in self.compiled_rules.values() {
            result = self.apply_rule(&result, compiled_rule)?;
        }

        Ok(result)
    }

    /// Apply a single rule to the input
    fn apply_rule(
        &self,
        input: &str,
        rule: &CompiledRule,
    ) -> Result<String, RewriteProcessorError> {
        // For full string replacement, we expect exactly one match of the entire string
        if let Some(captures) = rule.regex.captures(input) {
            if captures.get(0).unwrap().as_str() != input {
                return Err(RewriteProcessorError::PartialMatchError(input.to_string()));
            }

            let result = if rule.max_applications == 0 {
                rule.regex.replace_all(input, &rule.replacement)
            } else {
                rule.regex
                    .replacen(input, rule.max_applications, &rule.replacement)
            };
            Ok(result.to_string())
        } else {
            // No match, return input unchanged
            Ok(input.to_string())
        }
    }

    /// Check if any rules would modify the input
    pub fn would_modify(&self, input: &str) -> Result<bool, RewriteProcessorError> {
        let result = self.process(input)?;
        Ok(result != input)
    }
}

/// High-level interface for applying rewrite rules to different types of metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRewriteRule {
    /// Optional rule for track names
    pub track_name: Option<TransformRule>,
    /// Optional rule for artist names  
    pub artist_name: Option<TransformRule>,
    /// Optional rule for album names
    pub album_name: Option<TransformRule>,
    /// Optional rule for album artist names
    pub album_artist_name: Option<TransformRule>,
    /// Whether this rule requires confirmation
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone)]
pub struct MetadataRewriteProcessor {
    track_processor: Option<RewriteProcessor>,
    artist_processor: Option<RewriteProcessor>,
    album_processor: Option<RewriteProcessor>,
    album_artist_processor: Option<RewriteProcessor>,
}

impl MetadataRewriteProcessor {
    /// Create a new metadata processor from a rule
    pub fn from_rule(rule: MetadataRewriteRule) -> Result<Self, RewriteProcessorError> {
        let track_processor = if let Some(rule) = rule.track_name {
            Some(RewriteProcessor::new(vec![rule])?)
        } else {
            None
        };

        let artist_processor = if let Some(rule) = rule.artist_name {
            Some(RewriteProcessor::new(vec![rule])?)
        } else {
            None
        };

        let album_processor = if let Some(rule) = rule.album_name {
            Some(RewriteProcessor::new(vec![rule])?)
        } else {
            None
        };

        let album_artist_processor = if let Some(rule) = rule.album_artist_name {
            Some(RewriteProcessor::new(vec![rule])?)
        } else {
            None
        };

        Ok(Self {
            track_processor,
            artist_processor,
            album_processor,
            album_artist_processor,
        })
    }

    /// Apply rules to track metadata
    pub fn process_track_name(&self, track_name: &str) -> Result<String, RewriteProcessorError> {
        if let Some(processor) = &self.track_processor {
            processor.process(track_name)
        } else {
            Ok(track_name.to_string())
        }
    }

    /// Apply rules to artist metadata  
    pub fn process_artist_name(&self, artist_name: &str) -> Result<String, RewriteProcessorError> {
        if let Some(processor) = &self.artist_processor {
            processor.process(artist_name)
        } else {
            Ok(artist_name.to_string())
        }
    }

    /// Apply rules to album metadata
    pub fn process_album_name(&self, album_name: &str) -> Result<String, RewriteProcessorError> {
        if let Some(processor) = &self.album_processor {
            processor.process(album_name)
        } else {
            Ok(album_name.to_string())
        }
    }

    /// Apply rules to album artist metadata
    pub fn process_album_artist_name(
        &self,
        album_artist_name: &str,
    ) -> Result<String, RewriteProcessorError> {
        if let Some(processor) = &self.album_artist_processor {
            processor.process(album_artist_name)
        } else {
            Ok(album_artist_name.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_full_string_replacement() {
        // Pattern that captures everything and replaces "feat." with "featuring"
        let rule = TransformRule::new(r"^(.*)feat\.(.*)$", "${1}featuring${2}");
        let processor = RewriteProcessor::new(vec![rule]).unwrap();

        let result = processor.process("Song Title (feat. Artist)").unwrap();
        assert_eq!(result, "Song Title (featuring Artist)");
    }

    #[test]
    fn test_regex_requires_anchors() {
        // This should fail because it doesn't use ^ and $ anchors
        let rule = TransformRule::new("feat.", "featuring");
        let result = RewriteProcessor::new(vec![rule]);

        assert!(matches!(
            result,
            Err(RewriteProcessorError::MissingAnchorsError(_))
        ));
    }

    #[test]
    fn test_case_insensitive_replacement() {
        let rule = TransformRule::new(r"^(.*)FEAT\.(.*)$", "${1}featuring${2}").with_flags("i");
        let processor = RewriteProcessor::new(vec![rule]).unwrap();

        let result = processor.process("Song Title (feat. Artist)").unwrap();
        assert_eq!(result, "Song Title (featuring Artist)");
    }

    #[test]
    fn test_no_change_when_no_match() {
        let rule = TransformRule::new(r"^(.*)feat\.(.*)$", "$1featuring$2");
        let processor = RewriteProcessor::new(vec![rule]).unwrap();

        let result = processor.process("Song Title").unwrap();
        assert_eq!(result, "Song Title");
    }

    #[test]
    fn test_metadata_processor() {
        let rule = MetadataRewriteRule {
            track_name: Some(TransformRule::new(r"^(.*)feat\.(.*)$", "${1}featuring${2}")),
            artist_name: Some(TransformRule::new(r"^(.*)&(.*)$", "${1} and ${2}")),
            album_name: None,
            album_artist_name: None,
            requires_confirmation: false,
        };

        let processor = MetadataRewriteProcessor::from_rule(rule).unwrap();

        let track_result = processor.process_track_name("Song (feat. Artist)").unwrap();
        assert_eq!(track_result, "Song (featuring Artist)");

        let artist_result = processor.process_artist_name("Artist A&Artist B").unwrap();
        assert_eq!(artist_result, "Artist A and Artist B");

        let album_result = processor.process_album_name("Album Name").unwrap();
        assert_eq!(album_result, "Album Name"); // No change since no rule
    }
}
