use crate::musicbrainz_client::MusicBrainzClient;
use crate::persistence::{PendingEdit, PendingRewriteRule};
use crate::scrub_action_provider::{
    ActionProviderError, ScrubActionProvider, SuggestionWithContext,
};
use async_trait::async_trait;
use lastfm_edit::{ScrobbleEdit, Track};

/// Provider that suggests moving tracks to their earliest known release
/// using MusicBrainz data to find the original/canonical album
pub struct CompilationToCanonicalProvider {
    client: MusicBrainzClient,
    enabled: bool,
    #[allow(dead_code)]
    confidence_threshold: f32,
}

impl CompilationToCanonicalProvider {
    /// Create a new compilation-to-canonical provider with default settings
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: MusicBrainzClient::new(0.8, 10),
            enabled: true,
            confidence_threshold: 0.8,
        }
    }

    /// Create a provider with custom confidence threshold
    #[must_use]
    pub fn with_confidence_threshold(confidence_threshold: f32) -> Self {
        Self {
            client: MusicBrainzClient::new(confidence_threshold, 10),
            enabled: true,
            confidence_threshold,
        }
    }

    /// Enable or disable the provider
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Default for CompilationToCanonicalProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScrubActionProvider for CompilationToCanonicalProvider {
    type Error = ActionProviderError;

    fn provider_name(&self) -> &str {
        "CompilationToCanonical"
    }

    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        _pending_edits: Option<&[PendingEdit]>,
        _pending_rules: Option<&[PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<SuggestionWithContext>)>, Self::Error> {
        if !self.enabled {
            return Ok(vec![]);
        }

        let mut results = Vec::new();

        for (index, track) in tracks.iter().enumerate() {
            // Skip if no album information
            let Some(current_album) = &track.album else {
                continue;
            };

            log::debug!(
                "Looking for earliest release of '{}' by '{}' (currently on '{}')",
                track.name,
                track.artist,
                current_album
            );

            // Try to find the earliest release for this recording
            match self
                .client
                .find_earliest_release_for_recording(&track.artist, &track.name, current_album)
                .await
            {
                Ok(Some(earliest_album)) if earliest_album != *current_album => {
                    log::info!(
                        "Found earlier release for '{}' by '{}': '{}' (was '{}')",
                        track.name,
                        track.artist,
                        earliest_album,
                        current_album
                    );

                    // Create a ScrobbleEdit that changes only the album
                    let edit = ScrobbleEdit::with_minimal_info(
                        &track.name,
                        &track.artist,
                        &earliest_album,
                        track.timestamp.unwrap_or(0),
                    );

                    let suggestion = SuggestionWithContext::edit_with_confirmation(
                        edit,
                        true, // Always require confirmation for album corrections
                        self.provider_name().to_string(),
                    );

                    results.push((index, vec![suggestion]));
                }
                Ok(Some(_)) => {
                    log::debug!(
                        "Track '{}' by '{}' - already on earliest known release",
                        track.name,
                        track.artist
                    );
                }
                Ok(None) => {
                    log::debug!(
                        "No alternative releases found for '{}' by '{}'",
                        track.name,
                        track.artist
                    );
                }
                Err(e) => {
                    log::warn!(
                        "Error finding earliest release for '{}' by '{}': {}",
                        track.name,
                        track.artist,
                        e
                    );
                }
            }

            // Add a small delay to be respectful to MusicBrainz API
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        log::debug!(
            "CompilationToCanonical analyzed {} tracks, found {} suggestions",
            tracks.len(),
            results.len()
        );

        Ok(results)
    }
}
