use scrobble_scrubber::config::ScrobbleScrubberConfig;

#[test_log::test]
fn should_create_valid_default_configuration() {
    let config = ScrobbleScrubberConfig::default();
    assert_eq!(config.scrubber.interval, 300);
    assert!(!config.scrubber.dry_run);
    assert!(!config.scrubber.require_confirmation);
    assert!(config.scrubber.require_proposed_rule_confirmation);
    assert!(config.providers.enable_rewrite_rules);
    assert!(!config.providers.enable_openai);
}

#[test_log::test]
fn should_return_valid_default_config_paths() {
    let paths = ScrobbleScrubberConfig::get_default_config_paths();

    // Should always include current directory paths
    assert!(paths.iter().any(|p| p.ends_with("scrobble-scrubber.toml")));
    assert!(paths
        .iter()
        .any(|p| p.ends_with("config/scrobble-scrubber.toml")));

    // Should include at least a few paths
    assert!(paths.len() >= 4);
}

#[test_log::test]
fn should_return_preferred_config_path_when_exists() {
    let preferred = ScrobbleScrubberConfig::get_preferred_config_path();

    // Should return a path (unless in a very unusual environment)
    if let Some(path) = preferred {
        assert!(path.ends_with("scrobble-scrubber/config.toml"));
    }
}
