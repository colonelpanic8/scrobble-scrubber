use chrono::{DateTime, Utc};
use lastfm_edit::{iterator::AsyncPaginatedIterator, LastFmEditClient, Result, ScrobbleEdit};
use log::{info, warn};

use crate::config::ScrobbleScrubberConfig;
use crate::persistence::{PendingEdit, PendingRewriteRule, StateStorage, TimestampState};
use crate::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct ScrobbleScrubber<S: StateStorage, P: ScrubActionProvider> {
    client: LastFmEditClient,
    storage: Arc<Mutex<S>>,
    action_provider: P,
    config: ScrobbleScrubberConfig,
    is_running: Arc<RwLock<bool>>,
    should_stop: Arc<RwLock<bool>>,
}

impl<S: StateStorage, P: ScrubActionProvider> ScrobbleScrubber<S, P> {
    pub fn new(
        storage: Arc<Mutex<S>>,
        client: LastFmEditClient,
        action_provider: P,
        config: ScrobbleScrubberConfig,
    ) -> Self {
        Self {
            client,
            storage,
            action_provider,
            config,
            is_running: Arc::new(RwLock::new(false)),
            should_stop: Arc::new(RwLock::new(false)),
        }
    }

    /// Get a reference to the storage for external access (e.g., web interface)
    pub fn storage(&self) -> Arc<Mutex<S>> {
        self.storage.clone()
    }

    /// Check if the scrubber is currently running a cycle
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Request the scrubber to stop gracefully
    pub async fn stop(&self) {
        *self.should_stop.write().await = true;
    }

