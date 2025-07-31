use chrono::{DateTime, Utc};
use lastfm_edit::{LastFmEditClient, Result, ScrobbleEdit};
use uuid::Uuid;

use crate::config::ScrobbleScrubberConfig;
use crate::events::ScrubberEvent;
use crate::events::{LogEditInfo, ProcessingContext, ProcessingType};
use crate::persistence::{PendingEdit, PendingRewriteRule, StateStorage, TimestampState};
use crate::scrub_action_provider::{
    ScrubActionProvider, ScrubActionSuggestion, SuggestionWithContext,
};
use crate::track_provider::{CachedTrackProvider, DirectTrackProvider, TrackProvider};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, Notify, RwLock};

pub struct ScrobbleScrubber<S: StateStorage, P: ScrubActionProvider> {
    client: Box<dyn LastFmEditClient + Send + Sync>,
    storage: Arc<Mutex<S>>,
    action_provider: P,
    config: ScrobbleScrubberConfig,
    is_running: Arc<RwLock<bool>>,
    should_stop: Arc<Notify>,
    event_sender: broadcast::Sender<ScrubberEvent>,
    trigger_immediate: Arc<Notify>,
    track_provider: TrackProvider,
}

impl<S: StateStorage, P: ScrubActionProvider> ScrobbleScrubber<S, P> {
    pub fn new(
        storage: Arc<Mutex<S>>,
        client: Box<dyn LastFmEditClient + Send + Sync>,
        action_provider: P,
        config: ScrobbleScrubberConfig,
    ) -> Self {
        Self::with_track_provider(
            storage,
            client,
            action_provider,
            config,
            TrackProvider::Cached(CachedTrackProvider::new()),
        )
    }

