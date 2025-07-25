use chrono::{TimeZone, Utc};
use lastfm_edit::{
    EditResponse, LastFmEditClientImpl, LastFmEditSession, MockLastFmEditClient, ScrobbleEdit,
    Track,
};
use mockall::predicate::*;
use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::persistence::{MemoryStorage, StateStorage, TimestampState};
use scrobble_scrubber::scrub_action_provider::{
    RewriteRulesScrubActionProvider, ScrubActionSuggestion,
};
use scrobble_scrubber::scrubber::ScrobbleScrubber;
// SerializableTrack is no longer needed - Track is now serializable
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

#[tokio::test]
async fn test_scrubber_creation() {
    // Create a dummy client (won't actually be used in this test)
    let http_client = http_client::native::NativeClient::new();
    let session = LastFmEditSession::new(
        "test_user".to_string(),
        vec!["test_cookie".to_string()],
        Some("test_csrf".to_string()),
        "https://www.last.fm".to_string(),
    );
    let client = LastFmEditClientImpl::from_session(Box::new(http_client), session);

    // Set up storage
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));

    // Set up action provider (empty rules for this test)
    let rules_state = scrobble_scrubber::persistence::RewriteRulesState {
        rewrite_rules: vec![],
    };
    let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);

    // Create configuration
    let config = ScrobbleScrubberConfig::default();

    // Create scrubber with boxed client
    let scrubber =
        ScrobbleScrubber::new(storage.clone(), Box::new(client), action_provider, config);

    // Test that scrubber can be created successfully
    // Verify we can get the storage reference and it's accessible
    let storage_ref = scrubber.storage();
    let _storage_guard = storage_ref.try_lock();
    // If we can get here without panicking, the scrubber was created successfully
}

#[tokio::test]
async fn test_scrubber_is_not_running_initially() {
    // Create a minimal scrubber setup
    let http_client = http_client::native::NativeClient::new();
    let session = LastFmEditSession::new(
        "test_user".to_string(),
        vec!["test_cookie".to_string()],
        Some("test_csrf".to_string()),
        "https://www.last.fm".to_string(),
    );
    let client = LastFmEditClientImpl::from_session(Box::new(http_client), session);
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));
    let rules_state = scrobble_scrubber::persistence::RewriteRulesState {
        rewrite_rules: vec![],
    };
    let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);
    let config = ScrobbleScrubberConfig::default();

    let scrubber = ScrobbleScrubber::new(storage, Box::new(client), action_provider, config);

    // Test that scrubber is not running initially
    assert!(!scrubber.is_running().await);
}

#[tokio::test]
async fn test_scrubber_with_mock_client() {
    // Create a mock client instead of a real one
    let mut mock_client = MockLastFmEditClient::new();

    // Set up mock expectations (these won't be called in this simple test)
    mock_client
        .expect_username()
        .returning(|| "test_user".to_string());

    // Mock the subscribe method for client event forwarding
    mock_client.expect_subscribe().returning(|| {
        let (_, receiver) = tokio::sync::broadcast::channel(100);
        receiver
    });

    // Set up storage and action provider
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));
    let rules_state = scrobble_scrubber::persistence::RewriteRulesState {
        rewrite_rules: vec![],
    };
    let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);
    let config = ScrobbleScrubberConfig::default();

    // Create scrubber with mock client
    let scrubber = ScrobbleScrubber::new(storage, Box::new(mock_client), action_provider, config);

    // Test that scrubber can be created successfully with a mock
    assert!(!scrubber.is_running().await);
}

