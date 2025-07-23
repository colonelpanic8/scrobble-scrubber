use scrobble_scrubber::{
    persistence::RewriteRulesState,
    rewrite::{RewriteRule, SdRule},
    scrub_action_provider::{RewriteRulesScrubActionProvider, ScrubActionProvider},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

mod client;

pub use client::{LastFmAuthResult, LastFmEditClient, LastFmTrack};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    console_log!("scrobble-scrubber WASM bindings loaded");
}

/// Track metadata representation for JavaScript
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Track {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub playcount: u32,
    pub timestamp: Option<u64>,
}

impl Track {
    pub fn new(
        name: String,
        artist: String,
        album: Option<String>,
        playcount: u32,
        timestamp: Option<u64>,
    ) -> Track {
        Track {
            name,
            artist,
            album,
            playcount,
            timestamp,
        }
    }
}

/// JavaScript-compatible rewrite rule representation
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JSRewriteRule {
    pub track_name: Option<JSSdRule>,
    pub artist_name: Option<JSSdRule>,
    pub album_name: Option<JSSdRule>,
    pub album_artist_name: Option<JSSdRule>,
    pub requires_confirmation: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JSSdRule {
    pub find: String,
    pub replace: String,
    pub flags: Option<String>,
}

impl From<&SdRule> for JSSdRule {
    fn from(sd_rule: &SdRule) -> Self {
        JSSdRule {
            find: sd_rule.find.clone(),
            replace: sd_rule.replace.clone(),
            flags: sd_rule.flags.clone(),
        }
    }
}

impl From<JSSdRule> for SdRule {
    fn from(js_rule: JSSdRule) -> Self {
        let mut sd_rule = SdRule::new(&js_rule.find, &js_rule.replace);
        if let Some(flags) = js_rule.flags {
            sd_rule = sd_rule.with_flags(&flags);
        }
        sd_rule
    }
}

impl From<&RewriteRule> for JSRewriteRule {
    fn from(rule: &RewriteRule) -> Self {
        JSRewriteRule {
            track_name: rule.track_name.as_ref().map(JSSdRule::from),
            artist_name: rule.artist_name.as_ref().map(JSSdRule::from),
            album_name: rule.album_name.as_ref().map(JSSdRule::from),
            album_artist_name: rule.album_artist_name.as_ref().map(JSSdRule::from),
            requires_confirmation: rule.requires_confirmation,
        }
    }
}

impl From<JSRewriteRule> for RewriteRule {
    fn from(js_rule: JSRewriteRule) -> Self {
        RewriteRule {
            name: None, // WASM interface doesn't support rule names yet
            track_name: js_rule.track_name.map(SdRule::from),
            artist_name: js_rule.artist_name.map(SdRule::from),
            album_name: js_rule.album_name.map(SdRule::from),
            album_artist_name: js_rule.album_artist_name.map(SdRule::from),
            requires_confirmation: js_rule.requires_confirmation,
        }
    }
}

/// Convert a JavaScript track to an internal track representation
fn js_track_to_internal(js_track: &Track) -> lastfm_edit::Track {
    lastfm_edit::Track {
        name: js_track.name.clone(),
        artist: js_track.artist.clone(),
        album: js_track.album.clone(),
        album_artist: None, // JavaScript track doesn't have album_artist
        playcount: js_track.playcount,
        timestamp: js_track.timestamp,
    }
}

/// Test if a rewrite rule applies to a track
#[wasm_bindgen]
pub fn test_rule_applies(rule_json: &str, track_json: &str) -> Result<bool, JsValue> {
    let track: Track = serde_json::from_str(track_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse track: {e}")))?;
    let js_rule: JSRewriteRule = serde_json::from_str(rule_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse rule: {e}")))?;

    let rewrite_rule: RewriteRule = js_rule.into();
    let internal_track = js_track_to_internal(&track);

    rewrite_rule
        .applies_to(&internal_track)
        .map_err(|e| JsValue::from_str(&format!("Rule application error: {e}")))
}

/// Apply a rewrite rule to a track and get the result
#[wasm_bindgen]
pub fn apply_rule_to_track(rule_json: &str, track_json: &str) -> Result<JsValue, JsValue> {
    let track: Track = serde_json::from_str(track_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse track: {e}")))?;
    let js_rule: JSRewriteRule = serde_json::from_str(rule_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse rule: {e}")))?;

    let rewrite_rule: RewriteRule = js_rule.into();
    let internal_track = js_track_to_internal(&track);

    // Create a no-op edit from the track
    let mut edit = scrobble_scrubber::rewrite::create_no_op_edit(&internal_track);

    // Apply the rule
    let changed = scrobble_scrubber::rewrite::apply_all_rules(&[rewrite_rule], &mut edit)
        .map_err(|e| JsValue::from_str(&format!("Rule application error: {e}")))?;

    let result = serde_json::json!({
        "changed": changed,
        "edit": {
            "track_name_original": edit.track_name_original,
            "track_name": edit.track_name,
            "artist_name_original": edit.artist_name_original,
            "artist_name": edit.artist_name,
            "album_name_original": edit.album_name_original,
            "album_name": edit.album_name,
            "album_artist_name_original": edit.album_artist_name_original,
            "album_artist_name": edit.album_artist_name,
        }
    });

    serde_wasm_bindgen::to_value(&result)
        .map_err(|e| JsValue::from_str(&format!("Failed to convert to JS: {e}")))
}

/// Apply multiple rules to a track and return suggestions
#[wasm_bindgen]
pub fn analyze_track_with_rules(rules_json: &str, track_json: &str) -> Result<JsValue, JsValue> {
    let track: Track = serde_json::from_str(track_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse track: {e}")))?;
    let js_rules: Vec<JSRewriteRule> = serde_json::from_str(rules_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse rules: {e}")))?;

    let rewrite_rules: Vec<RewriteRule> = js_rules.into_iter().map(RewriteRule::from).collect();
    let internal_track = js_track_to_internal(&track);

    // Create a rewrite rules state
    let rules_state = RewriteRulesState { rewrite_rules };

    // Create a provider
    let provider = RewriteRulesScrubActionProvider::new(&rules_state);

    // Analyze the track
    let tracks = vec![internal_track];
    let result =
        futures::executor::block_on(async { provider.analyze_tracks(&tracks, None, None).await });

    let suggestions = result.map_err(|e| JsValue::from_str(&format!("Analysis error: {e}")))?;

    // Convert suggestions to a simple format since ScrubActionSuggestion doesn't implement Serialize
    let json_suggestions = serde_json::json!({
        "suggestions_count": suggestions.len(),
        "message": "Analysis complete - check console for details"
    });

    serde_wasm_bindgen::to_value(&json_suggestions)
        .map_err(|e| JsValue::from_str(&format!("Failed to convert to JS: {e}")))
}

/// Create a simple rewrite rule from pattern and replacement
#[wasm_bindgen]
pub fn create_simple_rule(
    field: &str,
    find: &str,
    replace: &str,
    _is_literal: bool,
) -> Result<String, JsValue> {
    let sd_rule = SdRule::new(find, replace);

    let mut rewrite_rule = RewriteRule {
        name: None, // WASM interface doesn't support rule names yet
        track_name: None,
        artist_name: None,
        album_name: None,
        album_artist_name: None,
        requires_confirmation: false,
    };

    match field {
        "track_name" => rewrite_rule.track_name = Some(sd_rule),
        "artist_name" => rewrite_rule.artist_name = Some(sd_rule),
        "album_name" => rewrite_rule.album_name = Some(sd_rule),
        "album_artist_name" => rewrite_rule.album_artist_name = Some(sd_rule),
        _ => return Err(JsValue::from_str(&format!("Invalid field: {field}"))),
    }

    let js_rule = JSRewriteRule::from(&rewrite_rule);

    serde_json::to_string(&js_rule)
        .map_err(|e| JsValue::from_str(&format!("Failed to serialize rule: {e}")))
}

/// Validate a regex pattern
#[wasm_bindgen]
pub fn validate_regex(pattern: &str) -> Result<JsValue, JsValue> {
    match regex::Regex::new(pattern) {
        Ok(_) => {
            let result = serde_json::json!({
                "valid": true,
                "message": "Regex is valid"
            });
            serde_wasm_bindgen::to_value(&result)
                .map_err(|e| JsValue::from_str(&format!("Failed to convert to JS: {e}")))
        }
        Err(e) => {
            let result = serde_json::json!({
                "valid": false,
                "error": e.to_string()
            });
            serde_wasm_bindgen::to_value(&result)
                .map_err(|e| JsValue::from_str(&format!("Failed to convert to JS: {e}")))
        }
    }
}

/// Test a regex pattern against a string
#[wasm_bindgen]
pub fn test_regex(pattern: &str, text: &str, replacement: &str) -> Result<JsValue, JsValue> {
    match regex::Regex::new(pattern) {
        Ok(re) => {
            let result = re.replace_all(text, replacement);
            let response = serde_json::json!({
                "success": true,
                "result": result.to_string(),
                "matched": re.is_match(text)
            });
            serde_wasm_bindgen::to_value(&response)
                .map_err(|e| JsValue::from_str(&format!("Failed to convert to JS: {e}")))
        }
        Err(e) => {
            let response = serde_json::json!({
                "success": false,
                "error": e.to_string()
            });
            serde_wasm_bindgen::to_value(&response)
                .map_err(|e| JsValue::from_str(&format!("Failed to convert to JS: {e}")))
        }
    }
}

/// Create a new track from JavaScript
#[wasm_bindgen]
pub fn create_track(
    name: &str,
    artist: &str,
    album: Option<String>,
    playcount: u32,
    timestamp: Option<u64>,
) -> JsValue {
    let track = Track {
        name: name.to_string(),
        artist: artist.to_string(),
        album,
        playcount,
        timestamp,
    };

    serde_wasm_bindgen::to_value(&track).unwrap_or(JsValue::null())
}

/// Process a collection of tracks with multiple rules
#[wasm_bindgen]
pub fn process_tracks_with_rules(tracks_json: &str, rules_json: &str) -> Result<JsValue, JsValue> {
    let tracks: Vec<Track> = serde_json::from_str(tracks_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse tracks: {e}")))?;

    let js_rules: Vec<JSRewriteRule> = serde_json::from_str(rules_json)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse rules: {e}")))?;

    let rewrite_rules: Vec<RewriteRule> = js_rules.into_iter().map(RewriteRule::from).collect();

    let mut results = Vec::new();

    for (index, track) in tracks.iter().enumerate() {
        let internal_track = js_track_to_internal(track);

        // Test each rule against the track
        let mut track_results = Vec::new();

        for (rule_index, rule) in rewrite_rules.iter().enumerate() {
            let applies = match rule.applies_to(&internal_track) {
                Ok(applies) => applies,
                Err(_) => continue,
            };

            if applies {
                let mut edit = scrobble_scrubber::rewrite::create_no_op_edit(&internal_track);
                match scrobble_scrubber::rewrite::apply_all_rules(&[rule.clone()], &mut edit) {
                    Ok(changed) => {
                        if changed {
                            track_results.push(serde_json::json!({
                                "rule_index": rule_index,
                                "changed": true,
                                "edit": {
                                    "track_name_original": edit.track_name_original,
                                    "track_name": edit.track_name,
                                    "artist_name_original": edit.artist_name_original,
                                    "artist_name": edit.artist_name,
                                    "album_name_original": edit.album_name_original,
                                    "album_name": edit.album_name,
                                    "album_artist_name_original": edit.album_artist_name_original,
                                    "album_artist_name": edit.album_artist_name,
                                }
                            }));
                        }
                    }
                    Err(_) => continue,
                }
            }
        }

        if !track_results.is_empty() {
            results.push(serde_json::json!({
                "track_index": index,
                "original_track": track,
                "results": track_results
            }));
        }
    }

    let final_result = serde_json::json!({
        "processed_tracks": tracks.len(),
        "tracks_with_changes": results.len(),
        "results": results
    });

    serde_wasm_bindgen::to_value(&final_result)
        .map_err(|e| JsValue::from_str(&format!("Failed to convert to JS: {e}")))
}

/// Get common rewrite rule templates
#[wasm_bindgen]
pub fn get_common_rule_templates() -> JsValue {
    let templates = serde_json::json!([
        {
            "name": "Remove Remaster from Track Names",
            "description": "Removes remaster indicators like '(2009 Remaster)' from track names",
            "rule": {
                "track_name": {
                    "find": "^(.*)\\s*\\(.*[Rr]emaster.*\\)\\s*$",
                    "replace": "$1",
                    "flags": null,
                },
                "artist_name": null,
                "album_name": null,
                "album_artist_name": null,
                "requires_confirmation": false
            }
        },
        {
            "name": "Remove Deluxe Edition from Album Names",
            "description": "Removes deluxe edition indicators from album names",
            "rule": {
                "track_name": null,
                "artist_name": null,
                "album_name": {
                    "find": "^(.*)\\s*\\([Dd]eluxe [Ee]dition\\)\\s*$",
                    "replace": "$1",
                    "flags": null,
                },
                "album_artist_name": null,
                "requires_confirmation": false
            }
        },
        {
            "name": "Normalize Featuring Format",
            "description": "Converts 'ft.' to 'feat.' in track names",
            "rule": {
                "track_name": {
                    "find": "^(.*)\\s+ft\\.\\s+(.*)$",
                    "replace": "$1 feat. $2",
                    "flags": null,
                },
                "artist_name": null,
                "album_name": null,
                "album_artist_name": null,
                "requires_confirmation": false
            }
        },
        {
            "name": "Remove Year Suffixes",
            "description": "Removes year suffixes like '- 2009' from track names",
            "rule": {
                "track_name": {
                    "find": "^(.*)\\s*-\\s*[0-9]{4}\\s*$",
                    "replace": "$1",
                    "flags": null,
                },
                "artist_name": null,
                "album_name": null,
                "album_artist_name": null,
                "requires_confirmation": false
            }
        },
        {
            "name": "Remove Radio Edit Labels",
            "description": "Removes '(Radio Edit)' and similar labels from track names",
            "rule": {
                "track_name": {
                    "find": "^(.*)\\s*\\([Rr]adio [Ee]dit\\)\\s*$",
                    "replace": "$1",
                    "flags": null,
                },
                "artist_name": null,
                "album_name": null,
                "album_artist_name": null,
                "requires_confirmation": false
            }
        }
    ]);

    serde_wasm_bindgen::to_value(&templates).unwrap_or(JsValue::null())
}
