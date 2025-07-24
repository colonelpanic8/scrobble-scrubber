pub mod event_logger;
pub mod events;
pub mod json_logger;
pub mod rewrite;
pub mod rewrite_processor;
pub mod scrub_action_provider;
pub mod track_cache;
pub mod track_provider;

pub mod config;
#[cfg(feature = "musicbrainz")]
pub mod musicbrainz_provider;
pub mod openai_provider;
pub mod persistence;
#[cfg(feature = "tokio")]
pub mod scrubber;
#[cfg(feature = "web")]
pub mod web_interface;