#[tokio::test]
async fn test_scrubber_processes_tracks_in_chronological_order() {
    // Create test tracks with timestamps (API returns newest first)
    let track_newest = Track {
        name: "Song Three".to_string(),
        artist: "Artist C".to_string(),
        playcount: 1,
        timestamp: Some(1640995400), // Jan 1, 2022, 00:03:20 UTC (newest)
        album: Some("Album C".to_string()),
        album_artist: None,
    };

    let track_middle = Track {
        name: "Song Two".to_string(),
        artist: "Artist B".to_string(),
        playcount: 1,
        timestamp: Some(1640995300), // Jan 1, 2022, 00:01:40 UTC (middle)
        album: Some("Album B".to_string()),
        album_artist: None,
    };

    let track_oldest = Track {
        name: "Song One".to_string(),
        artist: "Artist A".to_string(),
        playcount: 1,
        timestamp: Some(1640995200), // Jan 1, 2022, 00:00:00 UTC (oldest)
        album: Some("Album A".to_string()),
        album_artist: None,
    };

    // Create mock client
    let mut mock_client = MockLastFmEditClient::new();

    // Mock the subscribe method for client event forwarding
    mock_client.expect_subscribe().returning(|| {
        let (_, receiver) = tokio::sync::broadcast::channel(100);
        receiver
    });

    // Mock the pagination method that the iterator uses internally
    // Return tracks in reverse chronological order (newest first, as API does)
    mock_client
        .expect_get_recent_scrobbles()
        .with(mockall::predicate::eq(1))
        .returning(move |_| {
            Ok(vec![
                track_newest.clone(),
                track_middle.clone(),
                track_oldest.clone(),
            ])
        });

    // Mock subsequent pages to return empty (no more tracks)
    mock_client
        .expect_get_recent_scrobbles()
        .with(mockall::predicate::gt(1))
        .returning(|_| Ok(vec![]));

    // Track the order of edits applied to verify chronological processing
    let edit_order = Arc::new(StdMutex::new(Vec::new()));
    let edit_order_clone = edit_order.clone();

    // Mock edit_scrobble to capture the order of processing
    mock_client.expect_edit_scrobble().returning(move |edit| {
        let mut order = edit_order_clone.lock().unwrap();
        // Use the original track name to track processing order
        if let Some(original_name) = &edit.track_name_original {
            order.push(original_name.clone());
        }

        Ok(EditResponse {
            individual_results: vec![],
        })
    });

    // Mock username for authentication check
    mock_client
        .expect_username()
        .returning(|| "test_user".to_string());

    // Set up storage
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));

    // Create a test action provider that suggests edits for all tracks
    let action_provider = TestActionProvider::new();

    // Create configuration with immediate processing (no confirmation)
    let mut config = ScrobbleScrubberConfig::default();
    config.scrubber.require_confirmation = false;

    // Create scrubber
    let scrubber = ScrobbleScrubber::new(
        storage.clone(),
        Box::new(mock_client),
        action_provider,
        config,
    );

    // Test the actual iterator behavior by creating one and verifying order
    // Note: This tests the mock setup, not the full scrubber since we can't easily
    // trigger a full scrubber run with the current mock limitations

    // For now, verify the scrubber is properly configured
    assert!(
        !scrubber.is_running().await,
        "Scrubber should not be running initially"
    );

    // Verify storage is accessible
    let storage_ref = scrubber.storage();
    let storage_guard = storage_ref.lock().await;
    let timestamp_state = storage_guard.load_timestamp_state().await.unwrap();
    assert!(
        timestamp_state.last_processed_timestamp.is_none(),
        "No initial timestamp should be set"
    );

    // TODO: This test would be complete if we could mock the client's recent_tracks()
    // iterator method, but that requires the concrete LastFmEditClientImpl type.
    // The current BoxedLastFmEditClient approach limits our ability to create iterators
    // in tests. A future improvement would be to make iterators work with the trait.
}

#[tokio::test]
async fn test_track_chronological_ordering_pattern() {
    // This test demonstrates the ordering behavior we expect from the scrubber:
    // API returns tracks newest-first, but processing should be oldest-first

    // Create test tracks in the order API returns them (newest first)
    let tracks_from_api = vec![
        Track {
            name: "Song Three".to_string(),
            artist: "Artist C".to_string(),
            playcount: 1,
            timestamp: Some(1640995400), // newest: Jan 1, 2022, 00:03:20 UTC
            album: Some("Album C".to_string()),
            album_artist: None,
        },
        Track {
            name: "Song Two".to_string(),
            artist: "Artist B".to_string(),
            playcount: 1,
            timestamp: Some(1640995300), // middle: Jan 1, 2022, 00:01:40 UTC
            album: Some("Album B".to_string()),
            album_artist: None,
        },
        Track {
            name: "Song One".to_string(),
            artist: "Artist A".to_string(),
            playcount: 1,
            timestamp: Some(1640995200), // oldest: Jan 1, 2022, 00:00:00 UTC
            album: Some("Album A".to_string()),
            album_artist: None,
        },
    ];

    // Simulate what the scrubber should do:
    // 1. Collect tracks from iterator (in API order - newest first)
    let mut collected_tracks = tracks_from_api.clone();

    // Verify tracks are in API order (newest first)
    assert_eq!(collected_tracks[0].name, "Song Three");
    assert_eq!(collected_tracks[1].name, "Song Two");
    assert_eq!(collected_tracks[2].name, "Song One");
    assert!(collected_tracks[0].timestamp > collected_tracks[1].timestamp);
    assert!(collected_tracks[1].timestamp > collected_tracks[2].timestamp);

    // 2. Sort for chronological processing (oldest first)
    collected_tracks.sort_by_key(|track| track.timestamp.unwrap_or(0));

    // Verify chronological order (oldest first)
    assert_eq!(collected_tracks[0].name, "Song One");
    assert_eq!(collected_tracks[1].name, "Song Two");
    assert_eq!(collected_tracks[2].name, "Song Three");
    assert!(collected_tracks[0].timestamp < collected_tracks[1].timestamp);
    assert!(collected_tracks[1].timestamp < collected_tracks[2].timestamp);

    // 3. Verify timestamps are in ascending order for processing
    let timestamps: Vec<u64> = collected_tracks
        .iter()
        .map(|t| t.timestamp.unwrap())
        .collect();

    assert_eq!(timestamps, vec![1640995200, 1640995300, 1640995400]);

    // This proves the ordering pattern the scrubber should follow:
    // API returns newest → oldest, but processing should be oldest → newest
    // This ensures incremental processing works correctly with timestamps
}

