use async_trait::async_trait;
use std::sync::{Arc, RwLock};

use super::{
    PendingEditsState, PendingRewriteRulesState, RewriteRulesState, SettingsState, StateStorage,
    TimestampState,
};

/// In-memory storage implementation - perfect for WASM and testing
#[derive(Debug)]
pub struct MemoryStorage {
    timestamp_state: Arc<RwLock<TimestampState>>,
    rewrite_rules_state: Arc<RwLock<RewriteRulesState>>,
    pending_edits_state: Arc<RwLock<PendingEditsState>>,
    pending_rules_state: Arc<RwLock<PendingRewriteRulesState>>,
    settings_state: Arc<RwLock<SettingsState>>,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryStorageError {
    #[error("Lock error: {0}")]
    LockError(String),
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            timestamp_state: Arc::new(RwLock::new(TimestampState::default())),
            rewrite_rules_state: Arc::new(RwLock::new(RewriteRulesState::default())),
            pending_edits_state: Arc::new(RwLock::new(PendingEditsState::default())),
            pending_rules_state: Arc::new(RwLock::new(PendingRewriteRulesState::default())),
            settings_state: Arc::new(RwLock::new(SettingsState::default())),
        }
    }

    pub fn with_initial_rules(rules: RewriteRulesState) -> Self {
        let storage = Self::new();
        *storage.rewrite_rules_state.write().unwrap() = rules;
        storage
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateStorage for MemoryStorage {
    type Error = MemoryStorageError;

    async fn save_timestamp_state(&mut self, state: &TimestampState) -> Result<(), Self::Error> {
        *self
            .timestamp_state
            .write()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))? = state.clone();
        Ok(())
    }

    async fn load_timestamp_state(&self) -> Result<TimestampState, Self::Error> {
        Ok(self
            .timestamp_state
            .read()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))?
            .clone())
    }

    async fn save_rewrite_rules_state(
        &mut self,
        state: &RewriteRulesState,
    ) -> Result<(), Self::Error> {
        *self
            .rewrite_rules_state
            .write()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))? = state.clone();
        Ok(())
    }

    async fn load_rewrite_rules_state(&self) -> Result<RewriteRulesState, Self::Error> {
        Ok(self
            .rewrite_rules_state
            .read()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))?
            .clone())
    }

    async fn save_pending_edits_state(
        &mut self,
        state: &PendingEditsState,
    ) -> Result<(), Self::Error> {
        *self
            .pending_edits_state
            .write()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))? = state.clone();
        Ok(())
    }

    async fn load_pending_edits_state(&self) -> Result<PendingEditsState, Self::Error> {
        Ok(self
            .pending_edits_state
            .read()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))?
            .clone())
    }

    async fn save_pending_rewrite_rules_state(
        &mut self,
        state: &PendingRewriteRulesState,
    ) -> Result<(), Self::Error> {
        *self
            .pending_rules_state
            .write()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))? = state.clone();
        Ok(())
    }

    async fn load_pending_rewrite_rules_state(
        &self,
    ) -> Result<PendingRewriteRulesState, Self::Error> {
        Ok(self
            .pending_rules_state
            .read()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))?
            .clone())
    }

    async fn save_settings_state(&mut self, state: &SettingsState) -> Result<(), Self::Error> {
        *self
            .settings_state
            .write()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))? = state.clone();
        Ok(())
    }

    async fn load_settings_state(&self) -> Result<SettingsState, Self::Error> {
        Ok(self
            .settings_state
            .read()
            .map_err(|e| MemoryStorageError::LockError(e.to_string()))?
            .clone())
    }
}