    /// Create a new scrubber with a custom track provider
    pub fn with_track_provider(
        storage: Arc<Mutex<S>>,
        client: Box<dyn LastFmEditClient + Send + Sync>,
        action_provider: P,
        config: ScrobbleScrubberConfig,
        track_provider: TrackProvider,
    ) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        let mut scrubber = Self {
            client,
            storage,
            action_provider,
            config,
            is_running: Arc::new(RwLock::new(false)),
            should_stop: Arc::new(Notify::new()),
            event_sender,
            trigger_immediate: Arc::new(Notify::new()),
            track_provider,
        };
        scrubber.setup_client_event_forwarding();
        scrubber
    }

    /// Create a new scrubber with cached track provider (default behavior)
    pub fn with_cached_provider(
        storage: Arc<Mutex<S>>,
        client: Box<dyn LastFmEditClient + Send + Sync>,
        action_provider: P,
        config: ScrobbleScrubberConfig,
    ) -> Self {
        Self::with_track_provider(
            storage,
            client,
            action_provider,
            config,
            TrackProvider::Cached(CachedTrackProvider::new()),
        )
    }

    /// Create a new scrubber with direct track provider (no caching)
    pub fn with_direct_provider(
        storage: Arc<Mutex<S>>,
        client: Box<dyn LastFmEditClient + Send + Sync>,
        action_provider: P,
        config: ScrobbleScrubberConfig,
    ) -> Self {
        Self::with_track_provider(
            storage,
            client,
            action_provider,
            config,
            TrackProvider::Direct(DirectTrackProvider::new()),
        )
    }

    /// Set up forwarding of client events to scrubber events
    fn setup_client_event_forwarding(&mut self) {
        let mut client_event_receiver = self.client.subscribe();
        let event_sender = self.event_sender.clone();

        tokio::spawn(async move {
            while let Ok(client_event) = client_event_receiver.recv().await {
                let scrubber_event = ScrubberEvent::client_event(client_event);
                let _ = event_sender.send(scrubber_event);
            }
        });
    }

    /// Trigger immediate processing, bypassing the normal wait interval
    pub fn trigger_immediate_processing(&self) {
        self.trigger_immediate.notify_one();
    }

    /// Subscribe to scrubber events
    pub fn subscribe_events(&self) -> broadcast::Receiver<ScrubberEvent> {
        self.event_sender.subscribe()
    }

    /// Emit an event to all subscribers
    fn emit_event(&self, event: ScrubberEvent) {
        // Log the event but don't fail if no receivers
        let _ = self.event_sender.send(event);
    }

    /// Get a reference to the storage for external access (e.g., web interface)
    pub fn storage(&self) -> Arc<Mutex<S>> {
        self.storage.clone()
    }

    /// Get a reference to the client for direct access to Last.fm API methods
    /// This allows external code to use client methods like iterators without
    /// going through the scrubber's wrapper methods
    pub fn client(&self) -> &(dyn lastfm_edit::LastFmEditClient + Send + Sync) {
        self.client.as_ref()
    }

    /// Get access to the underlying cache if using CachedTrackProvider
    /// Returns None if using a different track provider implementation
    pub fn cache(&self) -> Option<&crate::track_cache::TrackCache> {
        self.track_provider.cache()
    }

    /// Get mutable access to the underlying cache if using CachedTrackProvider
    /// Returns None if using a different track provider implementation
    pub fn cache_mut(&mut self) -> Option<&mut crate::track_cache::TrackCache> {
        self.track_provider.cache_mut()
    }

    /// Check if the scrubber is currently running a cycle
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Request the scrubber to stop gracefully
    pub fn stop(&self) {
        self.should_stop.notify_one();
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

    /// Ensure timestamp state is initialized with the most recent track if not set
    async fn ensure_timestamp_initialized(
        &mut self,
        timestamp_state: TimestampState,
    ) -> Result<TimestampState> {
        if timestamp_state.last_processed_timestamp.is_some() {
            return Ok(timestamp_state);
        }

        log::info!("No timestamp anchor found, initializing with most recent track...");

        let mut recent_iterator = self.client.recent_tracks();

        // Get the first (most recent) track to use as our anchor
        if let Some(first_track) = recent_iterator.next().await? {
            log::info!(
                "Most recent track found: '{}' by '{}' (playcount: {})",
                first_track.name,
                first_track.artist,
                first_track.playcount
            );

            if let Some(ts) = first_track.timestamp {
                let track_time = DateTime::from_timestamp(ts as i64, 0).unwrap_or_else(Utc::now);
                let new_state = TimestampState {
                    last_processed_timestamp: Some(track_time),
                };

                // Save the new timestamp state
                self.storage
                    .lock()
                    .await
                    .save_timestamp_state(&new_state)
                    .await
                    .map_err(|e| {
                        lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                            "Failed to save initial timestamp state: {e}"
                        )))
                    })?;

                log::info!(
                    "Initialized timestamp anchor at: {} for track '{}' by '{}'",
                    track_time,
                    first_track.name,
                    first_track.artist
                );
                return Ok(new_state);
            } else {
                log::info!(
                    "Most recent track '{}' by '{}' has no timestamp, using original state",
                    first_track.name,
                    first_track.artist
                );
            }
        } else {
            log::info!("No recent tracks found, using original state");
        }

        Ok(timestamp_state)
    }

    pub async fn run(&mut self) -> Result<()> {
        self.emit_event(ScrubberEvent::started("Scrubber started".to_string()));

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.scrubber.interval,
        ));
        interval.tick().await; // Skip the first immediate tick

        loop {
            tokio::select! {
                // Regular interval tick
                _ = interval.tick() => {
                    log::info!("Starting scheduled track monitoring cycle...");
                    self.run_processing_cycle().await;
                }

                // Immediate processing triggered
                _ = self.trigger_immediate.notified() => {
                    log::info!("Immediate processing triggered");
                    self.emit_event(ScrubberEvent::info(
                        "Immediate processing triggered".to_string(),
                    ));
                    self.run_processing_cycle().await;
                }

                // Stop signal received
                _ = self.should_stop.notified() => {
                    log::info!("Scrubber stop requested, exiting main loop");
                    break;
                }
            }
        }

        self.emit_event(ScrubberEvent::stopped("Scrubber stopped".to_string()));
        Ok(())
    }

    /// Run a single processing cycle with proper state management
    async fn run_processing_cycle(&mut self) {
        *self.is_running.write().await = true;
        let _ = self.check_and_process_tracks().await; // Error handling is done inside the method
        *self.is_running.write().await = false;
    }

    async fn check_and_process_tracks(&mut self) -> Result<()> {
        log::info!("Starting track monitoring cycle...");
        self.emit_event(ScrubberEvent::cycle_started(
            "Starting track monitoring cycle".to_string(),
        ));

        let result = self.check_and_process_tracks_inner().await;

        if let Err(ref e) = result {
            log::warn!("Error during track processing: {e}");
            self.emit_event(ScrubberEvent::error_from_string(format!(
                "Error during track processing: {e}"
            )));
        }

        result
    }

    async fn check_and_process_tracks_inner(&mut self) -> Result<()> {
        // Step 1: Load current timestamp state to know where we left off
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

        // Step 2: Ensure we have an anchor timestamp before updating cache
        let timestamp_state = self.ensure_timestamp_initialized(timestamp_state).await?;

        // Step 3: Update cache with latest tracks from API, using anchor to limit fetch
        log::debug!("Updating track cache from Last.fm API...");

        // The anchor timestamp must be set at this point due to ensure_timestamp_initialized
        let anchor_timestamp = timestamp_state
            .last_processed_timestamp
            .expect("Anchor timestamp should be set after ensure_timestamp_initialized");

        self.track_provider
            .update_cache_from_api(self.client.as_ref(), Some(anchor_timestamp))
            .await?;

        // Step 3: Find tracks to process from cache using current anchor
        let tracks_to_process = self.find_tracks_to_process(&timestamp_state).await?;

        log::info!("Found {} tracks to process", tracks_to_process.len());

        // Step 4: Process all collected tracks (oldest first) and update anchor after processing
        if !tracks_to_process.is_empty() {
            self.process_tracks_and_update_anchor(&tracks_to_process)
                .await?;
        }

        log::info!(
            "Processing complete: processed {} tracks from cache",
            tracks_to_process.len()
        );

        self.emit_event(ScrubberEvent::cycle_completed(
            tracks_to_process.len(),
            0, // We'll update this when we track rule applications
        ));
        Ok(())
    }

    /// Process tracks and update anchor to the newest processed track
    async fn process_tracks_and_update_anchor(
        &mut self,
        tracks: &[lastfm_edit::Track],
    ) -> Result<()> {
        // Process tracks first
        self.process_tracks_individually_no_timestamp_update(tracks)
            .await?;

        // After processing, update anchor to the newest (last in chronological
        // order) processed track Since tracks are processed oldest first, the
        // newest processed track is the last one
        if let Some(newest_processed_track) = tracks.last() {
            if let Some(ts) = newest_processed_track.timestamp {
                let track_time = DateTime::from_timestamp(ts as i64, 0).unwrap_or_else(Utc::now);
                let updated_state = TimestampState {
                    last_processed_timestamp: Some(track_time),
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

                log::info!(
                    "Updated anchor to newest processed track: {} (track: '{}' by '{}')",
                    track_time,
                    newest_processed_track.name,
                    newest_processed_track.artist
                );

                // Emit anchor update event
                self.emit_event(ScrubberEvent::anchor_updated_from_names(
                    ts,
                    &newest_processed_track.name,
                    &newest_processed_track.artist,
                ));
            }
        }

        Ok(())
    }

    /// Find tracks to process from cache based on timestamp state. The anchor
    /// points to the last track we processed, so we process all tracks newer
    /// than the anchor
    async fn find_tracks_to_process(
        &self,
        timestamp_state: &TimestampState,
    ) -> Result<Vec<lastfm_edit::Track>> {
        let mut tracks_to_process = Vec::new();

        // Get all recent tracks from provider, sorted newest first
        let cached_tracks = self.track_provider.get_all_recent_tracks();

        // Debug: Show anchor timestamp
        if let Some(anchor) = timestamp_state.last_processed_timestamp {
            log::trace!("Using anchor timestamp: {anchor}");
        } else {
            log::trace!("No anchor timestamp set (first run)");
        }

        for cached_track in cached_tracks {
            // Check if we've reached our last processed track (anchor point)
            if let Some(last_processed) = timestamp_state.last_processed_timestamp {
                if let Some(track_ts) = cached_track.timestamp {
                    let track_time = DateTime::from_timestamp(track_ts as i64, 0);
                    if let Some(track_time) = track_time {
                        log::trace!(
                            "Examining cached track '{}' by '{}' at {} vs anchor at {}",
                            cached_track.name,
                            cached_track.artist,
                            track_time,
                            last_processed
                        );

                        if track_time <= last_processed {
                            log::info!("Reached previously processed track {} at {}, found {} new tracks to process",
                                  cached_track, track_time, tracks_to_process.len());
                            break; // Stop here - we've caught up to where we left off
                        }
                    }
                } else {
                    log::warn!("Cached track {cached_track} has no timestamp");
                }
            } else {
                // First run - no anchor timestamp, collect tracks up to limit
                log::info!("First run - found cached track: {cached_track}");
            }

            // Add track directly since cached_track is already a Track
            tracks_to_process.push(cached_track);
        }

        // Reverse to process oldest first (tracks were collected newest first)
        tracks_to_process.reverse();

        // Emit TracksFound event with count and anchor timestamp
        let anchor_timestamp = timestamp_state
            .last_processed_timestamp
            .map(|ts| ts.timestamp() as u64)
            .unwrap_or(0);

        let tracks_found_event =
            ScrubberEvent::tracks_found(tracks_to_process.len(), anchor_timestamp);
        let _ = self.event_sender.send(tracks_found_event);

        Ok(tracks_to_process)
    }

    /// Process the last N tracks without updating timestamp state
    pub async fn process_last_n_tracks(&mut self, n: u32) -> Result<()> {
        log::info!("Processing last {n} tracks (no timestamp updates)");

        // Emit cycle started event for UI progress
        self.emit_event(ScrubberEvent::cycle_started(format!(
            "Processing last {n} tracks"
        )));

        let mut recent_iterator = self.client.recent_tracks();
        let mut tracks_to_process = Vec::new();
        let mut examined = 0;

        // Collect the last n tracks
        while let Some(track) = recent_iterator.next().await? {
            examined += 1;
            tracks_to_process.push(track);

            if tracks_to_process.len() >= n as usize {
                log::info!("Collected {n} tracks for processing");
                break;
            }
        }

        if tracks_to_process.is_empty() {
            log::info!("No tracks found to process");
            return Ok(());
        }

        // Process using shared helper
        self.process_collected_tracks(
            &tracks_to_process,
            ProcessingType::Track,
            "Processing complete",
        )
        .await?;

        // Emit cycle completed event for UI progress
        self.emit_event(ScrubberEvent::cycle_completed(
            tracks_to_process.len(),
            0, // TODO: track applied count in future enhancement
        ));

        log::info!(
            "Processing complete: examined {} tracks, processed {} tracks",
            examined,
            tracks_to_process.len()
        );
        Ok(())
    }

    /// Collect items from an iterator with optional limit
    async fn collect_from_iterator<T>(
        &mut self,
        iterator: &mut Box<dyn lastfm_edit::AsyncPaginatedIterator<T>>,
        limit: Option<u32>,
    ) -> Result<Vec<T>> {
        let mut items = Vec::new();
        let mut collected = 0;

        while let Some(item) = iterator.next().await? {
            items.push(item);
            collected += 1;

            if let Some(limit_val) = limit {
                if collected >= limit_val {
                    break;
                }
            }
        }

        Ok(items)
    }

    /// Collect all tracks from a list of albums
    async fn collect_tracks_from_albums(
        &mut self,
        albums: &[lastfm_edit::Album],
    ) -> Result<Vec<lastfm_edit::Track>> {
        let mut all_tracks = Vec::new();

        for album in albums {
            log::debug!(
                "Loading tracks for album: {} - {}",
                album.artist,
                album.name
            );

            // Get tracks for this specific album
            match self
                .client
                .get_album_tracks(&album.artist, &album.name)
                .await
            {
                Ok(tracks) => {
                    log::debug!(
                        "Found {} tracks in album '{}' by '{}'",
                        tracks.len(),
                        album.name,
                        album.artist
                    );
                    all_tracks.extend(tracks);
                }
                Err(e) => {
                    log::warn!(
                        "Failed to load tracks for album '{}' by '{}': {e}",
                        album.name,
                        album.artist
                    );
                    // Continue with other albums even if one fails
                }
            }
        }

        Ok(all_tracks)
    }

    /// Shared helper to process collected tracks with optional logging
    async fn process_collected_tracks(
        &mut self,
        tracks: &[lastfm_edit::Track],
        processing_type: ProcessingType,
        completion_message: &str,
    ) -> Result<()> {
        if tracks.is_empty() {
            return Ok(());
        }

        self.process_tracks_individually_no_timestamp_update_with_context(tracks, processing_type)
            .await?;

        log::debug!("{}: processed {} tracks", completion_message, tracks.len());
        Ok(())
    }

    /// Process tracks individually without timestamp updates
    async fn process_tracks_individually_no_timestamp_update(
        &mut self,
        tracks: &[lastfm_edit::Track],
    ) -> Result<()> {
        self.process_tracks_individually_no_timestamp_update_with_context(
            tracks,
            ProcessingType::Track,
        )
        .await
    }

    /// Process tracks individually without timestamp updates, with processing type context
    async fn process_tracks_individually_no_timestamp_update_with_context(
        &mut self,
        tracks: &[lastfm_edit::Track],
        processing_type: ProcessingType,
    ) -> Result<()> {
        // Emit batch started event for progress UI
        if !tracks.is_empty() {
            self.emit_event(ScrubberEvent::processing_batch_started(
                tracks.to_vec(),
                processing_type,
            ));
        }

        for (track_index, track) in tracks.iter().enumerate() {
            log::debug!(
                "Processing track {} of {}: {} - {}",
                track_index + 1,
                tracks.len(),
                track.artist,
                track.name
            );

            // Process this track individually without timestamp updates
            self.process_single_track_with_context(
                track,
                track_index,
                tracks.len(),
                processing_type,
            )
            .await?;

            // Yield control to allow other async tasks (like UI updates) to run
            tokio::task::yield_now().await;
        }

        Ok(())
    }

    /// Process a single track with its suggestions and artist processing context
    ///
    /// **IMPORTANT**: This is the ONLY function where rules are applied to tracks.
    /// All track processing entry points (run, process_last_n_tracks, process_artist,
    /// process_album, process_search, process_search_albums) must ultimately flow
    /// through this function to ensure consistent rule application and logging.
    async fn process_single_track_with_context(
        &mut self,
        track: &lastfm_edit::Track,
        track_index: usize,
        total_tracks: usize,
        processing_type: ProcessingType,
    ) -> Result<()> {
        // Emit track processing started event for progress UI
        self.emit_event(ScrubberEvent::track_processing_started(
            track.clone(),
            track_index,
            total_tracks,
        ));
        log::trace!(
            "Starting analysis for track: {} - {}",
            track.artist,
            track.name
        );

        // Analyze this single track
        let track_slice = &[track.clone()];
        let track_suggestions = self.analyze_tracks(track_slice).await;
        let run_id = Uuid::new_v4().to_string();

        // Find suggestions for this track (should be at index 0 since we only passed one track)
        let empty_suggestions = vec![];
        let suggestions = track_suggestions
            .iter()
            .find(|(index, _)| *index == 0)
            .map(|(_, suggestions)| suggestions)
            .unwrap_or(&empty_suggestions);

        // Collect suggestions for event emission (will emit after processing is complete)

        log::debug!("Processed track: {track}");

        // Apply suggestions using the helper method
        self.apply_suggestions_to_track(track, suggestions, run_id, track_index, processing_type)
            .await?;

        // Always process suggestions and log the result
        let mut summary_parts = Vec::new();
        let mut pending_count = 0;
        let mut applied_count = 0;
        let mut has_rule_proposal = false;
        let mut _has_changes = false;
        let mut edit_details = Vec::new();

        for suggestion in suggestions {
            match &suggestion.suggestion {
                crate::scrub_action_provider::ScrubActionSuggestion::Edit(edit) => {
                    if Self::has_changes(edit) {
                        _has_changes = true;

                        // Use ScrobbleEdit's Display implementation for logging
                        edit_details.push(edit.to_string());

                        if suggestion.requires_confirmation
                            || self
                                .storage
                                .lock()
                                .await
                                .load_settings_state()
                                .await
                                .map(|s| s.require_confirmation || s.require_confirmation_for_edits)
                                .unwrap_or(false)
                            || self.config.scrubber.require_confirmation
                        {
                            pending_count += 1;
                        } else {
                            applied_count += 1;
                        }
                    }
                }
                crate::scrub_action_provider::ScrubActionSuggestion::ProposeRule { .. } => {
                    has_rule_proposal = true;
                }
                crate::scrub_action_provider::ScrubActionSuggestion::NoAction => {}
            }
        }

        if applied_count > 0 {
            summary_parts.push(format!(
                "{} edit{} applied",
                applied_count,
                if applied_count == 1 { "" } else { "s" }
            ));
        }
        if pending_count > 0 {
            summary_parts.push(format!(
                "{} edit{} pending confirmation",
                pending_count,
                if pending_count == 1 { "" } else { "s" }
            ));
        }

        if has_rule_proposal {
            summary_parts.push("proposed rule".to_string());
        }

        let summary = if summary_parts.is_empty() {
            "no changes".to_string()
        } else {
            summary_parts.join(", ")
        };

        // Always log the processing result
        let edit_info = if edit_details.is_empty() {
            String::new()
        } else {
            format!(" ({})", edit_details.join("; "))
        };

        log::info!(
            "Processed [{}]: {} - {}{}",
            track_index + 1,
            track,
            summary,
            edit_info,
        );

        // Generate processing result for progress UI
        let processing_result = if !suggestions.is_empty() {
            let mut pending_count = 0;
            let mut applied_count = 0;
            let mut has_rule_proposal = false;
            let mut _has_changes = false;

            for suggestion in suggestions {
                match &suggestion.suggestion {
                    crate::scrub_action_provider::ScrubActionSuggestion::Edit(edit) => {
                        if Self::has_changes(edit) {
                            _has_changes = true;
                            if suggestion.requires_confirmation
                                || self
                                    .storage
                                    .lock()
                                    .await
                                    .load_settings_state()
                                    .await
                                    .map(|s| {
                                        s.require_confirmation || s.require_confirmation_for_edits
                                    })
                                    .unwrap_or(false)
                                || self.config.scrubber.require_confirmation
                            {
                                pending_count += 1;
                            } else {
                                applied_count += 1;
                            }
                        }
                    }
                    crate::scrub_action_provider::ScrubActionSuggestion::ProposeRule { .. } => {
                        has_rule_proposal = true;
                    }
                    crate::scrub_action_provider::ScrubActionSuggestion::NoAction => {}
                }
            }

            use crate::events::ProcessingResult;
            match (applied_count, pending_count, has_rule_proposal) {
                (0, 0, false) => ProcessingResult::NoChanges,
                (applied, 0, false) if applied > 0 => ProcessingResult::EditsApplied(applied),
                (0, pending, false) if pending > 0 => ProcessingResult::EditsPending(pending),
                (0, 0, true) => ProcessingResult::RuleProposed,
                (applied, 0, true) if applied > 0 => {
                    ProcessingResult::EditsAppliedAndRuleProposed(applied)
                }
                (0, pending, true) if pending > 0 => {
                    ProcessingResult::EditsPendingAndRuleProposed(pending)
                }
                _ => ProcessingResult::NoChanges, // fallback for unexpected combinations
            }
        } else {
            use crate::events::ProcessingResult;
            ProcessingResult::NoChanges
        };

        // Emit detailed track processed event
        let suggestions_for_event: Vec<ScrubActionSuggestion> =
            suggestions.iter().map(|s| s.suggestion.clone()).collect();
        self.emit_event(ScrubberEvent::track_processed(
            track.clone(),
            suggestions_for_event,
            processing_result.clone(),
        ));

        // Emit track processing completed event for progress UI
        self.emit_event(ScrubberEvent::track_processing_completed(
            track.clone(),
            track_index,
            total_tracks,
            true, // success - if we got here, processing succeeded
            processing_result,
        ));

        Ok(())
    }

    /// Apply suggestions to a track with proper context and event emission
    async fn apply_suggestions_to_track(
        &mut self,
        track: &lastfm_edit::Track,
        suggestions: &[SuggestionWithContext],
        run_id: String,
        track_index: usize,
        processing_type: ProcessingType,
    ) -> Result<()> {
        if suggestions.is_empty() {
            return Ok(());
        }

        log::trace!(
            "Applying {} suggestions to track: {} - {}",
            suggestions.len(),
            track.artist,
            track.name
        );

        for (i, suggestion) in suggestions.iter().enumerate() {
            log::trace!(
                "Applying suggestion {}/{} for track '{}' by '{}': {:?}",
                i + 1,
                suggestions.len(),
                track.name,
                track.artist,
                suggestion
            );

            let suggestion_context = ProcessingContext {
                run_id: run_id.clone(),
                batch_id: None, // No batch processing anymore
                track_index: Some(track_index),
                batch_size: Some(1), // Always 1 since we process individually
                is_artist_processing: processing_type == ProcessingType::Artist,
            };
            self.apply_suggestion_with_context(track, suggestion, Some(suggestion_context))
                .await?;

            // Emit rule applied event based on suggestion type
            let description = match &suggestion.suggestion {
                crate::scrub_action_provider::ScrubActionSuggestion::Edit(edit) => {
                    log::trace!("Applied edit: {edit:?}");
                    format!("Applied edit from {}", suggestion.provider_name)
                }
                crate::scrub_action_provider::ScrubActionSuggestion::ProposeRule {
                    rule,
                    motivation,
                } => {
                    log::trace!("Proposed rule: {rule:?} with motivation: {motivation}");
                    format!(
                        "Proposed rule from {}: {motivation}",
                        suggestion.provider_name
                    )
                }
                crate::scrub_action_provider::ScrubActionSuggestion::NoAction => {
                    log::trace!("No action taken for track");
                    format!("No action taken by {}", suggestion.provider_name)
                }
            };
            self.emit_event(ScrubberEvent::rule_applied(
                track.clone(),
                suggestion.suggestion.clone(),
                description,
            ));
        }

        Ok(())
    }

    /// Process all tracks for a specific artist
    pub async fn process_artist(&mut self, artist: &str) -> Result<()> {
        log::info!("Starting artist track processing for: {artist}");

        // Emit cycle started event for UI progress
        self.emit_event(ScrubberEvent::cycle_started(format!(
            "Starting artist processing for: {artist}"
        )));

        // Collect all tracks for the artist using shared helper
        let mut artist_iterator = self.client.artist_tracks(artist);
        let tracks_to_process = self
            .collect_from_iterator(&mut artist_iterator, None)
            .await?;

        log::info!(
            "Found {} tracks for artist '{}'",
            tracks_to_process.len(),
            artist
        );

        // Process using shared helper with artist context
        self.process_collected_tracks(
            &tracks_to_process,
            ProcessingType::Artist,
            "Artist processing complete",
        )
        .await?;

        // Emit cycle completed event for UI progress
        self.emit_event(ScrubberEvent::cycle_completed(
            tracks_to_process.len(),
            0, // TODO: track applied count in future enhancement
        ));

        log::info!(
            "Processed {} tracks for artist '{artist}'",
            tracks_to_process.len()
        );
        Ok(())
    }

    /// Process all tracks for a specific album by a specific artist
    pub async fn process_album(&mut self, artist: &str, album: &str) -> Result<()> {
        log::info!("Starting album track processing for: '{album}' by '{artist}'");

        // Emit cycle started event for UI progress
        self.emit_event(ScrubberEvent::cycle_started(format!(
            "Starting album processing for: '{album}' by '{artist}'"
        )));

        // Get tracks for the specific album
        let tracks_to_process = self.client.get_album_tracks(album, artist).await?;

        log::info!(
            "Found {} tracks for album '{album}' by '{artist}'",
            tracks_to_process.len()
        );

        // Process using shared helper with artist context
        self.process_collected_tracks(
            &tracks_to_process,
            ProcessingType::Album,
            "Album processing complete",
        )
        .await?;

        // Emit cycle completed event for UI progress
        self.emit_event(ScrubberEvent::cycle_completed(
            tracks_to_process.len(),
            0, // TODO: track applied count in future enhancement
        ));

        log::info!(
            "Processed {} tracks for album '{album}' by '{artist}'",
            tracks_to_process.len()
        );
        Ok(())
    }

    /// Process tracks matching a search query
    pub async fn process_search(&mut self, query: &str, limit: u32) -> Result<()> {
        self.process_search_with_limit(query, Some(limit)).await
    }

    /// Process tracks matching a search query with optional limit
    pub async fn process_search_with_limit(
        &mut self,
        query: &str,
        limit: Option<u32>,
    ) -> Result<()> {
        let limit_info = match limit {
            Some(l) => format!("limit: {l} tracks"),
            None => "no limit".to_string(),
        };
        log::info!("Starting search-based track processing for query: '{query}' ({limit_info})");

        // Emit cycle started event for UI progress
        self.emit_event(ScrubberEvent::cycle_started(format!(
            "Starting search processing for query: '{query}' ({limit_info})"
        )));

        // Collect tracks from search iterator with optional limit
        let mut search_iterator = self.client.search_tracks(query);
        let tracks_to_process = self
            .collect_from_iterator(&mut search_iterator, limit)
            .await?;

        log::info!(
            "Found {} tracks matching search query '{query}'",
            tracks_to_process.len()
        );

        if tracks_to_process.is_empty() {
            log::warn!(
                "No tracks found matching search query '{query}'. Try a different search term."
            );
            return Ok(());
        }

        // Process using shared helper without artist context
        self.process_collected_tracks(
            &tracks_to_process,
            ProcessingType::Search,
            "Search processing complete",
        )
        .await?;

        // Emit cycle completed event for UI progress
        self.emit_event(ScrubberEvent::cycle_completed(
            tracks_to_process.len(),
            0, // TODO: track applied count in future enhancement
        ));

        log::info!(
            "Search processing complete: processed {} tracks matching query '{query}'",
            tracks_to_process.len()
        );

        Ok(())
    }

    /// Process tracks from albums matching a search query
    pub async fn process_search_albums(&mut self, query: &str, limit: Option<u32>) -> Result<()> {
        let limit_info = match limit {
            Some(l) => format!("limit: {l} albums"),
            None => "no limit".to_string(),
        };
        log::info!("Starting album-based track processing for query: '{query}' ({limit_info})");

        // Emit cycle started event for UI progress
        self.emit_event(ScrubberEvent::cycle_started(format!(
            "Starting album search processing for query: '{query}' ({limit_info})"
        )));

        // Collect albums from search iterator with optional limit
        let mut search_iterator = self.client.search_albums(query);
        let albums_found = self
            .collect_from_iterator(&mut search_iterator, limit)
            .await?;

        log::info!(
            "Found {} albums matching search query '{query}'",
            albums_found.len()
        );

        if albums_found.is_empty() {
            log::warn!(
                "No albums found matching search query '{query}'. Try a different search term."
            );
            return Ok(());
        }

        // Collect all tracks from these albums
        let all_tracks = self.collect_tracks_from_albums(&albums_found).await?;

        log::info!(
            "Collected {} tracks from {} albums matching query '{query}'",
            all_tracks.len(),
            albums_found.len()
        );

        if all_tracks.is_empty() {
            log::warn!("No tracks found in the matched albums");
            return Ok(());
        }

        // Process using shared helper without artist context
        self.process_collected_tracks(
            &all_tracks,
            ProcessingType::Search,
            "Album search processing complete",
        )
        .await?;

        // Emit cycle completed event for UI progress
        self.emit_event(ScrubberEvent::cycle_completed(
            all_tracks.len(),
            0, // TODO: track applied count in future enhancement
        ));

        log::info!(
            "Album search processing complete: processed {} tracks from {} albums matching query '{query}'",
            all_tracks.len(),
            albums_found.len()
        );

        Ok(())
    }

    /// Set the processing timestamp anchor to a specific track's timestamp
    /// This allows manual control of where the scrubber starts processing from
    pub async fn set_timestamp_to_track(&mut self, track: &lastfm_edit::Track) -> Result<()> {
        if let Some(ts) = track.timestamp {
            let track_time = DateTime::from_timestamp(ts as i64, 0).unwrap_or_else(Utc::now);
            let updated_state = TimestampState {
                last_processed_timestamp: Some(track_time),
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

            log::info!(
                "Manually set timestamp anchor to {} for track '{}' by '{}'",
                track_time,
                track.name,
                track.artist
            );

            // Emit anchor update event
            self.emit_event(ScrubberEvent::anchor_updated_from_names(
                ts,
                &track.name,
                &track.artist,
            ));
        } else {
            return Err(lastfm_edit::LastFmError::Io(std::io::Error::other(
                "Track has no timestamp",
            )));
        }
        Ok(())
    }

    /// Get the current timestamp state
    pub async fn get_current_timestamp(&self) -> Result<Option<DateTime<Utc>>> {
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

        Ok(timestamp_state.last_processed_timestamp)
    }

    async fn analyze_tracks(
        &self,
        tracks: &[lastfm_edit::Track],
    ) -> Vec<(usize, Vec<SuggestionWithContext>)> {
        // Load pending items to provide context for action providers
        let (pending_edits_result, pending_rules_result) = tokio::join!(
            async {
                self.storage
                    .lock()
                    .await
                    .load_pending_edits_state()
                    .await
                    .map(|state| state.pending_edits)
                    .map_err(|e| format!("Failed to load pending edits: {e}"))
            },
            async {
                self.storage
                    .lock()
                    .await
                    .load_pending_rewrite_rules_state()
                    .await
                    .map(|state| state.pending_rules)
                    .map_err(|e| format!("Failed to load pending rules: {e}"))
            }
        );

        // Call the unified analyze_tracks method with optional context
        match (pending_edits_result, pending_rules_result) {
            (Ok(pending_edits), Ok(pending_rules)) => {
                match self
                    .action_provider
                    .analyze_tracks(tracks, Some(&pending_edits), Some(&pending_rules))
                    .await
                {
                    Ok(suggestions) => {
                        for (track_idx, track_suggestions) in &suggestions {
                            if let Some(track) = tracks.get(*track_idx) {
                                log::debug!(
                                    "Action provider '{}' (with context) suggested {} actions for track '{} - {}'",
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
                        log::warn!("Error from context-aware action provider: {e}, falling back to regular analysis");
                        // Fall back to no context
                        match self
                            .action_provider
                            .analyze_tracks(tracks, None, None)
                            .await
                        {
                            Ok(suggestions) => {
                                for (track_idx, track_suggestions) in &suggestions {
                                    if let Some(track) = tracks.get(*track_idx) {
                                        log::debug!(
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
                                log::warn!("Error from action provider: {e}");
                                Vec::new()
                            }
                        }
                    }
                }
            }
            (Err(e1), Err(e2)) => {
                log::warn!(
                    "Failed to load pending items: {e1} and {e2}, using analysis without context"
                );
                match self
                    .action_provider
                    .analyze_tracks(tracks, None, None)
                    .await
                {
                    Ok(suggestions) => {
                        for (track_idx, track_suggestions) in &suggestions {
                            if let Some(track) = tracks.get(*track_idx) {
                                log::debug!(
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
                        log::warn!("Error from action provider: {e}");
                        Vec::new()
                    }
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                log::warn!(
                    "Failed to load some pending items: {e}, using analysis without context"
                );
                match self
                    .action_provider
                    .analyze_tracks(tracks, None, None)
                    .await
                {
                    Ok(suggestions) => {
                        for (track_idx, track_suggestions) in &suggestions {
                            if let Some(track) = tracks.get(*track_idx) {
                                log::debug!(
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
                        log::warn!("Error from action provider: {e}");
                        Vec::new()
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    async fn apply_suggestion(
        &mut self,
        track: &lastfm_edit::Track,
        suggestion: &SuggestionWithContext,
    ) -> Result<()> {
        self.apply_suggestion_with_context(track, suggestion, None)
            .await
    }

    /// Check if a ScrobbleEdit contains any actual changes
    fn has_changes(edit: &ScrobbleEdit) -> bool {
        // Check track name change
        if edit.track_name.as_ref() != edit.track_name_original.as_ref() {
            return true;
        }
        // Check artist name change
        if Some(&edit.artist_name) != Some(&edit.artist_name_original) {
            return true;
        }
        // Check album name change
        if edit.album_name.as_ref() != edit.album_name_original.as_ref() {
            return true;
        }
        // Check album artist name change
        if edit.album_artist_name.as_ref() != edit.album_artist_name_original.as_ref() {
            return true;
        }
        false
    }

    async fn apply_suggestion_with_context(
        &mut self,
        track: &lastfm_edit::Track,
        suggestion: &SuggestionWithContext,
        context: Option<ProcessingContext>,
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

        match &suggestion.suggestion {
            ScrubActionSuggestion::Edit(edit) => {
                // Clone edit (edit_all is now always true by default)
                let edit = edit.clone();

                // Check if global settings require confirmation (persistent state takes precedence over config)
                let global_confirmation = settings_state.require_confirmation
                    || settings_state.require_confirmation_for_edits
                    || self.config.scrubber.require_confirmation;

                log::trace!(
                    "Confirmation settings - Global: {}, Provider suggests confirmation: {}, Config dry_run: {}",
                    global_confirmation,
                    suggestion.requires_confirmation,
                    self.config.scrubber.dry_run
                );

                let requires_confirmation = global_confirmation || suggestion.requires_confirmation;

                if self.config.scrubber.dry_run {
                    if requires_confirmation {
                        log::debug!("DRY RUN: Would have created pending edit {edit:?}");
                    } else {
                        log::trace!(
                            "DRY RUN: Would apply edit to track '{}' by '{}': {edit:?}",
                            track.name,
                            track.artist
                        );
                    }
                }

                if requires_confirmation {
                    log::trace!("Edit requires confirmation, creating pending edit");
                    self.create_pending_edit(track, &edit, context.clone())
                        .await?;

                    // Emit event for pending edit skip
                    let default_context = ProcessingContext {
                        run_id: "pending_edit".to_string(),
                        batch_id: None,
                        track_index: None,
                        batch_size: None,
                        is_artist_processing: false,
                    };
                    let log_context = context.unwrap_or(default_context);
                    self.emit_event(ScrubberEvent::track_skipped(
                        track,
                        log_context,
                        "Edit requires confirmation - created as pending".to_string(),
                    ));
                } else if self.config.scrubber.dry_run {
                    // Emit event for dry run skip
                    let default_context = ProcessingContext {
                        run_id: "dry_run".to_string(),
                        batch_id: None,
                        track_index: None,
                        batch_size: None,
                        is_artist_processing: false,
                    };
                    let log_context = context.unwrap_or(default_context);
                    self.emit_event(ScrubberEvent::track_skipped(
                        track,
                        log_context,
                        "Dry run mode - would apply edit".to_string(),
                    ));
                } else {
                    log::trace!("Applying edit directly to track");
                    self.apply_edit_with_context(track, &edit, context).await?;
                }
            }
            ScrubActionSuggestion::ProposeRule { rule, motivation } => {
                log::info!(
                    "Provider proposed new rule for track '{}' by '{}': {}",
                    track.name,
                    track.artist,
                    motivation
                );
                self.handle_proposed_rule(track, rule, motivation).await?;
                if self.config.scrubber.dry_run {
                    log::info!(
                        "DRY RUN: Processed proposed rule for track '{}' by '{}'",
                        track.name,
                        track.artist
                    );
                }
            }
            ScrubActionSuggestion::NoAction => {
                // This shouldn't happen since we filter NoAction in analyze_track
                log::debug!("Provider suggested no action needed");
            }
        }
        Ok(())
    }

    async fn create_pending_edit(
        &self,
        track: &lastfm_edit::Track,
        edit: &ScrobbleEdit,
        context: Option<ProcessingContext>,
    ) -> Result<()> {
        let new_track_name = if edit.track_name.as_ref() == edit.track_name_original.as_ref() {
            None
        } else {
            edit.track_name.clone()
        };

        let new_artist_name = if Some(&edit.artist_name) == Some(&edit.artist_name_original) {
            None
        } else {
            Some(edit.artist_name.clone())
        };

        let new_album_name = if edit.album_name.as_ref() == edit.album_name_original.as_ref() {
            None
        } else {
            edit.album_name.clone()
        };

        let new_album_artist_name =
            if edit.album_artist_name.as_ref() == edit.album_artist_name_original.as_ref() {
                None
            } else {
                edit.album_artist_name.clone()
            };

        let pending_edit = PendingEdit::new(
            track.name.clone(),
            track.artist.clone(),
            edit.album_name_original.clone(),
            edit.album_artist_name_original.clone(),
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

        log::trace!(
            "Created pending edit requiring confirmation (ID: {})",
            pending_edit.id
        );

        // Emit pending edit created event
        let default_context = ProcessingContext {
            run_id: "pending_edit".to_string(),
            batch_id: None,
            track_index: None,
            batch_size: None,
            is_artist_processing: false,
        };
        let log_context = context.unwrap_or(default_context);

        let edit_info = LogEditInfo {
            original_track_name: edit.track_name_original.clone(),
            original_artist_name: Some(edit.artist_name_original.clone()),
            original_album_name: edit.album_name_original.clone(),
            original_album_artist_name: edit.album_artist_name_original.clone(),
            new_track_name: edit.track_name.clone(),
            new_artist_name: Some(edit.artist_name.clone()),
            new_album_name: edit.album_name.clone(),
            new_album_artist_name: edit.album_artist_name.clone(),
        };

        self.emit_event(ScrubberEvent::pending_edit_created(
            pending_edit.id,
            track,
            &edit_info,
            log_context,
        ));

        Ok(())
    }

    #[allow(dead_code)]
    async fn apply_edit(&mut self, track: &lastfm_edit::Track, edit: &ScrobbleEdit) -> Result<()> {
        self.apply_edit_with_context(track, edit, None).await
    }

    async fn apply_edit_with_context(
        &mut self,
        track: &lastfm_edit::Track,
        edit: &ScrobbleEdit,
        context: Option<ProcessingContext>,
    ) -> Result<()> {
        // Log the edit using ScrobbleEdit's Display implementation
        if Self::has_changes(edit) {
            log::debug!("Applying edit: {edit}");

            // Use the comprehensive edit_scrobble method which handles all field changes
            let default_context = ProcessingContext {
                run_id: "manual_edit".to_string(),
                batch_id: None,
                track_index: None,
                batch_size: None,
                is_artist_processing: false,
            };
            let log_context = context.unwrap_or(default_context);

            match self.client.edit_scrobble(edit).await {
                Ok(_response) => {
                    // Emit event for successful edit
                    let edit_info = LogEditInfo {
                        original_track_name: edit.track_name_original.clone(),
                        original_artist_name: Some(edit.artist_name_original.clone()),
                        original_album_name: edit.album_name_original.clone(),
                        original_album_artist_name: edit.album_artist_name_original.clone(),
                        new_track_name: edit.track_name.clone(),
                        new_artist_name: Some(edit.artist_name.clone()),
                        new_album_name: edit.album_name.clone(),
                        new_album_artist_name: edit.album_artist_name.clone(),
                    };

                    self.emit_event(ScrubberEvent::track_edited(track, &edit_info, log_context));
                }
                Err(e) => {
                    log::warn!("Failed to apply edit: {e}");

                    // Emit event for failed edit
                    let edit_info = LogEditInfo {
                        original_track_name: edit.track_name_original.clone(),
                        original_artist_name: Some(edit.artist_name_original.clone()),
                        original_album_name: edit.album_name_original.clone(),
                        original_album_artist_name: edit.album_artist_name_original.clone(),
                        new_track_name: edit.track_name.clone(),
                        new_artist_name: Some(edit.artist_name.clone()),
                        new_album_name: edit.album_name.clone(),
                        new_album_artist_name: edit.album_artist_name.clone(),
                    };

                    self.emit_event(ScrubberEvent::track_edit_failed_from_string(
                        track,
                        Some(&edit_info),
                        log_context,
                        format!("{e}"),
                    ));

                    return Err(e);
                }
            }
        }

        Ok(())
    }

    async fn handle_proposed_rule(
        &self,
        track: &lastfm_edit::Track,
        rule: &crate::rewrite::RewriteRule,
        motivation: &str,
    ) -> Result<()> {
        // Load settings state to check confirmation requirements
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

        // Check if confirmation is required for proposed rules (persistent state takes precedence over config)
        let requires_confirmation = settings_state.require_confirmation
            || settings_state.require_confirmation_for_new_rules
            || self.config.scrubber.require_proposed_rule_confirmation;

        if requires_confirmation {
            // Create a pending rewrite rule for approval
            let pending_rule = PendingRewriteRule::new_with_album_info(
                rule.clone(),
                motivation.to_string(),
                track.name.clone(),
                track.artist.clone(),
                track.album.clone(),
                None, // Track doesn't have album_artist field, will be populated from ScrobbleEdit if needed
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

            log::info!(
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

            log::info!("Auto-approved and added new rewrite rule: {motivation}");
        }
        Ok(())
    }
}