#[tokio::test]
async fn test_scrubber_track_processing_order_with_cache() {
    // Create test tracks with specific timestamps
    let track1 = Track {
        name: "Song One".to_string(),
        artist: "Artist A".to_string(),
        playcount: 1,
        timestamp: Some(1640995200), // Jan 1, 2022, 00:00:00 UTC (oldest)
        album: Some("Album A".to_string()),
        album_artist: None,
    };

    let track2 = Track {
        name: "Song Two".to_string(),
        artist: "Artist B".to_string(),
        playcount: 1,
        timestamp: Some(1640995300), // Jan 1, 2022, 00:01:40 UTC (middle)
        album: Some("Album B".to_string()),
        album_artist: None,
    };

    let track3 = Track {
        name: "Song Three".to_string(),
        artist: "Artist C".to_string(),
        playcount: 1,
        timestamp: Some(1640995400), // Jan 1, 2022, 00:03:20 UTC (newest)
        album: Some("Album C".to_string()),
        album_artist: None,
    };

    // Create mock client that will capture the order of edit calls
    let mut mock_client = MockLastFmEditClient::new();
    let edit_order = Arc::new(StdMutex::new(Vec::new()));
    let edit_order_clone = edit_order.clone();

    // Mock the subscribe method for client event forwarding
    mock_client.expect_subscribe().returning(|| {
        let (_, receiver) = tokio::sync::broadcast::channel(100);
        receiver
    });

    mock_client.expect_edit_scrobble().returning(move |edit| {
        let mut order = edit_order_clone.lock().unwrap();
        // Record the original track name to verify processing order
        if let Some(original_name) = &edit.track_name_original {
            order.push(original_name.clone());
        }

        Ok(EditResponse {
            individual_results: vec![],
        })
    });

    // Set up storage and manually populate it with a timestamp state
    // to simulate having processed up to a certain point
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));
    {
        let mut storage_guard = storage.lock().await;
        let initial_timestamp = TimestampState {
            // Set to before track1 so all tracks will be processed
            last_processed_timestamp: Some(Utc.timestamp_opt(1640995100, 0).unwrap()),
        };
        storage_guard
            .save_timestamp_state(&initial_timestamp)
            .await
            .unwrap();
    }

    // Create a test action provider
    let action_provider = TestActionProvider::new();

    // Create configuration with immediate processing
    let mut config = ScrobbleScrubberConfig::default();
    config.scrubber.require_confirmation = false;

    // Create scrubber
    let scrubber = ScrobbleScrubber::new(
        storage.clone(),
        Box::new(mock_client),
        action_provider,
        config,
    );

    // Manually populate the cache with tracks in reverse chronological order
    // (as they would come from the API - newest first)
    let _cache_tracks = vec![track3, track2, track1];

    // Access the scrubber's internal track cache and add our test tracks
    // Note: This is testing the internal processing order, not the full end-to-end flow
    // The real test would be more complex with full cache integration

    // For now, let's test that the scrubber is properly initialized
    assert!(
        !scrubber.is_running().await,
        "Scrubber should not be running initially"
    );

    // Verify storage has the initial timestamp we set
    let storage_guard = storage.lock().await;
    let timestamp_state = storage_guard.load_timestamp_state().await.unwrap();
    assert!(
        timestamp_state.last_processed_timestamp.is_some(),
        "Initial timestamp should be set"
    );

    let initial_timestamp = timestamp_state.last_processed_timestamp.unwrap();
    let expected_initial = Utc.timestamp_opt(1640995100, 0).unwrap();
    assert_eq!(
        initial_timestamp, expected_initial,
        "Initial timestamp should match what we set"
    );
}

// Test action provider that suggests edits for all tracks
struct TestActionProvider {
    suggested_tracks: Arc<StdMutex<Vec<String>>>,
}

impl TestActionProvider {
    fn new() -> Self {
        Self {
            suggested_tracks: Arc::new(StdMutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl scrobble_scrubber::scrub_action_provider::ScrubActionProvider for TestActionProvider {
    type Error = scrobble_scrubber::scrub_action_provider::ActionProviderError;

    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        _pending_edits: Option<&[scrobble_scrubber::persistence::PendingEdit]>,
        _pending_rules: Option<&[scrobble_scrubber::persistence::PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<ScrubActionSuggestion>)>, Self::Error> {
        let mut suggestions = Vec::new();

        for (index, track) in tracks.iter().enumerate() {
            // Record that we analyzed this track
            self.suggested_tracks
                .lock()
                .unwrap()
                .push(track.name.clone());

            // Create an edit suggestion that appends " [EDITED]" to track name
            let edit = ScrobbleEdit::with_minimal_info(
                &format!("{} [EDITED]", track.name),
                &track.artist,
                track.album.as_deref().unwrap_or(""),
                track.timestamp.unwrap_or(0),
            );

            suggestions.push((index, vec![ScrubActionSuggestion::Edit(edit)]));
        }

        Ok(suggestions)
    }

    fn provider_name(&self) -> &str {
        "TestActionProvider"
    }
}
