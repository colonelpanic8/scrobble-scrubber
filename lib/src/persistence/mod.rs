use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// use uuid::Uuid;

use crate::rewrite::RewriteRule;

/// Preview of rule transformation showing changes
#[derive(Debug, Clone)]
pub struct RuleTransformationPreview {
    pub original_track_name: String,
    pub original_artist_name: String,
    pub original_album_name: Option<String>,
    pub original_album_artist_name: Option<String>,
    pub transformed_track_name: Option<String>,
    pub transformed_artist_name: Option<String>,
    pub transformed_album_name: Option<String>,
    pub transformed_album_artist_name: Option<String>,
}

/// Core persistence traits and types
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimestampState {
    pub last_processed_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RewriteRulesState {
    pub rewrite_rules: Vec<RewriteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEdit {
    pub id: String,
    pub original_track_name: String,
    pub original_artist_name: String,
    pub original_album_name: Option<String>,
    pub original_album_artist_name: Option<String>,
    pub new_track_name: Option<String>,
    pub new_artist_name: Option<String>,
    pub new_album_name: Option<String>,
    pub new_album_artist_name: Option<String>,
    pub timestamp: Option<u64>,
}

impl PendingEdit {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        original_track_name: String,
        original_artist_name: String,
        original_album_name: Option<String>,
        original_album_artist_name: Option<String>,
        new_track_name: Option<String>,
        new_artist_name: Option<String>,
        new_album_name: Option<String>,
        new_album_artist_name: Option<String>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            id: format!(
                "id-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            original_track_name,
            original_artist_name,
            original_album_name,
            original_album_artist_name,
            new_track_name,
            new_artist_name,
            new_album_name,
            new_album_artist_name,
            timestamp,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRewriteRule {
    pub id: String,
    pub rule: RewriteRule,
    pub reason: String,
    pub example_track_name: String,
    pub example_artist_name: String,
    pub example_album_name: Option<String>,
    pub example_album_artist_name: Option<String>,
}

impl PendingRewriteRule {
    pub fn new(
        rule: RewriteRule,
        reason: String,
        example_track_name: String,
        example_artist_name: String,
    ) -> Self {
        Self {
            id: format!(
                "id-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            rule,
            reason,
            example_track_name,
            example_artist_name,
            example_album_name: None,
            example_album_artist_name: None,
        }
    }

    pub fn new_with_album_info(
        rule: RewriteRule,
        reason: String,
        example_track_name: String,
        example_artist_name: String,
        example_album_name: Option<String>,
        example_album_artist_name: Option<String>,
    ) -> Self {
        Self {
            id: format!(
                "id-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            rule,
            reason,
            example_track_name,
            example_artist_name,
            example_album_name,
            example_album_artist_name,
        }
    }

    /// Apply the rule to the example and return a preview
    pub fn apply_rule_to_example(
        &self,
    ) -> Result<RuleTransformationPreview, crate::rewrite::RewriteError> {
        // Create a mock track from the example
        let track = lastfm_edit::Track {
            name: self.example_track_name.clone(),
            artist: self.example_artist_name.clone(),
            playcount: 1,                // placeholder
            timestamp: Some(1640995200), // placeholder timestamp
            album: self.example_album_name.clone(),
        };

        let original_track_name = self.example_track_name.clone();
        let original_artist_name = self.example_artist_name.clone();
        let original_album_name = self.example_album_name.clone();
        let original_album_artist_name = self.example_album_artist_name.clone();

        // Apply the rule to see the transformation
        if self.rule.applies_to(&track)? {
            let mut edit = crate::rewrite::create_no_op_edit(&track);
            let changed = self.rule.apply(&mut edit)?;

            if changed {
                Ok(RuleTransformationPreview {
                    original_track_name: original_track_name.clone(),
                    original_artist_name: original_artist_name.clone(),
                    original_album_name: original_album_name.clone(),
                    original_album_artist_name: original_album_artist_name.clone(),
                    transformed_track_name: if edit.track_name != original_track_name {
                        Some(edit.track_name)
                    } else {
                        None
                    },
                    transformed_artist_name: if edit.artist_name != original_artist_name {
                        Some(edit.artist_name)
                    } else {
                        None
                    },
                    transformed_album_name: if original_album_name
                        .as_ref()
                        .is_some_and(|orig| edit.album_name != *orig)
                    {
                        Some(edit.album_name)
                    } else {
                        None
                    },
                    transformed_album_artist_name: if original_album_artist_name
                        .as_ref()
                        .is_some_and(|orig| edit.album_artist_name != *orig)
                    {
                        Some(edit.album_artist_name)
                    } else {
                        None
                    },
                })
            } else {
                Ok(RuleTransformationPreview {
                    original_track_name,
                    original_artist_name,
                    original_album_name,
                    original_album_artist_name,
                    transformed_track_name: None,
                    transformed_artist_name: None,
                    transformed_album_name: None,
                    transformed_album_artist_name: None,
                })
            }
        } else {
            Ok(RuleTransformationPreview {
                original_track_name,
                original_artist_name,
                original_album_name,
                original_album_artist_name,
                transformed_track_name: None,
                transformed_artist_name: None,
                transformed_album_name: None,
                transformed_album_artist_name: None,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingEditsState {
    pub pending_edits: Vec<PendingEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingRewriteRulesState {
    pub pending_rules: Vec<PendingRewriteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsState {
    pub require_confirmation: bool,
}

/// Main persistence trait
#[async_trait]
pub trait StateStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn save_timestamp_state(&mut self, state: &TimestampState) -> Result<(), Self::Error>;
    async fn load_timestamp_state(&self) -> Result<TimestampState, Self::Error>;

    async fn save_rewrite_rules_state(
        &mut self,
        state: &RewriteRulesState,
    ) -> Result<(), Self::Error>;
    async fn load_rewrite_rules_state(&self) -> Result<RewriteRulesState, Self::Error>;

    async fn save_pending_edits_state(
        &mut self,
        state: &PendingEditsState,
    ) -> Result<(), Self::Error>;
    async fn load_pending_edits_state(&self) -> Result<PendingEditsState, Self::Error>;

    async fn save_pending_rewrite_rules_state(
        &mut self,
        state: &PendingRewriteRulesState,
    ) -> Result<(), Self::Error>;
    async fn load_pending_rewrite_rules_state(
        &self,
    ) -> Result<PendingRewriteRulesState, Self::Error>;

    async fn save_settings_state(&mut self, state: &SettingsState) -> Result<(), Self::Error>;
    async fn load_settings_state(&self) -> Result<SettingsState, Self::Error>;
}

// Re-export implementations
#[cfg(feature = "pickledb")]
mod file_storage;
#[cfg(feature = "pickledb")]
pub use file_storage::FileStorage;

mod memory_storage;
pub use memory_storage::MemoryStorage;
