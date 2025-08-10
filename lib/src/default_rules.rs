use crate::rewrite::{RewriteRule, SdRule};
use serde::{Deserialize, Serialize};

/// Structures for default rule import
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DefaultRulePattern {
    pub find: String,
    pub replace: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DefaultRule {
    pub name: String,
    pub description: String,
    pub examples: Vec<String>,
    pub track_name: Option<DefaultRulePattern>,
    pub artist_name: Option<DefaultRulePattern>,
    pub album_name: Option<DefaultRulePattern>,
    pub album_artist_name: Option<DefaultRulePattern>,
    pub requires_confirmation: bool,
    /// Optional flag: require MusicBrainz confirmation of the rewritten metadata
    #[serde(default)]
    pub requires_musicbrainz_confirmation: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DefaultRuleSet {
    pub title: String,
    pub description: String,
    pub version: String,
    pub rules: Vec<DefaultRule>,
}

/// Convert default rule to RewriteRule
impl From<DefaultRule> for RewriteRule {
    fn from(default_rule: DefaultRule) -> Self {
        RewriteRule {
            name: Some(default_rule.name),
            track_name: default_rule.track_name.map(|p| SdRule {
                find: p.find,
                replace: p.replace,
                flags: Some("i".to_string()), // Default to case-insensitive
            }),
            artist_name: default_rule.artist_name.map(|p| SdRule {
                find: p.find,
                replace: p.replace,
                flags: Some("i".to_string()),
            }),
            album_name: default_rule.album_name.map(|p| SdRule {
                find: p.find,
                replace: p.replace,
                flags: Some("i".to_string()),
            }),
            album_artist_name: default_rule.album_artist_name.map(|p| SdRule {
                find: p.find,
                replace: p.replace,
                flags: Some("i".to_string()),
            }),
            requires_confirmation: default_rule.requires_confirmation,
            requires_musicbrainz_confirmation: default_rule.requires_musicbrainz_confirmation,
        }
    }
}

/// Load default remaster rules from embedded JSON
pub fn load_default_remaster_rules() -> Result<DefaultRuleSet, Box<dyn std::error::Error>> {
    let json_content = include_str!("../assets/default_remaster_rules.json");
    let rule_set: DefaultRuleSet = serde_json::from_str(json_content)?;
    Ok(rule_set)
}
