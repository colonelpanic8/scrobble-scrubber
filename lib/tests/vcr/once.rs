use chrono::DateTime;
use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::events::ScrubberEventType;
use scrobble_scrubber::persistence::{MemoryStorage, RewriteRulesState};
use scrobble_scrubber::rewrite::load_comprehensive_default_rules;
use scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
use scrobble_scrubber::scrubber::ScrobbleScrubber;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::common::create_lastfm_vcr_client;

#[test_log::test(tokio::test)]
async fn should_run_end_to_end_processing_with_default_rules() {
    // Create VCR client
    let lastfm_client = create_lastfm_vcr_client("scrubber_end_to_end_with_default_rules")
        .await
        .expect("Failed to create VCR test client");

    // Set up storage
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));

    // Load all default remaster cleanup rules using existing functionality
    let rewrite_rules = load_comprehensive_default_rules();
    let rules_state = RewriteRulesState { rewrite_rules };

    log::debug!("Loaded {} default rules", rules_state.rewrite_rules.len());

    // Create action provider with default rules
    let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);

    // Create configuration with dry_run disabled for actual testing
    let mut config = ScrobbleScrubberConfig::default();
    config.scrubber.dry_run = false;
    config.scrubber.require_confirmation = false; // Auto-apply edits for testing

    log::debug!(
        "Config: dry_run={}, require_confirmation={}",
        config.scrubber.dry_run,
        config.scrubber.require_confirmation
    );

    // Create scrubber without cache to ensure we're not using cached tracks
    let mut scrubber = ScrobbleScrubber::with_direct_provider(
        storage.clone(),
        lastfm_client,
        action_provider,
        config,
    );

    // Subscribe to events to track edits
    let mut event_receiver = scrubber.subscribe_events();

    // Spawn a task to collect edit events
    let edit_events = Arc::new(Mutex::new(Vec::new()));
    let edit_events_clone = edit_events.clone();
    let failed_edit_events = Arc::new(Mutex::new(Vec::new()));
    let failed_edit_events_clone = failed_edit_events.clone();

    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            match &event.event_type {
                ScrubberEventType::TrackEdited {
                    track,
                    edit,
                    context: _,
                } => {
                    log::info!(
                        "Edit performed: {} - {} -> {} - {}",
                        track.artist,
                        track.name,
                        edit.new_artist_name.as_ref().unwrap_or(&track.artist),
                        edit.new_track_name.as_ref().unwrap_or(&track.name)
                    );
                    edit_events_clone
                        .lock()
                        .await
                        .push((track.clone(), edit.clone()));
                }
                ScrubberEventType::TrackEditFailed {
                    track, edit, error, ..
                } => {
                    log::error!(
                        "Edit failed for track: {} - {} (error: {})",
                        track.artist,
                        track.name,
                        error
                    );
                    if let Some(edit) = edit {
                        log::info!(
                            "Failed edit was: {} - {} -> {} - {}",
                            track.artist,
                            track.name,
                            edit.new_artist_name.as_ref().unwrap_or(&track.artist),
                            edit.new_track_name.as_ref().unwrap_or(&track.name)
                        );
                        failed_edit_events_clone.lock().await.push((
                            track.clone(),
                            edit.clone(),
                            error.to_string(),
                        ));
                    }
                }
                ScrubberEventType::PendingEditCreated { track, edit, .. } => {
                    log::info!(
                        "Pending edit created: {} - {} -> {} - {}",
                        track.artist,
                        track.name,
                        edit.new_artist_name.as_ref().unwrap_or(&track.artist),
                        edit.new_track_name.as_ref().unwrap_or(&track.name)
                    );
                }
                _ => {}
            }
        }
    });

    // Set timestamp anchor to August 1, 2025 to process recent tracks
    let anchor_timestamp = DateTime::parse_from_rfc3339("2025-08-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    scrubber
        .set_timestamp(anchor_timestamp)
        .await
        .expect("Failed to set timestamp anchor");

    log::debug!("Set timestamp anchor to August 1, 2025");

    // Verify initial state
    assert!(
        !scrubber.is_running().await,
        "Scrubber should not be running initially"
    );

    // Get initial timestamp to verify it was set
    let initial_timestamp = scrubber
        .get_current_timestamp()
        .await
        .expect("Failed to get current timestamp")
        .expect("Timestamp should be set");

    assert_eq!(
        initial_timestamp, anchor_timestamp,
        "Timestamp anchor should be set to August 1, 2025"
    );

    // Run one complete processing cycle
    let cycle_result = scrubber.run_processing_cycle().await;

    // The processing may encounter some errors with individual tracks
    match cycle_result {
        Ok(()) => log::debug!("Processing cycle completed successfully"),
        Err(e) => {
            log::warn!("Processing cycle encountered error: {e}");
            // Don't panic - we want to check if any edits were attempted
        }
    }

    // Verify scrubber is not running after cycle completes
    assert!(
        !scrubber.is_running().await,
        "Scrubber should not be running after cycle"
    );

    // Give event handler time to process any remaining events
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify that edits were actually attempted
    let collected_edits = edit_events.lock().await;
    let failed_edits = failed_edit_events.lock().await;

    let total_edit_attempts = collected_edits.len() + failed_edits.len();
    assert!(
        total_edit_attempts > 0,
        "Expected at least one edit attempt (successful or failed), but found none"
    );

    log::info!(
        "Test completed with {} successful edits and {} failed edits",
        collected_edits.len(),
        failed_edits.len()
    );

    // Log details about successful edits
    for (track, edit) in collected_edits.iter() {
        log::info!(
            "Successful edit: {} - {} -> {} - {}",
            track.artist,
            track.name,
            edit.new_artist_name.as_ref().unwrap_or(&track.artist),
            edit.new_track_name.as_ref().unwrap_or(&track.name)
        );
    }

    // Log details about failed edits
    for (track, edit, error) in failed_edits.iter() {
        log::info!(
            "Failed edit: {} - {} -> {} - {} (error: {})",
            track.artist,
            track.name,
            edit.new_artist_name.as_ref().unwrap_or(&track.artist),
            edit.new_track_name.as_ref().unwrap_or(&track.name),
            error
        );
    }
}
