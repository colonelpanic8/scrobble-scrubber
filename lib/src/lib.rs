pub mod default_rules;
pub mod events;
pub mod page_search;
pub mod rewrite;
pub mod rewrite_processor;
pub mod scrub_action_provider;

#[cfg(feature = "cli")]
pub mod cli;
pub mod config;

// Tokio-dependent modules. These are excluded from non-tokio builds (e.g. WASM bindings which only
// use rewrite rule analysis).
#[cfg(feature = "tokio")]
pub mod edit;
#[cfg(feature = "tokio")]
pub mod event_logger;
#[cfg(feature = "tokio")]
pub mod json_logger;
#[cfg(feature = "tokio")]
pub mod musicbrainz;
#[cfg(feature = "tokio")]
pub mod track_cache;
#[cfg(feature = "tokio")]
pub mod track_provider;

#[cfg(feature = "openai")]
pub mod openai_provider;
pub mod persistence;
#[cfg(feature = "tokio")]
pub mod scrubber;

#[cfg(feature = "cli")]
pub mod recent_user_manager;
#[cfg(feature = "cli")]
pub mod session_manager;

#[cfg(feature = "mirror")]
pub mod mirror;
