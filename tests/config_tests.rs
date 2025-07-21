use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::Args;

#[test]
fn test_default_config() {
    let config = ScrobbleScrubberConfig::default();
    assert_eq!(config.scrubber.interval, 300);
    assert_eq!(config.scrubber.max_tracks, 1000);
    assert!(!config.scrubber.dry_run);
    assert!(!config.scrubber.require_confirmation);
    assert!(config.scrubber.require_proposed_rule_confirmation);
    assert!(!config.scrubber.enable_web_interface);
    assert_eq!(config.scrubber.web_port, 8080);
    assert!(config.providers.enable_rewrite_rules);
    assert!(!config.providers.enable_openai);
}

#[test]
fn test_config_merge_args() {
    let mut config = ScrobbleScrubberConfig::default();
    let args = Args {
        interval: Some(600),
        max_tracks: Some(50),
        dry_run: true,
        state_file: Some("custom.db".to_string()),
        config: None,
        require_confirmation: false,
        require_proposed_rule_confirmation: false,
        enable_web_interface: false,
        web_port: None,
        lastfm_username: None,
        lastfm_password: None,
        enable_openai: false,
        openai_api_key: None,
    };

    config = config.merge_args(&args);

    assert_eq!(config.scrubber.interval, 600);
    assert_eq!(config.scrubber.max_tracks, 50);
    assert!(config.scrubber.dry_run);
    assert_eq!(config.storage.state_file, "custom.db");
}

#[test]
fn test_get_default_config_paths() {
    let paths = ScrobbleScrubberConfig::get_default_config_paths();

    // Should always include current directory paths
    assert!(paths.iter().any(|p| p.ends_with("scrobble-scrubber.toml")));
    assert!(paths
        .iter()
        .any(|p| p.ends_with("config/scrobble-scrubber.toml")));

    // Should include at least a few paths
    assert!(paths.len() >= 4);
}

#[test]
fn test_get_preferred_config_path() {
    let preferred = ScrobbleScrubberConfig::get_preferred_config_path();

    // Should return a path (unless in a very unusual environment)
    if let Some(path) = preferred {
        assert!(path.ends_with("scrobble-scrubber/config.toml"));
    }
}
