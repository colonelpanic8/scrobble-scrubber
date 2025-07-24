use chrono::{DateTime, Utc};
use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, Result, ScrobbleEdit};
use log::{trace, warn};
use uuid::Uuid;

use crate::config::ScrobbleScrubberConfig;
use crate::events::ScrubberEvent;
use crate::events::{LogEditInfo, LogTrackInfo, ProcessingContext};
use crate::persistence::{PendingEdit, PendingRewriteRule, StateStorage, TimestampState};
use crate::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};
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
        let (event_sender, _) = broadcast::channel(1000);
        Self {
            client,
            storage,
            action_provider,
            config,
            is_running: Arc::new(RwLock::new(false)),
            should_stop: Arc::new(Notify::new()),
            event_sender,
            trigger_immediate: Arc::new(Notify::new()),
            track_provider: TrackProvider::Cached(CachedTrackProvider::new()),
        }
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
        Self {
            client,
            storage,
            action_provider,
            config,
            is_running: Arc::new(RwLock::new(false)),
            should_stop: Arc::new(Notify::new()),
            event_sender,
            trigger_immediate: Arc::new(Notify::new()),
            track_provider,
        }
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
            warn!("Error during track processing: {e}");
            self.emit_event(ScrubberEvent::error(format!(
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
        let tracks_to_process = self
            .find_tracks_to_process_from_cache(&timestamp_state)
            .await?;

        log::info!(
            "Found {} tracks to process from cache",
            tracks_to_process.len()
        );

        // Step 4: Process all collected tracks (oldest first) and update anchor after processing
        if !tracks_to_process.is_empty() {
            log::info!(
                "Processing {} tracks in batches of {} (oldest first)...",
                tracks_to_process.len(),
                self.config.scrubber.processing_batch_size
            );

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
        self.process_tracks_in_batches_no_timestamp_update(tracks)
            .await?;

        // After processing, update anchor to the newest (last in chronological order) processed track
        // Since tracks are processed oldest first, the newest processed track is the last one
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
    async fn find_tracks_to_process_from_cache(
        &self,
        timestamp_state: &TimestampState,
    ) -> Result<Vec<lastfm_edit::Track>> {
        let mut tracks_to_process = Vec::new();

        // Get all recent tracks from provider, sorted newest first
        let cached_tracks = self.track_provider.get_all_recent_tracks();

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
                            log::info!("Reached previously processed track '{}' by '{}' at {}, found {} new tracks to process",
                                  cached_track.name, cached_track.artist, track_time, tracks_to_process.len());
                            break; // Stop here - we've caught up to where we left off
                        }
                    }
                } else {
                    warn!(
                        "Cached track '{}' by '{}' has no timestamp",
                        cached_track.name, cached_track.artist
                    );
                }
            } else {
                // First run - no anchor timestamp, collect tracks up to limit
                log::info!(
                    "First run - found cached track: '{}' by '{}'",
                    cached_track.name,
                    cached_track.artist
                );
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

        log::info!(
            "Processing {} tracks in batches of {} (no timestamp updates)...",
            tracks_to_process.len(),
            self.config.scrubber.processing_batch_size
        );

        // Process tracks without timestamp updates
        self.process_tracks_in_batches_no_timestamp_update(&tracks_to_process)
            .await?;

        log::info!(
            "Processing complete: examined {} tracks, processed {} tracks",
            examined,
            tracks_to_process.len()
        );
        Ok(())
    }

    /// Process tracks in configurable batches without timestamp updates
    async fn process_tracks_in_batches_no_timestamp_update(
        &mut self,
        tracks: &[lastfm_edit::Track],
    ) -> Result<()> {
        self.process_tracks_in_batches_no_timestamp_update_with_context(tracks, false)
            .await
    }

    /// Process tracks in configurable batches without timestamp updates, with artist processing context
    async fn process_tracks_in_batches_no_timestamp_update_with_context(
        &mut self,
        tracks: &[lastfm_edit::Track],
        is_artist_processing: bool,
    ) -> Result<()> {
        let batch_size = self.config.scrubber.processing_batch_size as usize;

        for (batch_num, batch) in tracks.chunks(batch_size).enumerate() {
            log::info!(
                "Processing batch {} of {} (batch size: {}) - no timestamp updates",
                batch_num + 1,
                tracks.len().div_ceil(batch_size),
                batch.len()
            );

            // Process this batch without timestamp updates
            self.process_track_batch_with_context(batch, is_artist_processing)
                .await?;
        }

        Ok(())
    }

    /// Process a single batch of tracks with their suggestions and artist processing context
    async fn process_track_batch_with_context(
        &mut self,
        tracks: &[lastfm_edit::Track],
        is_artist_processing: bool,
    ) -> Result<()> {
        trace!("Starting batch analysis for {} tracks", tracks.len());

        let batch_suggestions = self.analyze_tracks(tracks).await;
        let run_id = Uuid::new_v4().to_string();

        // Process each track individually and emit detailed events
        for (track_index, track) in tracks.iter().enumerate() {
            log::info!(
                "Processing track: {} - {} (index {})",
                track.artist,
                track.name,
                track_index
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
            self.emit_event(ScrubberEvent::track_processed(
                track.clone(),
                track_suggestions.clone(),
                result,
            ));
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
                log::info!(
                    "No suggestions for track: {} - {}",
                    track.artist,
                    track.name
                );
                continue;
            }

            log::info!(
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
                    is_artist_processing,
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
                    track.clone(),
                    suggestion.clone(),
                    description,
                ));
            }
        }

        Ok(())
    }

    /// Process all tracks for a specific artist
    pub async fn process_artist(&mut self, artist: &str) -> Result<()> {
        log::info!("Starting artist track processing for: {artist}");

        let mut artist_iterator = self.client.artist_tracks(artist);
        let mut processed = 0;

        // Collect tracks first to avoid borrow checker issues
        let mut tracks_to_process = Vec::new();
        while let Some(track) = artist_iterator.next().await? {
            tracks_to_process.push(track);
            processed += 1;
        }

        log::info!(
            "Found {} tracks for artist '{}'",
            tracks_to_process.len(),
            artist
        );

        // Process collected tracks in batch with artist processing context
        if !tracks_to_process.is_empty() {
            self.process_tracks_in_batches_no_timestamp_update_with_context(
                &tracks_to_process,
                true,
            )
            .await?;
        }

        log::info!("Processed {processed} tracks for artist '{artist}'");
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
                                log::info!(
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
                                        log::info!(
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
                                log::info!(
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
                                log::info!(
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
                // Clone edit and set edit_all to true if this is artist processing
                let mut edit = edit.clone();
                if context.as_ref().is_some_and(|c| c.is_artist_processing) {
                    edit.edit_all = true;
                }
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
                    self.create_pending_edit(track, &edit).await?;

                    // Emit event for pending edit skip
                    let default_context = ProcessingContext {
                        run_id: "pending_edit".to_string(),
                        batch_id: None,
                        track_index: None,
                        batch_size: None,
                        is_artist_processing: false,
                    };
                    let log_context = context.unwrap_or(default_context);
                    let track_info = LogTrackInfo::from(track);

                    self.emit_event(ScrubberEvent::track_skipped(
                        &track_info,
                        log_context,
                        "Edit requires confirmation - created as pending".to_string(),
                    ));

                    if self.config.scrubber.dry_run {
                        log::info!(
                            "DRY RUN: Created pending edit for track '{}' by '{}'",
                            track.name,
                            track.artist
                        );
                    }
                } else if self.config.scrubber.dry_run {
                    trace!("Dry run mode, would apply edit directly");
                    log::info!(
                        "DRY RUN: Would apply edit to track '{}' by '{}': {edit:?}",
                        track.name,
                        track.artist
                    );

                    // Emit event for dry run skip
                    let default_context = ProcessingContext {
                        run_id: "dry_run".to_string(),
                        batch_id: None,
                        track_index: None,
                        batch_size: None,
                        is_artist_processing: false,
                    };
                    let log_context = context.unwrap_or(default_context);
                    let track_info = LogTrackInfo::from(track);

                    self.emit_event(ScrubberEvent::track_skipped(
                        &track_info,
                        log_context,
                        "Dry run mode - would apply edit".to_string(),
                    ));
                } else {
                    trace!("Applying edit directly to track");
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
                log::info!("Provider suggested no action needed");
            }
        }
        Ok(())
    }

    async fn create_pending_edit(
        &self,
        track: &lastfm_edit::Track,
        edit: &ScrobbleEdit,
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

        log::info!(
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

        if edit.track_name.as_ref() != edit.track_name_original.as_ref() {
            changes.push(format!(
                "track: '{}' -> '{}'",
                edit.track_name_original.as_deref().unwrap_or("unknown"),
                edit.track_name.as_deref().unwrap_or("unknown")
            ));
        }
        if Some(&edit.artist_name) != Some(&edit.artist_name_original) {
            changes.push(format!(
                "artist: '{}' -> '{}'",
                &edit.artist_name_original, edit.artist_name
            ));
        }
        if edit.album_name.as_ref() != edit.album_name_original.as_ref() {
            changes.push(format!(
                "album: '{}' -> '{}'",
                edit.album_name_original.as_deref().unwrap_or("unknown"),
                edit.album_name.as_deref().unwrap_or("unknown")
            ));
        }
        if edit.album_artist_name.as_ref() != edit.album_artist_name_original.as_ref() {
            changes.push(format!(
                "album artist: '{}' -> '{}'",
                edit.album_artist_name_original
                    .as_deref()
                    .unwrap_or("unknown"),
                edit.album_artist_name.as_deref().unwrap_or("unknown")
            ));
        }

        if !changes.is_empty() {
            log::info!(
                "Applying edit to track '{}' by '{}': {}",
                edit.track_name_original.as_deref().unwrap_or("unknown"),
                &edit.artist_name_original,
                changes.join(", ")
            );

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
                Ok(response) => {
                    log::info!("Edit applied successfully: {response:?}");

                    // Emit event for successful edit
                    let track_info = LogTrackInfo::from(track);
                    let edit_info = LogEditInfo::from(edit);

                    self.emit_event(ScrubberEvent::track_edited(
                        &track_info,
                        &edit_info,
                        log_context,
                    ));
                }
                Err(e) => {
                    warn!("Failed to apply edit: {e}");

                    // Emit event for failed edit
                    let track_info = LogTrackInfo::from(track);
                    let edit_info = LogEditInfo::from(edit);

                    self.emit_event(ScrubberEvent::track_edit_failed(
                        &track_info,
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
