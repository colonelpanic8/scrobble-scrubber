use crate::types::AppState;
use ::scrobble_scrubber::persistence::{RewriteRulesState, StateStorage};
use ::scrobble_scrubber::rewrite::RewriteRule;
use dioxus::document::eval;
use dioxus::prelude::*;
use lastfm_edit::Track;

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
