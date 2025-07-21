use crate::rewrite::RewriteRule;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimestampState {
    /// Timestamp of the most recent processed scrobble
    pub last_processed_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RewriteRulesState {
    /// Set of regex rewrite rules for cleaning track/artist names
    pub rewrite_rules: Vec<RewriteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEdit {
    /// Unique identifier for this pending edit
    pub id: Uuid,
    /// Timestamp when this edit was created
    pub created_at: DateTime<Utc>,
    /// Original track information
    pub original_track_name: String,
    pub original_artist_name: String,
    pub original_album_name: Option<String>,
    pub original_album_artist_name: Option<String>,
    /// Proposed changes
    pub new_track_name: Option<String>,
    pub new_artist_name: Option<String>,
    pub new_album_name: Option<String>,
    pub new_album_artist_name: Option<String>,
    /// Timestamp of the scrobble being edited
    pub scrobble_timestamp: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingEditsState {
    /// List of pending edits awaiting confirmation
    pub pending_edits: Vec<PendingEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRewriteRule {
    /// Unique identifier for this pending rule
    pub id: Uuid,
    /// Timestamp when this rule suggestion was created
    pub created_at: DateTime<Utc>,
    /// The suggested rewrite rule
    pub rule: RewriteRule,
    /// Explanation of why this rule was suggested
    pub reason: String,
    /// Track that triggered this suggestion
    pub example_track_name: String,
    pub example_artist_name: String,
    pub example_album_name: Option<String>,
    pub example_album_artist_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingRewriteRulesState {
    /// List of pending rewrite rules awaiting approval
    pub pending_rules: Vec<PendingRewriteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsState {
    /// Global setting to require confirmation for all edits
    pub require_confirmation: bool,
}

impl PendingEdit {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        original_track_name: String,
        original_artist_name: String,
        original_album_name: Option<String>,
        original_album_artist_name: Option<String>,
        new_track_name: Option<String>,
        new_artist_name: Option<String>,
        new_album_name: Option<String>,
        new_album_artist_name: Option<String>,
        scrobble_timestamp: Option<u64>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            original_track_name,
            original_artist_name,
            original_album_name,
            original_album_artist_name,
            new_track_name,
            new_artist_name,
            new_album_name,
            new_album_artist_name,
            scrobble_timestamp,
        }
    }
}

impl PendingRewriteRule {
    #[must_use]
    pub fn new(
        rule: RewriteRule,
        reason: String,
        example_track_name: String,
        example_artist_name: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            rule,
            reason,
            example_track_name,
            example_artist_name,
            example_album_name: None,
            example_album_artist_name: None,
        }
    }

    #[must_use]
    pub fn new_with_album_info(
        rule: RewriteRule,
        reason: String,
        example_track_name: String,
        example_artist_name: String,
        example_album_name: Option<String>,
        example_album_artist_name: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            rule,
            reason,
            example_track_name,
            example_artist_name,
            example_album_name,
            example_album_artist_name,
        }
    }

    /// Apply the rewrite rule to the example track and return the transformed values
    pub fn apply_rule_to_example(
        &self,
    ) -> Result<TransformedExample, crate::rewrite::RewriteError> {
        use lastfm_edit::Track;

        // Create a Track from the example data
        let example_track = Track {
            name: self.example_track_name.clone(),
            artist: self.example_artist_name.clone(),
            album: self.example_album_name.clone(),
            playcount: 0,
            timestamp: None,
        };

        // Apply the rule to see the transformation
        let mut edit = crate::rewrite::create_no_op_edit(&example_track);
        let changes_made = self.rule.apply(&mut edit)?;

        Ok(TransformedExample {
            original_track_name: self.example_track_name.clone(),
            transformed_track_name: if edit.track_name != edit.track_name_original {
                Some(edit.track_name)
            } else {
                None
            },
            original_artist_name: self.example_artist_name.clone(),
            transformed_artist_name: if edit.artist_name != edit.artist_name_original {
                Some(edit.artist_name)
            } else {
                None
            },
            original_album_name: self.example_album_name.clone(),
            transformed_album_name: if edit.album_name != edit.album_name_original {
                Some(edit.album_name)
            } else {
                None
            },
            original_album_artist_name: self.example_album_artist_name.clone(),
            transformed_album_artist_name: if edit.album_artist_name
                != edit.album_artist_name_original
            {
                Some(edit.album_artist_name)
            } else {
                None
            },
            changes_made,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformedExample {
    pub original_track_name: String,
    pub transformed_track_name: Option<String>,
    pub original_artist_name: String,
    pub transformed_artist_name: Option<String>,
    pub original_album_name: Option<String>,
    pub transformed_album_name: Option<String>,
    pub original_album_artist_name: Option<String>,
    pub transformed_album_artist_name: Option<String>,
    pub changes_made: bool,
}

#[async_trait]
pub trait StateStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Clear all stored state
    #[allow(dead_code)]
    async fn clear_state(&mut self) -> Result<(), Self::Error>;

    /// Load the timestamp state
    async fn load_timestamp_state(&self) -> Result<TimestampState, Self::Error>;

    /// Save the timestamp state
    async fn save_timestamp_state(&mut self, state: &TimestampState) -> Result<(), Self::Error>;

    /// Load the rewrite rules state
    async fn load_rewrite_rules_state(&self) -> Result<RewriteRulesState, Self::Error>;

    /// Save the rewrite rules state
    async fn save_rewrite_rules_state(
        &mut self,
        state: &RewriteRulesState,
    ) -> Result<(), Self::Error>;

    /// Load the pending edits state
    async fn load_pending_edits_state(&self) -> Result<PendingEditsState, Self::Error>;

    /// Save the pending edits state
    async fn save_pending_edits_state(
        &mut self,
        state: &PendingEditsState,
    ) -> Result<(), Self::Error>;

    /// Load the pending rewrite rules state
    async fn load_pending_rewrite_rules_state(
        &self,
    ) -> Result<PendingRewriteRulesState, Self::Error>;

    /// Save the pending rewrite rules state
    async fn save_pending_rewrite_rules_state(
        &mut self,
        state: &PendingRewriteRulesState,
    ) -> Result<(), Self::Error>;

    /// Load the settings state
    async fn load_settings_state(&self) -> Result<SettingsState, Self::Error>;

    /// Save the settings state
    async fn save_settings_state(&mut self, state: &SettingsState) -> Result<(), Self::Error>;
}

/// File-based storage implementation using pickledb
pub struct FileStorage {
    db: pickledb::PickleDb,
}

impl FileStorage {
    pub fn new(path: &str) -> Result<Self, pickledb::error::Error> {
        let db = pickledb::PickleDb::load_json(path, pickledb::PickleDbDumpPolicy::AutoDump)
            .unwrap_or_else(|_| {
                pickledb::PickleDb::new_json(path, pickledb::PickleDbDumpPolicy::AutoDump)
            });
        Ok(Self { db })
    }
}

#[async_trait]
impl StateStorage for FileStorage {
    type Error = pickledb::error::Error;

    async fn clear_state(&mut self) -> Result<(), Self::Error> {
        self.db.rem("timestamp_state").ok();
        self.db.rem("rewrite_rules_state").ok();
        Ok(())
    }

    async fn load_timestamp_state(&self) -> Result<TimestampState, Self::Error> {
        match self.db.get("timestamp_state") {
            Some(state) => Ok(state),
            None => Ok(TimestampState::default()),
        }
    }

    async fn save_timestamp_state(&mut self, state: &TimestampState) -> Result<(), Self::Error> {
        self.db.set("timestamp_state", state)?;
        Ok(())
    }

    async fn load_rewrite_rules_state(&self) -> Result<RewriteRulesState, Self::Error> {
        match self.db.get("rewrite_rules_state") {
            Some(state) => Ok(state),
            None => Ok(RewriteRulesState::default()),
        }
    }

    async fn save_rewrite_rules_state(
        &mut self,
        state: &RewriteRulesState,
    ) -> Result<(), Self::Error> {
        self.db.set("rewrite_rules_state", state)?;
        Ok(())
    }

    async fn load_pending_edits_state(&self) -> Result<PendingEditsState, Self::Error> {
        match self.db.get("pending_edits_state") {
            Some(state) => Ok(state),
            None => Ok(PendingEditsState::default()),
        }
    }

    async fn save_pending_edits_state(
        &mut self,
        state: &PendingEditsState,
    ) -> Result<(), Self::Error> {
        self.db.set("pending_edits_state", state)?;
        Ok(())
    }

    async fn load_pending_rewrite_rules_state(
        &self,
    ) -> Result<PendingRewriteRulesState, Self::Error> {
        match self.db.get("pending_rewrite_rules_state") {
            Some(state) => Ok(state),
            None => Ok(PendingRewriteRulesState::default()),
        }
    }

    async fn save_pending_rewrite_rules_state(
        &mut self,
        state: &PendingRewriteRulesState,
    ) -> Result<(), Self::Error> {
        self.db.set("pending_rewrite_rules_state", state)?;
        Ok(())
    }

    async fn load_settings_state(&self) -> Result<SettingsState, Self::Error> {
        match self.db.get("settings_state") {
            Some(state) => Ok(state),
            None => Ok(SettingsState::default()),
        }
    }

    async fn save_settings_state(&mut self, state: &SettingsState) -> Result<(), Self::Error> {
        self.db.set("settings_state", state)?;
        Ok(())
    }
}

/// In-memory storage implementation for testing
pub struct MemoryStorage {
    timestamp_state: tokio::sync::RwLock<TimestampState>,
    rewrite_rules_state: tokio::sync::RwLock<RewriteRulesState>,
    pending_edits_state: tokio::sync::RwLock<PendingEditsState>,
    pending_rewrite_rules_state: tokio::sync::RwLock<PendingRewriteRulesState>,
    settings_state: tokio::sync::RwLock<SettingsState>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    #[allow(dead_code)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            timestamp_state: tokio::sync::RwLock::new(TimestampState::default()),
            rewrite_rules_state: tokio::sync::RwLock::new(RewriteRulesState::default()),
            pending_edits_state: tokio::sync::RwLock::new(PendingEditsState::default()),
            pending_rewrite_rules_state: tokio::sync::RwLock::new(
                PendingRewriteRulesState::default(),
            ),
            settings_state: tokio::sync::RwLock::new(SettingsState::default()),
        }
    }
}

#[async_trait]
impl StateStorage for MemoryStorage {
    type Error = std::convert::Infallible;

    async fn clear_state(&mut self) -> Result<(), Self::Error> {
        *self.timestamp_state.write().await = TimestampState::default();
        *self.rewrite_rules_state.write().await = RewriteRulesState::default();
        *self.pending_edits_state.write().await = PendingEditsState::default();
        *self.pending_rewrite_rules_state.write().await = PendingRewriteRulesState::default();
        *self.settings_state.write().await = SettingsState::default();
        Ok(())
    }

    async fn load_timestamp_state(&self) -> Result<TimestampState, Self::Error> {
        Ok(self.timestamp_state.read().await.clone())
    }

    async fn save_timestamp_state(&mut self, state: &TimestampState) -> Result<(), Self::Error> {
        *self.timestamp_state.write().await = state.clone();
        Ok(())
    }

    async fn load_rewrite_rules_state(&self) -> Result<RewriteRulesState, Self::Error> {
        Ok(self.rewrite_rules_state.read().await.clone())
    }

    async fn save_rewrite_rules_state(
        &mut self,
        state: &RewriteRulesState,
    ) -> Result<(), Self::Error> {
        *self.rewrite_rules_state.write().await = state.clone();
        Ok(())
    }

    async fn load_pending_edits_state(&self) -> Result<PendingEditsState, Self::Error> {
        Ok(self.pending_edits_state.read().await.clone())
    }

    async fn save_pending_edits_state(
        &mut self,
        state: &PendingEditsState,
    ) -> Result<(), Self::Error> {
        *self.pending_edits_state.write().await = state.clone();
        Ok(())
    }

    async fn load_pending_rewrite_rules_state(
        &self,
    ) -> Result<PendingRewriteRulesState, Self::Error> {
        Ok(self.pending_rewrite_rules_state.read().await.clone())
    }

    async fn save_pending_rewrite_rules_state(
        &mut self,
        state: &PendingRewriteRulesState,
    ) -> Result<(), Self::Error> {
        *self.pending_rewrite_rules_state.write().await = state.clone();
        Ok(())
    }

    async fn load_settings_state(&self) -> Result<SettingsState, Self::Error> {
        Ok(self.settings_state.read().await.clone())
    }

    async fn save_settings_state(&mut self, state: &SettingsState) -> Result<(), Self::Error> {
        *self.settings_state.write().await = state.clone();
        Ok(())
    }
}

impl RewriteRulesState {
    #[must_use]
    pub fn with_default_rules() -> Self {
        Self {
            rewrite_rules: crate::rewrite::default_rules(),
        }
    }
}
