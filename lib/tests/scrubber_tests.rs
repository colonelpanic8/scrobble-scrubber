use lastfm_edit::LastFmEditClientImpl;
use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::persistence::MemoryStorage;
use scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
use scrobble_scrubber::scrubber::ScrobbleScrubber;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_scrubber_creation() {
    // Create a dummy client (won't actually be used in this test)
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClientImpl::new(Box::new(http_client));

    // Set up storage
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));

    // Set up action provider (empty rules for this test)
    let rules_state = scrobble_scrubber::persistence::RewriteRulesState {
        rewrite_rules: vec![],
    };
    let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);

    // Create configuration
    let config = ScrobbleScrubberConfig::default();

    // Create scrubber with concrete client
    let scrubber = ScrobbleScrubber::new(storage.clone(), client, action_provider, config);

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
    let client = LastFmEditClientImpl::new(Box::new(http_client));
    let storage = Arc::new(Mutex::new(MemoryStorage::new()));
    let rules_state = scrobble_scrubber::persistence::RewriteRulesState {
        rewrite_rules: vec![],
    };
    let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);
    let config = ScrobbleScrubberConfig::default();

    let scrubber = ScrobbleScrubber::new(storage, client, action_provider, config);

    // Test that scrubber is not running initially
    assert!(!scrubber.is_running().await);
}
