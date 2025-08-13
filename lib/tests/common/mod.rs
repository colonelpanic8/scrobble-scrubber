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

/// Macro to define a MusicBrainz live test that can be skipped via environment variable
/// 
/// Usage:
/// ```rust
/// mb_live_test!(
///     async fn test_name() {
///         // test code
///     }
/// );
/// ```
/// 
/// This expands to:
/// ```rust
/// #[test_log::test(tokio::test)]
/// async fn test_name() {
///     skip_if_live_mb_disabled!();
///     // test code
/// }
/// ```
#[macro_export]
macro_rules! mb_live_test {
    (
        async fn $name:ident() $body:block
    ) => {
        #[test_log::test(tokio::test)]
        async fn $name() {
            skip_if_live_mb_disabled!();
            $body
        }
    };
    (
        fn $name:ident() $body:block
    ) => {
        #[test]
        fn $name() {
            skip_if_live_mb_disabled!();
            $body
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
