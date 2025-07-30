use crate::types::AppState;
use ::scrobble_scrubber::persistence::{RewriteRulesState, StateStorage};
use ::scrobble_scrubber::rewrite::{RewriteRule, SdRule};
use dioxus::document::eval;
use dioxus::prelude::*;
use lastfm_edit::Track;
use serde::{Deserialize, Serialize};

pub fn get_current_tracks(state: &AppState) -> Vec<Track> {
    let mut all_tracks = Vec::new();

    // Add recent tracks if enabled (single chronological list)
    if state.recent_tracks.enabled {
        all_tracks.extend(state.track_cache.recent_tracks.clone());
    }

    // Add artist tracks if enabled (from cache)
    for (artist_name, cached_tracks) in &state.track_cache.artist_tracks {
        if let Some(track_state) = state.artist_tracks.get(artist_name) {
            if track_state.enabled {
                all_tracks.extend(cached_tracks.clone());
            }
        }
    }

    all_tracks
}

pub async fn save_current_rule(
    mut state: Signal<AppState>,
    rule: RewriteRule,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if rule has any content
    if rule.track_name.is_none()
        && rule.artist_name.is_none()
        && rule.album_name.is_none()
        && rule.album_artist_name.is_none()
    {
        return Err("Cannot save empty rule".into());
    }

    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Load current rules
        let mut rules_state = storage_lock
            .load_rewrite_rules_state()
            .await
            .unwrap_or_default();

        // Add new rule
        rules_state.rewrite_rules.push(rule);

        // Save updated rules
        storage_lock.save_rewrite_rules_state(&rules_state).await?;

        // Update local state
        let saved_rules = rules_state.rewrite_rules;
        drop(storage_lock);
        state.with_mut(|s| s.saved_rules = saved_rules);
    }

    Ok(())
}

pub async fn remove_rule_at_index(
    mut state: Signal<AppState>,
    index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Load current rules
        let mut rules_state = storage_lock
            .load_rewrite_rules_state()
            .await
            .unwrap_or_default();

        // Remove rule at index
        if index < rules_state.rewrite_rules.len() {
            rules_state.rewrite_rules.remove(index);

            // Save updated rules
            storage_lock.save_rewrite_rules_state(&rules_state).await?;

            // Update local state
            let saved_rules = rules_state.rewrite_rules;
            drop(storage_lock);
            state.with_mut(|s| s.saved_rules = saved_rules);
        }
    }

    Ok(())
}

pub async fn clear_all_rules(
    mut state: Signal<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Clear all rules
        let empty_rules_state = RewriteRulesState::default();
        storage_lock
            .save_rewrite_rules_state(&empty_rules_state)
            .await?;

        // Update local state
        drop(storage_lock);
        state.with_mut(|s| s.saved_rules = Vec::new());
    }

    Ok(())
}

#[allow(dead_code)]
pub async fn update_rule_confirmation(
    mut state: Signal<AppState>,
    index: usize,
    requires_confirmation: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Load current rules
        let mut rules_state = storage_lock
            .load_rewrite_rules_state()
            .await
            .unwrap_or_default();

        // Update rule confirmation at index
        if index < rules_state.rewrite_rules.len() {
            rules_state.rewrite_rules[index].requires_confirmation = requires_confirmation;

            // Save updated rules
            storage_lock.save_rewrite_rules_state(&rules_state).await?;

            // Update local state
            let saved_rules = rules_state.rewrite_rules;
            drop(storage_lock);
            state.with_mut(|s| s.saved_rules = saved_rules);
        }
    }

    Ok(())
}

// Helper function to copy text to clipboard
pub fn copy_to_clipboard(text: String) {
    spawn(async move {
        let _ = eval(&format!(
            "navigator.clipboard.writeText(`{}`)",
            text.replace('`', "\\`")
        ));
    });
}

// Structures for default rule import
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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DefaultRuleSet {
    pub title: String,
    pub description: String,
    pub version: String,
    pub rules: Vec<DefaultRule>,
}

// Convert default rule to RewriteRule
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
        }
    }
}

// Load default remaster rules from embedded JSON
pub fn load_default_remaster_rules() -> Result<DefaultRuleSet, Box<dyn std::error::Error>> {
    let json_content = include_str!("../assets/default_remaster_rules.json");
    let rule_set: DefaultRuleSet = serde_json::from_str(json_content)?;
    Ok(rule_set)
}

// Import selected default rules
pub async fn import_default_rules(
    mut state: Signal<AppState>,
    rules_to_import: Vec<DefaultRule>,
) -> Result<usize, Box<dyn std::error::Error>> {
    if rules_to_import.is_empty() {
        return Ok(0);
    }

    let storage = state.read().storage.clone();
    if let Some(storage) = storage {
        let mut storage_lock = storage.lock().await;

        // Load current rules
        let mut rules_state = storage_lock
            .load_rewrite_rules_state()
            .await
            .unwrap_or_default();

        // Convert and add new rules
        let new_rules: Vec<RewriteRule> = rules_to_import
            .into_iter()
            .map(|rule| rule.into())
            .collect();

        let imported_count = new_rules.len();
        rules_state.rewrite_rules.extend(new_rules);

        // Save updated rules
        storage_lock.save_rewrite_rules_state(&rules_state).await?;

        // Update local state
        let saved_rules = rules_state.rewrite_rules;
        drop(storage_lock);
        state.with_mut(|s| s.saved_rules = saved_rules);

        Ok(imported_count)
    } else {
        Err("Storage not available".into())
    }
}
