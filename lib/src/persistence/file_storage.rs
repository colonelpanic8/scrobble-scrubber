use async_trait::async_trait;
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};
use std::path::Path;

use super::{
    PendingEditsState, PendingRewriteRulesState, RewriteRulesState, SettingsState, StateStorage,
    TimestampState,
};
use crate::rewrite::load_comprehensive_default_rules;

/// PickleDB-based file storage implementation
pub struct FileStorage {
    db: PickleDb,
}

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum FileStorageError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl FileStorage {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, FileStorageError> {
        let path_ref = path.as_ref();

        // Check if this is a new database
        let is_new_database = !path_ref.exists();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path_ref.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = match PickleDb::load(
            path_ref,
            PickleDbDumpPolicy::AutoDump,
            SerializationMethod::Json,
        ) {
            Ok(db) => db,
            Err(_) => PickleDb::new(
                path_ref,
                PickleDbDumpPolicy::AutoDump,
                SerializationMethod::Json,
            ),
        };

        let mut storage = Self { db };

        // Initialize default rules for new databases
        if is_new_database {
            if let Err(e) = storage.initialize_default_rules() {
                log::warn!("Failed to initialize default rules for new database: {e}");
            } else {
                log::info!("Initialized new database with comprehensive default rewrite rules");
            }
        }

        Ok(storage)
    }

    /// Initialize default rewrite rules for a new database
    fn initialize_default_rules(&mut self) -> Result<(), FileStorageError> {
        let default_rules = load_comprehensive_default_rules();
        let rules_state = RewriteRulesState {
            rewrite_rules: default_rules,
        };

        self.db
            .set("rewrite_rules_state", &rules_state)
            .map_err(|e| FileStorageError::SerializationError(e.to_string()))?;

        // Force a database dump to ensure the changes are persisted immediately
        self.db
            .dump()
            .map_err(|e| FileStorageError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl StateStorage for FileStorage {
    type Error = FileStorageError;

    async fn save_timestamp_state(&mut self, state: &TimestampState) -> Result<(), Self::Error> {
        self.db
            .set("timestamp_state", state)
            .map_err(|e| FileStorageError::SerializationError(e.to_string()))?;
        Ok(())
    }

    async fn load_timestamp_state(&self) -> Result<TimestampState, Self::Error> {
        Ok(self.db.get("timestamp_state").unwrap_or_default())
    }

    async fn save_rewrite_rules_state(
        &mut self,
        state: &RewriteRulesState,
    ) -> Result<(), Self::Error> {
        self.db
            .set("rewrite_rules_state", state)
            .map_err(|e| FileStorageError::SerializationError(e.to_string()))?;

        // Force a database dump to ensure the changes are persisted immediately
        self.db
            .dump()
            .map_err(|e| FileStorageError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn load_rewrite_rules_state(&self) -> Result<RewriteRulesState, Self::Error> {
        Ok(self.db.get("rewrite_rules_state").unwrap_or_default())
    }

    async fn save_pending_edits_state(
        &mut self,
        state: &PendingEditsState,
    ) -> Result<(), Self::Error> {
        self.db
            .set("pending_edits_state", state)
            .map_err(|e| FileStorageError::SerializationError(e.to_string()))?;

        // Force a database dump to ensure the changes are persisted immediately
        self.db
            .dump()
            .map_err(|e| FileStorageError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn load_pending_edits_state(&self) -> Result<PendingEditsState, Self::Error> {
        Ok(self.db.get("pending_edits_state").unwrap_or_default())
    }

    async fn save_pending_rewrite_rules_state(
        &mut self,
        state: &PendingRewriteRulesState,
    ) -> Result<(), Self::Error> {
        self.db
            .set("pending_rewrite_rules_state", state)
            .map_err(|e| FileStorageError::SerializationError(e.to_string()))?;

        // Force a database dump to ensure the changes are persisted immediately
        self.db
            .dump()
            .map_err(|e| FileStorageError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn load_pending_rewrite_rules_state(
        &self,
    ) -> Result<PendingRewriteRulesState, Self::Error> {
        Ok(self
            .db
            .get("pending_rewrite_rules_state")
            .unwrap_or_default())
    }

    async fn save_settings_state(&mut self, state: &SettingsState) -> Result<(), Self::Error> {
        self.db
            .set("settings_state", state)
            .map_err(|e| FileStorageError::SerializationError(e.to_string()))?;
        Ok(())
    }

    async fn load_settings_state(&self) -> Result<SettingsState, Self::Error> {
        Ok(self.db.get("settings_state").unwrap_or_default())
    }
}

// PickleDb is not Send + Sync by default, but since we're using it in a controlled manner
// with proper synchronization via Arc<Mutex<FileStorage>>, we can safely implement Send + Sync
unsafe impl Send for FileStorage {}
unsafe impl Sync for FileStorage {}
