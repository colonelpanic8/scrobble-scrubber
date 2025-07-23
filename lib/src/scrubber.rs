use chrono::{DateTime, Utc};
use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, Result, ScrobbleEdit};
use log::{info, trace, warn};
use uuid::Uuid;

use crate::config::ScrobbleScrubberConfig;
use crate::events::ScrubberEvent;
use crate::json_logger::{JsonLogger, ProcessingContext};
use crate::persistence::{PendingEdit, PendingRewriteRule, StateStorage, TimestampState};
use crate::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};
use crate::track_cache::TrackCache;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

pub struct ScrobbleScrubber<S: StateStorage, P: ScrubActionProvider> {
    client: Box<dyn LastFmEditClient + Send + Sync>,
    storage: Arc<Mutex<S>>,
    action_provider: P,
    config: ScrobbleScrubberConfig,
    is_running: Arc<RwLock<bool>>,
    should_stop: Arc<RwLock<bool>>,
    event_sender: broadcast::Sender<ScrubberEvent>,
    trigger_immediate: Arc<RwLock<bool>>,
    track_cache: TrackCache,
    json_logger: JsonLogger,
}

impl<S: StateStorage, P: ScrubActionProvider> ScrobbleScrubber<S, P> {
    pub fn new(
        storage: Arc<Mutex<S>>,
        client: Box<dyn LastFmEditClient + Send + Sync>,
        action_provider: P,
        config: ScrobbleScrubberConfig,
    ) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        let json_logger = JsonLogger::new(
            config.scrubber.json_logging.log_file_path(),
            config.scrubber.json_logging.enabled,
        );
        Self {
            client,
            storage,
            action_provider,
            config,
            is_running: Arc::new(RwLock::new(false)),
            should_stop: Arc::new(RwLock::new(false)),
            event_sender,
            trigger_immediate: Arc::new(RwLock::new(false)),
            track_cache: TrackCache::load(),
            json_logger,
        }
    }

    /// Trigger immediate processing, bypassing the normal wait interval
    pub async fn trigger_immediate_processing(&self) {
        *self.trigger_immediate.write().await = true;
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

    /// Ensure timestamp state is initialized with the most recent track if not set
    #[allow(dead_code)]
    async fn ensure_timestamp_initialized(
        &mut self,
        timestamp_state: TimestampState,
    ) -> Result<TimestampState> {
        if timestamp_state.last_processed_timestamp.is_some() {
            return Ok(timestamp_state);
        }

        info!("No timestamp anchor found, initializing with most recent track...");

        let mut recent_iterator = self.client.recent_tracks();

        // Get the first (most recent) track to use as our anchor
        if let Some(first_track) = recent_iterator.next().await? {
            info!(
                "Most recent track found: '{}' by '{}' (playcount: {})",
                first_track.name, first_track.artist, first_track.playcount
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

                info!(
                    "Initialized timestamp anchor at: {} for track '{}' by '{}'",
                    track_time, first_track.name, first_track.artist
                );
                return Ok(new_state);
            } else {
                info!(
                    "Most recent track '{}' by '{}' has no timestamp, using original state",
                    first_track.name, first_track.artist
                );
            }
        } else {
            info!("No recent tracks found, using original state");
        }

        Ok(timestamp_state)
    }

    pub async fn run(&mut self) -> Result<()> {
        self.emit_event(ScrubberEvent::started("Scrubber started".to_string()));

        loop {
            // Check if we should stop
            if *self.should_stop.read().await {
                info!("Scrubber stop requested, exiting main loop");
                break;
            }

            *self.is_running.write().await = true;
            info!("Starting track monitoring cycle...");
            self.emit_event(ScrubberEvent::cycle_started(
                "Starting track monitoring cycle".to_string(),
            ));

            if let Err(e) = self.check_and_process_tracks().await {
                warn!("Error during track processing: {e}");
                self.emit_event(ScrubberEvent::error(format!(
                    "Error during track processing: {e}"
                )));
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

                // Check if immediate processing was triggered
                if *self.trigger_immediate.read().await {
                    *self.trigger_immediate.write().await = false; // Reset flag
                    info!("Immediate processing triggered, skipping remaining sleep");
                    self.emit_event(ScrubberEvent::info(
                        "Immediate processing triggered".to_string(),
                    ));
                    break;
                }

                let remaining = sleep_duration - elapsed;
                let sleep_time = std::cmp::min(check_interval, remaining);
                tokio::time::sleep(sleep_time).await;
                elapsed += sleep_time;
            }
        }

        self.emit_event(ScrubberEvent::stopped("Scrubber stopped".to_string()));
        Ok(())
    }

    async fn check_and_process_tracks(&mut self) -> Result<()> {
        // Step 1: Update cache with latest tracks from API
        info!("Updating track cache from Last.fm API...");
        self.update_cache_from_api().await?;

        // Step 2: Load current timestamp state to know where to start reading
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

        // Step 3: Find tracks to process from cache
        let tracks_to_process = self
            .find_tracks_to_process_from_cache(&timestamp_state)
            .await?;

        info!(
            "Found {} tracks to process from cache",
            tracks_to_process.len()
        );

        // Step 4: Process all collected tracks (oldest first) with incremental timestamp updates
        if !tracks_to_process.is_empty() {
            info!(
                "Processing {} tracks in batches of {} (oldest first)...",
                tracks_to_process.len(),
                self.config.scrubber.processing_batch_size
            );

            self.process_tracks_in_batches(&tracks_to_process).await?;
        }

        info!(
            "Processing complete: processed {} tracks from cache",
            tracks_to_process.len()
        );

        self.emit_event(ScrubberEvent::cycle_completed(
            tracks_to_process.len(),
            0, // We'll update this when we track rule applications
        ));
        Ok(())
    }

    /// Update cache with latest tracks from Last.fm API
    async fn update_cache_from_api(&mut self) -> Result<()> {
        let mut recent_iterator = self.client.recent_tracks();
        let mut api_tracks = Vec::new();
        let mut fetched = 0;

        // Fetch first few pages of recent tracks to update cache
        const MAX_TRACKS_TO_FETCH: usize = 200; // Fetch about 4 pages worth

        info!("Fetching recent tracks from Last.fm API...");
        while let Some(track) = recent_iterator.next().await? {
            api_tracks.push(track);
            fetched += 1;

            if fetched >= MAX_TRACKS_TO_FETCH {
                break;
            }
        }

        info!(
            "Fetched {} tracks from API, merging with cache...",
            api_tracks.len()
        );

        // Merge with existing cache
        self.track_cache.merge_recent_tracks(api_tracks);

        // Save updated cache
        if let Err(e) = self.track_cache.save() {
            warn!("Failed to save updated cache: {e}");
        } else {
            info!("Cache updated and saved successfully");
        }

        Ok(())
    }

    /// Find tracks to process from cache based on timestamp state
    async fn find_tracks_to_process_from_cache(
        &self,
        timestamp_state: &TimestampState,
    ) -> Result<Vec<lastfm_edit::Track>> {
        let mut tracks_to_process = Vec::new();

        // Get all recent tracks from cache, sorted newest first
        let cached_tracks = self.track_cache.get_all_recent_tracks();

        info!(
            "Scanning {} cached tracks to find new tracks since last run...",
            cached_tracks.len()
        );

        // Debug: Show anchor timestamp
        if let Some(anchor) = timestamp_state.last_processed_timestamp {
            trace!("Using anchor timestamp: {anchor}");
        } else {
            trace!("No anchor timestamp set (first run)");
        }

        for cached_track in cached_tracks {
            // Check if we've reached our last processed track (anchor point)
            if let Some(last_processed) = timestamp_state.last_processed_timestamp {
                if let Some(track_ts) = cached_track.timestamp {
                    let track_time = DateTime::from_timestamp(track_ts as i64, 0);
                    if let Some(track_time) = track_time {
                        trace!(
                            "Examining cached track '{}' by '{}' at {} vs anchor at {}",
                            cached_track.name,
                            cached_track.artist,
                            track_time,
                            last_processed
                        );

                        if track_time <= last_processed {
                            info!("Reached previously processed track '{}' by '{}' at {}, found {} new tracks to process",
                                  cached_track.name, cached_track.artist, track_time, tracks_to_process.len());
                            break; // Stop here - we've caught up to where we left off
                        }
                        // Track is newer than our anchor, collect it for processing
                        info!(
                            "Found new track: '{}' by '{}' at {}",
                            cached_track.name, cached_track.artist, track_time
                        );
                    }
                } else {
                    trace!(
                        "Cached track '{}' by '{}' has no timestamp",
                        cached_track.name,
                        cached_track.artist
                    );
                }
            } else {
                // First run - no anchor timestamp, collect tracks up to limit
                info!(
                    "First run - found cached track: '{}' by '{}'",
                    cached_track.name, cached_track.artist
                );
            }

            // Convert SerializableTrack back to Track for processing
            tracks_to_process.push(lastfm_edit::Track::from(cached_track));
        }

        // Reverse to process oldest first (tracks were collected newest first)
        tracks_to_process.reverse();

        Ok(tracks_to_process)
    }

    /// Process the last N tracks without updating timestamp state
    pub async fn process_last_n_tracks(&mut self, n: u32) -> Result<()> {
        info!("Processing last {n} tracks (no timestamp updates)");

        let mut recent_iterator = self.client.recent_tracks();
        let mut tracks_to_process = Vec::new();
        let mut examined = 0;

        // Collect the last n tracks
        while let Some(track) = recent_iterator.next().await? {
            examined += 1;
            tracks_to_process.push(track);

            if tracks_to_process.len() >= n as usize {
                info!("Collected {n} tracks for processing");
                break;
            }
        }

        if tracks_to_process.is_empty() {
            info!("No tracks found to process");
            return Ok(());
        }

        info!(
            "Processing {} tracks in batches of {} (no timestamp updates)...",
            tracks_to_process.len(),
            self.config.scrubber.processing_batch_size
        );

        // Process tracks without timestamp updates
        self.process_tracks_in_batches_no_timestamp_update(&tracks_to_process)
            .await?;

        info!(
            "Processing complete: examined {} tracks, processed {} tracks",
            examined,
            tracks_to_process.len()
        );
        Ok(())
    }

    /// Process tracks in configurable batches with incremental timestamp updates
    async fn process_tracks_in_batches(&mut self, tracks: &[lastfm_edit::Track]) -> Result<()> {
        let batch_size = self.config.scrubber.processing_batch_size as usize;

        for (batch_num, batch) in tracks.chunks(batch_size).enumerate() {
            info!(
                "Processing batch {} of {} (batch size: {})",
                batch_num + 1,
                tracks.len().div_ceil(batch_size),
                batch.len()
            );

            // Process this batch
            self.process_track_batch(batch).await?;

            // Update timestamp incrementally after each batch (using the newest track in batch)
            if let Some(newest_track_in_batch) = batch.last() {
                self.update_timestamp_to_track(newest_track_in_batch)
                    .await?;
            }
        }

        Ok(())
    }

    /// Process tracks in configurable batches without timestamp updates
    async fn process_tracks_in_batches_no_timestamp_update(
        &mut self,
        tracks: &[lastfm_edit::Track],
    ) -> Result<()> {
        let batch_size = self.config.scrubber.processing_batch_size as usize;

        for (batch_num, batch) in tracks.chunks(batch_size).enumerate() {
            info!(
                "Processing batch {} of {} (batch size: {}) - no timestamp updates",
                batch_num + 1,
                tracks.len().div_ceil(batch_size),
                batch.len()
            );

            // Process this batch without timestamp updates
            self.process_track_batch(batch).await?;
        }

        Ok(())
    }

    /// Process a single batch of tracks with their suggestions
    async fn process_track_batch(&mut self, tracks: &[lastfm_edit::Track]) -> Result<()> {
        trace!("Starting batch analysis for {} tracks", tracks.len());

        let batch_suggestions = self.analyze_tracks(tracks).await;
        let run_id = Uuid::new_v4().to_string();

        // Process each track individually and emit detailed events
        for (track_index, track) in tracks.iter().enumerate() {
            info!(
                "Processing track: {} - {} (index {})",
                track.artist, track.name, track_index
            );

            // Find suggestions for this track
            let empty_suggestions = vec![];
            let track_suggestions = batch_suggestions
                .iter()
                .find(|(index, _)| *index == track_index)
                .map(|(_, suggestions)| suggestions)
                .unwrap_or(&empty_suggestions);

            // Determine processing result
            let result = if track_suggestions.is_empty() {
                "no rules applied".to_string()
            } else {
                match track_suggestions.len() {
                    1 => "1 rule applied".to_string(),
                    n => format!("{n} rules applied"),
                }
            };

            // Emit detailed track processed event
            self.emit_event(ScrubberEvent::track_processed_with_result(
                &track.name,
                &track.artist,
                &result,
            ));

            // Log track processing to JSON
            let context = ProcessingContext {
                run_id: run_id.clone(),
                batch_id: Some(format!("batch_{}", chrono::Utc::now().timestamp())),
                track_index: Some(track_index),
                batch_size: Some(tracks.len()),
            };

            let applied_rules: Vec<String> = track_suggestions
                .iter()
                .enumerate()
                .map(|(i, _)| format!("rule_{}", i + 1))
                .collect();

            if track_suggestions.is_empty() {
                // Log that track was processed but no changes were made
                if let Err(e) = self.json_logger.log_track_processed(
                    track,
                    0,      // rules_applied
                    vec![], // applied_rules
                    context,
                ) {
                    warn!("Failed to log track processed to JSON: {e}");
                }
            } else {
                // Track will have edits applied, so we'll log that in apply_suggestion
                // For now, just log that it was processed with suggestions
                if let Err(e) = self.json_logger.log_track_processed(
                    track,
                    track_suggestions.len(),
                    applied_rules,
                    context,
                ) {
                    warn!("Failed to log track processed to JSON: {e}");
                }
            }
        }

        // Then apply suggestions
        for (track_index, suggestions) in batch_suggestions {
            if track_index >= tracks.len() {
                log::warn!(
                    "Invalid track index {} for batch size {}",
                    track_index,
                    tracks.len()
                );
                continue;
            }

            let track = &tracks[track_index];

            if suggestions.is_empty() {
                info!(
                    "No suggestions for track: {} - {}",
                    track.artist, track.name
                );
                continue;
            }

            info!(
                "Applying {} suggestions to track: {} - {}",
                suggestions.len(),
                track.artist,
                track.name
            );

            for (i, suggestion) in suggestions.iter().enumerate() {
                trace!(
                    "Applying suggestion {}/{} for track '{}' by '{}': {:?}",
                    i + 1,
                    suggestions.len(),
                    track.name,
                    track.artist,
                    suggestion
                );

                let suggestion_context = ProcessingContext {
                    run_id: run_id.clone(),
                    batch_id: Some(format!("batch_{}", chrono::Utc::now().timestamp())),
                    track_index: Some(track_index),
                    batch_size: Some(tracks.len()),
                };
                self.apply_suggestion_with_context(track, suggestion, Some(suggestion_context))
                    .await?;

                // Emit rule applied event based on suggestion type
                let description = match suggestion {
                    crate::scrub_action_provider::ScrubActionSuggestion::Edit(edit) => {
                        trace!("Applied edit: {edit:?}");
                        "Applied edit".to_string()
                    }
                    crate::scrub_action_provider::ScrubActionSuggestion::ProposeRule {
                        rule,
                        motivation,
                    } => {
                        trace!("Proposed rule: {rule:?} with motivation: {motivation}");
                        format!("Proposed rule: {motivation}")
                    }
                    crate::scrub_action_provider::ScrubActionSuggestion::NoAction => {
                        trace!("No action taken for track");
                        "No action taken".to_string()
                    }
                };
                self.emit_event(ScrubberEvent::rule_applied(
                    &track.name,
                    &track.artist,
                    &description,
                ));
            }
        }

        Ok(())
    }

    /// Update timestamp state to a specific track
    async fn update_timestamp_to_track(&mut self, track: &lastfm_edit::Track) -> Result<()> {
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

            info!(
                "Updated timestamp anchor to: {} (track: '{}' by '{}')",
                track_time, track.name, track.artist
            );

            // Emit anchor update event
            self.emit_event(ScrubberEvent::anchor_updated(
                ts,
                &track.name,
                &track.artist,
            ));
        }
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
            self.process_track_batch(&tracks_to_process).await?;
        }

        info!("Processed {processed} tracks for artist '{artist}'");
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

            info!(
                "Manually set timestamp anchor to {} for track '{}' by '{}'",
                track_time, track.name, track.artist
            );

            // Emit anchor update event
            self.emit_event(ScrubberEvent::anchor_updated(
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
    ) -> Vec<(usize, Vec<ScrubActionSuggestion>)> {
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
                                info!(
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
                        warn!("Error from context-aware action provider: {e}, falling back to regular analysis");
                        // Fall back to no context
                        match self
                            .action_provider
                            .analyze_tracks(tracks, None, None)
                            .await
                        {
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
                }
            }
            (Err(e1), Err(e2)) => {
                warn!(
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
            (Err(e), _) | (_, Err(e)) => {
                warn!("Failed to load some pending items: {e}, using analysis without context");
                match self
                    .action_provider
                    .analyze_tracks(tracks, None, None)
                    .await
                {
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
        }
    }

    #[allow(dead_code)]
    async fn apply_suggestion(
        &mut self,
        track: &lastfm_edit::Track,
        suggestion: &ScrubActionSuggestion,
    ) -> Result<()> {
        self.apply_suggestion_with_context(track, suggestion, None)
            .await
    }

    async fn apply_suggestion_with_context(
        &mut self,
        track: &lastfm_edit::Track,
        suggestion: &ScrubActionSuggestion,
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

        match suggestion {
            ScrubActionSuggestion::Edit(edit) => {
                // Load rewrite rules to check individual rule confirmation requirements
                let rules_state = self
                    .storage
                    .lock()
                    .await
                    .load_rewrite_rules_state()
                    .await
                    .map_err(|e| {
                        lastfm_edit::LastFmError::Io(std::io::Error::other(format!(
                            "Failed to load rewrite rules state: {e}"
                        )))
                    })?;

                // Check if any applicable rule requires individual confirmation
                let individual_rule_confirmation = rules_state.rewrite_rules.iter().any(|rule| {
                    let applies = rule.applies_to(track).unwrap_or(false);
                    let requires_conf = rule.requires_confirmation;
                    trace!(
                        "Rule '{}' applies: {}, requires confirmation: {}",
                        rule.name.as_deref().unwrap_or("Unnamed"),
                        applies,
                        requires_conf
                    );
                    applies && requires_conf
                });

                // Check if global settings require confirmation (persistent state takes precedence over config)
                let global_confirmation = settings_state.require_confirmation
                    || settings_state.require_confirmation_for_edits
                    || self.config.scrubber.require_confirmation;

                trace!(
                    "Confirmation settings - Global: {}, Individual rule: {}, Config dry_run: {}",
                    global_confirmation,
                    individual_rule_confirmation,
                    self.config.scrubber.dry_run
                );

                let requires_confirmation = global_confirmation || individual_rule_confirmation;

                if requires_confirmation {
                    trace!("Edit requires confirmation, creating pending edit");
                    self.create_pending_edit(track, edit).await?;

                    // Log that edit was created as pending (requires confirmation)
                    let default_context = ProcessingContext {
                        run_id: "pending_edit".to_string(),
                        batch_id: None,
                        track_index: None,
                        batch_size: None,
                    };
                    let log_context = context.unwrap_or(default_context);

                    if let Err(e) = self.json_logger.log_track_skipped(
                        track,
                        "Edit requires confirmation - created as pending".to_string(),
                        log_context,
                    ) {
                        warn!("Failed to log pending edit to JSON: {e}");
                    }

                    if self.config.scrubber.dry_run {
                        info!(
                            "DRY RUN: Created pending edit for track '{}' by '{}'",
                            track.name, track.artist
                        );
                    }
                } else if self.config.scrubber.dry_run {
                    trace!("Dry run mode, would apply edit directly");
                    info!(
                        "DRY RUN: Would apply edit to track '{}' by '{}': {edit:?}",
                        track.name, track.artist
                    );

                    // Log that edit would be applied in dry run mode
                    let default_context = ProcessingContext {
                        run_id: "dry_run".to_string(),
                        batch_id: None,
                        track_index: None,
                        batch_size: None,
                    };
                    let log_context = context.unwrap_or(default_context);

                    if let Err(e) = self.json_logger.log_track_skipped(
                        track,
                        "Dry run mode - would apply edit".to_string(),
                        log_context,
                    ) {
                        warn!("Failed to log dry run to JSON: {e}");
                    }
                } else {
                    trace!("Applying edit directly to track");
                    self.apply_edit_with_context(track, edit, context).await?;
                }
            }
            ScrubActionSuggestion::ProposeRule { rule, motivation } => {
                info!(
                    "Provider proposed new rule for track '{}' by '{}': {}",
                    track.name, track.artist, motivation
                );
                self.handle_proposed_rule(track, rule, motivation).await?;
                if self.config.scrubber.dry_run {
                    info!(
                        "DRY RUN: Processed proposed rule for track '{}' by '{}'",
                        track.name, track.artist
                    );
                }
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
        let new_track_name = if Some(&edit.track_name) == edit.track_name_original.as_ref() {
            None
        } else {
            Some(edit.track_name.clone())
        };

        let new_artist_name = if Some(&edit.artist_name) == edit.artist_name_original.as_ref() {
            None
        } else {
            Some(edit.artist_name.clone())
        };

        let new_album_name = if Some(&edit.album_name) == edit.album_name_original.as_ref() {
            None
        } else {
            Some(edit.album_name.clone())
        };

        let new_album_artist_name =
            if Some(&edit.album_artist_name) == edit.album_artist_name_original.as_ref() {
                None
            } else {
                Some(edit.album_artist_name.clone())
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

        info!(
            "Created pending edit requiring confirmation (ID: {})",
            pending_edit.id
        );
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
        // Check what changes are being made and log them
        let mut changes = Vec::new();

        if Some(&edit.track_name) != edit.track_name_original.as_ref() {
            changes.push(format!(
                "track: '{}' -> '{}'",
                edit.track_name_original.as_deref().unwrap_or("unknown"),
                edit.track_name
            ));
        }
        if Some(&edit.artist_name) != edit.artist_name_original.as_ref() {
            changes.push(format!(
                "artist: '{}' -> '{}'",
                edit.artist_name_original.as_deref().unwrap_or("unknown"),
                edit.artist_name
            ));
        }
        if Some(&edit.album_name) != edit.album_name_original.as_ref() {
            changes.push(format!(
                "album: '{}' -> '{}'",
                edit.album_name_original.as_deref().unwrap_or("unknown"),
                edit.album_name
            ));
        }
        if Some(&edit.album_artist_name) != edit.album_artist_name_original.as_ref() {
            changes.push(format!(
                "album artist: '{}' -> '{}'",
                edit.album_artist_name_original
                    .as_deref()
                    .unwrap_or("unknown"),
                edit.album_artist_name
            ));
        }

        if !changes.is_empty() {
            info!(
                "Applying edit to track '{}' by '{}': {}",
                edit.track_name_original.as_deref().unwrap_or("unknown"),
                edit.artist_name_original.as_deref().unwrap_or("unknown"),
                changes.join(", ")
            );

            // Use the comprehensive edit_scrobble method which handles all field changes
            let default_context = ProcessingContext {
                run_id: "manual_edit".to_string(),
                batch_id: None,
                track_index: None,
                batch_size: None,
            };
            let log_context = context.unwrap_or(default_context);

            match self.client.edit_scrobble(edit).await {
                Ok(response) => {
                    info!("Edit applied successfully: {response:?}");

                    // Log successful edit to JSON
                    if let Err(e) = self.json_logger.log_track_edited(
                        track,
                        edit,
                        1,                                // rules_applied
                        vec!["applied_edit".to_string()], // applied_rules
                        log_context,
                    ) {
                        warn!("Failed to log track edit to JSON: {e}");
                    }
                }
                Err(e) => {
                    warn!("Failed to apply edit: {e}");

                    // Log failed edit to JSON
                    if let Err(log_err) = self.json_logger.log_track_edit_failed(
                        track,
                        Some(edit),
                        format!("{e}"),
                        0,      // rules_applied
                        vec![], // applied_rules
                        log_context,
                    ) {
                        warn!("Failed to log track edit failure to JSON: {log_err}");
                    }

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
