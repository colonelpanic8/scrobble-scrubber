/// Common test utilities and macros

/// Macro to skip live MusicBrainz tests when the environment variable is set
#[macro_export]
macro_rules! skip_if_live_mb_disabled {
    () => {
        if std::env::var("SCROBBLE_SCRUBBER_SKIP_LIVE_MB_TESTS")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
        {
            log::warn!(
                "Skipping live MusicBrainz test (unset SCROBBLE_SCRUBBER_SKIP_LIVE_MB_TESTS to run)"
            );
            return;
        }
    };
}

/// Helper function version of the macro for cases where a function is preferred
#[allow(dead_code)]
pub fn should_skip_live_mb_tests() -> bool {
    std::env::var("SCROBBLE_SCRUBBER_SKIP_LIVE_MB_TESTS")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}