    /// Trigger a single scrubbing run manually
    pub async fn trigger_run(&mut self) -> Result<()> {
        if *self.is_running.read().await {
            return Err(lastfm_edit::LastFmError::Io(std::io::Error::other(
                "Scrubber is already running",
            )));
        }

        self.check_and_process_tracks().await
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            // Check if we should stop
            if *self.should_stop.read().await {
                info!("Scrubber stop requested, exiting main loop");
                break;
            }

            *self.is_running.write().await = true;
            info!("Starting track monitoring cycle...");

            if let Err(e) = self.check_and_process_tracks().await {
                warn!("Error during track processing: {e}");
            }

            *self.is_running.write().await = false;

            info!("Sleeping for {} seconds...", self.config.scrubber.interval);

            // Sleep with periodic checks for stop signal
            let sleep_duration = std::time::Duration::from_secs(self.config.scrubber.interval);
            let check_interval = std::time::Duration::from_secs(1);
            let mut elapsed = std::time::Duration::ZERO;

            while elapsed < sleep_duration {
                if *self.should_stop.read().await {
                    info!("Scrubber stop requested during sleep, exiting");
                    return Ok(());
                }

                let remaining = sleep_duration - elapsed;
                let sleep_time = std::cmp::min(check_interval, remaining);
                tokio::time::sleep(sleep_time).await;
                elapsed += sleep_time;
            }
        }
        Ok(())
    }

    async fn check_and_process_tracks(&mut self) -> Result<()> {
        // Load current timestamp state to know where to start reading
        let timestamp_state = self
            .storage
            .lock()
            .await
            .load_timestamp_state()
            .await
            .map_err(|e| {
                lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                    "Failed to load timestamp state: {e}"
                )))
            })?;

        let mut recent_iterator = self.client.recent_tracks();

        let mut processed = 0;
        let mut latest_processed_timestamp: Option<DateTime<Utc>> = None;
        let mut found_anchor = timestamp_state.last_processed_timestamp.is_none();

        // Collect tracks first to avoid borrow checker issues
        let mut tracks_to_process = Vec::new();
        while let Some(track) = recent_iterator.next().await? {
            if processed >= self.config.scrubber.max_tracks {
                return Err(lastfm_edit::LastFmError::Io(std::io::Error::other(
                    format!(
                        "Reached maximum track limit ({}), unable to proceed",
                        self.config.scrubber.max_tracks
                    ),
                )));
            }

            // Check if this track matches our last processed timestamp (our anchor point)
            if !found_anchor {
                if let (Some(track_ts), Some(last_processed)) =
                    (track.timestamp, timestamp_state.last_processed_timestamp)
                {
                    let track_time = DateTime::from_timestamp(track_ts as i64, 0);
                    if let Some(track_time) = track_time {
                        if track_time == last_processed {
                            info!("Found anchor track at timestamp {last_processed}, starting processing from next track");
                            found_anchor = true;
                            continue; // Skip this track since we've already processed it
                        } else if track_time < last_processed {
                            info!(
                                "Reached track older than last processed ({last_processed}), stopping"
                            );
                            break;
                        }
                    }
                }
                // If we haven't found our anchor yet, continue looking but don't process
                continue;
            }

            // Track the timestamp of this track since we're processing it
            if let Some(ts) = track.timestamp {
                let track_time = DateTime::from_timestamp(ts as i64, 0).unwrap_or_else(Utc::now);
                if latest_processed_timestamp.is_none()
                    || latest_processed_timestamp.unwrap() < track_time
                {
                    latest_processed_timestamp = Some(track_time);
                }
            }

            tracks_to_process.push(track);
            processed += 1;
        }

        // Process collected tracks in batch
        if !tracks_to_process.is_empty() {
            let batch_suggestions = self.analyze_tracks(&tracks_to_process).await;

            for (track_index, suggestions) in batch_suggestions {
                if track_index >= tracks_to_process.len() {
                    log::warn!(
                        "Invalid track index {} for batch size {}",
                        track_index,
                        tracks_to_process.len()
                    );
                    continue;
                }

                let track = &tracks_to_process[track_index];
                info!(
                    "Processing track: {} - {} ({} suggestions)",
                    track.artist,
                    track.name,
                    suggestions.len()
                );

                for suggestion in suggestions {
                    if self.config.scrubber.dry_run {
                        info!("DRY RUN: Would apply suggestion: {suggestion:?}");
                    } else {
                        self.apply_suggestion(track, &suggestion).await?;
                    }
                }
            }
        }

        // Update timestamp with the latest scrobble timestamp we actually processed
        if let Some(latest) = latest_processed_timestamp {
            let updated_state = TimestampState {
                last_processed_timestamp: Some(latest),
            };

            self.storage
                .lock()
                .await
                .save_timestamp_state(&updated_state)
                .await
                .map_err(|e| {
                    lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                        "Failed to save timestamp state: {e}"
                    )))
                })?;

            info!("Updated last processed timestamp to: {latest}");
        }

        info!("Processed {processed} tracks");
        Ok(())
    }

    /// Process all tracks for a specific artist
    pub async fn process_artist(&mut self, artist: &str) -> Result<()> {
        info!("Starting artist track processing for: {artist}");

        let mut artist_iterator = self.client.artist_tracks(artist);
        let mut processed = 0;

        // Collect tracks first to avoid borrow checker issues
        let mut tracks_to_process = Vec::new();
        while let Some(track) = artist_iterator.next().await? {
            tracks_to_process.push(track);
            processed += 1;
        }

        info!(
            "Found {} tracks for artist '{}'",
            tracks_to_process.len(),
            artist
        );

        // Process collected tracks in batch
        if !tracks_to_process.is_empty() {
            let batch_suggestions = self.analyze_tracks(&tracks_to_process).await;

            for (track_index, suggestions) in batch_suggestions {
                if track_index >= tracks_to_process.len() {
                    log::warn!(
                        "Invalid track index {} for batch size {}",
                        track_index,
                        tracks_to_process.len()
                    );
                    continue;
                }

                let track = &tracks_to_process[track_index];
                info!(
                    "Processing track: {} - {} ({} suggestions)",
                    track.artist,
                    track.name,
                    suggestions.len()
                );

                for suggestion in suggestions {
                    if self.config.scrubber.dry_run {
                        info!("DRY RUN: Would apply suggestion: {suggestion:?}");
                    } else {
                        self.apply_suggestion(track, &suggestion).await?;
                    }
                }
            }
        }

        info!("Processed {processed} tracks for artist '{artist}'");
        Ok(())
    }

    async fn analyze_tracks(
        &self,
        tracks: &[lastfm_edit::Track],
    ) -> Vec<(usize, Vec<ScrubActionSuggestion>)> {
        match self.action_provider.analyze_tracks(tracks).await {
            Ok(suggestions) => {
                for (track_idx, track_suggestions) in &suggestions {
                    if let Some(track) = tracks.get(*track_idx) {
                        info!(
                            "Action provider '{}' suggested {} actions for track '{} - {}'",
                            self.action_provider.provider_name(),
                            track_suggestions.len(),
                            track.artist,
                            track.name
                        );
                    }
                }
                suggestions
            }
            Err(e) => {
                warn!("Error from action provider: {e}");
                Vec::new()
            }
        }
    }

    async fn apply_suggestion(
        &mut self,
        track: &lastfm_edit::Track,
        suggestion: &ScrubActionSuggestion,
    ) -> Result<()> {
        // Load settings to check global confirmation requirement
        let settings_state = self
            .storage
            .lock()
            .await
            .load_settings_state()
            .await
            .map_err(|e| {
                lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                    "Failed to load settings state: {e}"
                )))
            })?;

        match suggestion {
            ScrubActionSuggestion::Edit(edit) => {
                // Check if global settings require confirmation
                if settings_state.require_confirmation || self.config.scrubber.require_confirmation
                {
                    self.create_pending_edit(track, edit).await?;
                } else {
                    self.apply_edit(track, edit).await?;
                }
            }
            ScrubActionSuggestion::ProposeRule { rule, motivation } => {
                info!(
                    "Provider proposed new rule for track '{}' by '{}': {}",
                    track.name, track.artist, motivation
                );
                self.handle_proposed_rule(track, rule, motivation).await?;
            }
            ScrubActionSuggestion::NoAction => {
                // This shouldn't happen since we filter NoAction in analyze_track
                info!("Provider suggested no action needed");
            }
        }
        Ok(())
    }

    async fn create_pending_edit(
        &self,
        track: &lastfm_edit::Track,
        edit: &ScrobbleEdit,
    ) -> Result<()> {
        let new_track_name = if edit.track_name == edit.track_name_original {
            None
        } else {
            Some(edit.track_name.clone())
        };

        let new_artist_name = if edit.artist_name == edit.artist_name_original {
            None
        } else {
            Some(edit.artist_name.clone())
        };

        let new_album_name = if edit.album_name == edit.album_name_original {
            None
        } else {
            Some(edit.album_name.clone())
        };

        let new_album_artist_name = if edit.album_artist_name == edit.album_artist_name_original {
            None
        } else {
            Some(edit.album_artist_name.clone())
        };

        let pending_edit = PendingEdit::new(
            track.name.clone(),
            track.artist.clone(),
            Some(edit.album_name_original.clone()),
            Some(edit.album_artist_name_original.clone()),
            new_track_name,
            new_artist_name,
            new_album_name,
            new_album_artist_name,
            track.timestamp,
        );

        // Load and save pending edits
        let mut pending_edits_state = self
            .storage
            .lock()
            .await
            .load_pending_edits_state()
            .await
            .map_err(|e| {
                lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                    "Failed to load pending edits: {e}"
                )))
            })?;

        pending_edits_state.pending_edits.push(pending_edit.clone());

        self.storage
            .lock()
            .await
            .save_pending_edits_state(&pending_edits_state)
            .await
            .map_err(|e| {
                lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                    "Failed to save pending edit: {e}"
                )))
            })?;

        info!(
            "Created pending edit requiring confirmation (ID: {})",
            pending_edit.id
        );
        Ok(())
    }

    async fn apply_edit(&mut self, track: &lastfm_edit::Track, edit: &ScrobbleEdit) -> Result<()> {
        // Check if track name changed
        if edit.track_name != edit.track_name_original {
            info!(
                "Renaming track '{}' to '{}'",
                edit.track_name_original, edit.track_name
            );
            // TODO: Implement track name editing in lastfm-edit library
            warn!(
                "Track renaming not yet implemented: '{}' -> '{}'",
                edit.track_name_original, edit.track_name
            );
        }

        // Check if artist name changed
        if edit.artist_name != edit.artist_name_original {
            info!(
                "Renaming artist '{}' to '{}' for track '{}'",
                edit.artist_name_original, edit.artist_name, track.name
            );
            self.client
                .edit_artist_for_track(&track.name, &track.artist, &edit.artist_name)
                .await?;
        }

        // TODO: Handle album and album_artist changes when implemented
        if edit.album_name != edit.album_name_original {
            info!("Album name change detected but not yet implemented");
        }
        if edit.album_artist_name != edit.album_artist_name_original {
            info!("Album artist name change detected but not yet implemented");
        }

        Ok(())
    }

    async fn handle_proposed_rule(
        &self,
        track: &lastfm_edit::Track,
        rule: &crate::rewrite::RewriteRule,
        motivation: &str,
    ) -> Result<()> {
        // Check if confirmation is required for proposed rules
        if self.config.scrubber.require_proposed_rule_confirmation {
            // Create a pending rewrite rule for approval
            let pending_rule = PendingRewriteRule::new(
                rule.clone(),
                motivation.to_string(),
                track.name.clone(),
                track.artist.clone(),
            );

            // Load and save pending rewrite rules
            let mut pending_rules_state = self
                .storage
                .lock()
                .await
                .load_pending_rewrite_rules_state()
                .await
                .map_err(|e| {
                    lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                        "Failed to load pending rewrite rules: {e}"
                    )))
                })?;

            pending_rules_state.pending_rules.push(pending_rule.clone());

            self.storage
                .lock()
                .await
                .save_pending_rewrite_rules_state(&pending_rules_state)
                .await
                .map_err(|e| {
                    lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                        "Failed to save pending rewrite rule: {e}"
                    )))
                })?;

            info!(
                "Created pending rewrite rule requiring approval (ID: {})",
                pending_rule.id
            );
        } else {
            // Auto-approve the rule and add it to active rewrite rules
            let mut rules_state = self
                .storage
                .lock()
                .await
                .load_rewrite_rules_state()
                .await
                .map_err(|e| {
                    lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                        "Failed to load rewrite rules: {e}"
                    )))
                })?;

            rules_state.rewrite_rules.push(rule.clone());

            self.storage
                .lock()
                .await
                .save_rewrite_rules_state(&rules_state)
                .await
                .map_err(|e| {
                    lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                        "Failed to save rewrite rules: {e}"
                    )))
                })?;

            info!("Auto-approved and added new rewrite rule: {motivation}");
        }
        Ok(())
    }
}
