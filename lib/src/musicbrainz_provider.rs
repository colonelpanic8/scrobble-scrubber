use crate::persistence::{PendingEdit, PendingRewriteRule};
use crate::scrub_action_provider::{ScrubActionProvider, SuggestionWithContext};
use async_trait::async_trait;
use lastfm_edit::{ScrobbleEdit, Track};
use log::warn;
use musicbrainz_rs::entity::recording::RecordingSearchQuery;
use std::collections::HashMap;
use std::sync::RwLock;

use musicbrainz_rs::entity::recording::Recording;
use musicbrainz_rs::Search;

/// MusicBrainz-based scrub action provider that suggests corrections using the MusicBrainz database
pub struct MusicBrainzScrubActionProvider {
    confidence_threshold: f32,
    max_results: usize,
    cache: RwLock<HashMap<String, Option<MusicBrainzMatch>>>,
}

#[derive(Debug, Clone)]
pub struct MusicBrainzMatch {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub confidence: f32,
    pub mbid: String,
}

impl MusicBrainzScrubActionProvider {
    /// Select the best release from a recording's releases and return its title
    fn select_best_release_title(recording: &Recording) -> Option<String> {
        recording
            .releases
            .as_ref()
            .and_then(|releases| releases.first())
            .map(|release| release.title.clone())
    }

    /// Build a MusicBrainz search query for a track
    fn build_track_query(artist: &str, title: &str, album: Option<&str>) -> String {
        if let Some(album_name) = album {
            RecordingSearchQuery::query_builder()
                .recording(title)
                .and()
                .artist(artist)
                .and()
                .release(album_name)
                .build()
        } else {
            RecordingSearchQuery::query_builder()
                .recording(title)
                .and()
                .artist(artist)
                .build()
        }
    }
    pub fn new(confidence_threshold: f32, max_results: usize) -> Self {
        Self {
            confidence_threshold,
            max_results,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Search MusicBrainz for multiple tracks and return all matches with confidence scores
    pub async fn search_musicbrainz_multiple(
        &self,
        artist: &str,
        title: &str,
        album: Option<&str>,
    ) -> Result<Vec<MusicBrainzMatch>, Box<dyn std::error::Error + Send + Sync>> {
        log::debug!("Searching MusicBrainz for: '{title}' by '{artist}'");

        // Build MusicBrainz search query
        let query_string = Self::build_track_query(artist, title, album);

        // Perform the search
        let search_results = Recording::search(query_string)
            .execute()
            .await
            .map_err(|e| format!("MusicBrainz search failed: {e}"))?;

        log::debug!(
            "Found {} MusicBrainz results",
            search_results.entities.len()
        );

        let mut results = Vec::new();

        for recording in search_results.entities.iter().take(self.max_results) {
            if let Some(artist_credit) = &recording.artist_credit {
                let mb_artist = artist_credit
                    .first()
                    .map(|ac| ac.artist.name.clone())
                    .unwrap_or_default();

                let mb_title = recording.title.clone();
                let mb_album = Self::select_best_release_title(recording);

                // Calculate confidence based on string similarity
                let artist_confidence = self.calculate_similarity(artist, &mb_artist);
                let title_confidence = self.calculate_similarity(title, &mb_title);
                let overall_confidence = (artist_confidence + title_confidence) / 2.0;

                results.push(MusicBrainzMatch {
                    artist: mb_artist,
                    title: mb_title,
                    album: mb_album,
                    confidence: overall_confidence,
                    mbid: recording.id.clone(),
                });
            }
        }

        // Sort by confidence (highest first)
        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// Search MusicBrainz for a track and return the best match
    async fn search_musicbrainz(&self, track: &Track) -> Option<MusicBrainzMatch> {
        let search_key = format!("{}:{}", track.artist, track.name);

        // Check cache first
        if let Ok(cache_read) = self.cache.read() {
            if let Some(cached_result) = cache_read.get(&search_key) {
                log::trace!("Using cached MusicBrainz result for '{search_key}'");
                return cached_result.clone();
            }
        }

        // Use the multiple search function to get all matches
        match self
            .search_musicbrainz_multiple(&track.artist, &track.name, track.album.as_deref())
            .await
        {
            Ok(mut matches) => {
                // Filter by confidence threshold and find the best match
                matches.retain(|m| m.confidence >= self.confidence_threshold);
                let best_match = matches.into_iter().next(); // Already sorted by confidence

                // Cache the result (even if None)
                if let Ok(mut cache_write) = self.cache.write() {
                    cache_write.insert(search_key, best_match.clone());
                }

                if let Some(ref m) = best_match {
                    log::info!(
                        "Found MusicBrainz match for '{}' by '{}': '{}' by '{}' (confidence: {:.2})",
                        track.name,
                        track.artist,
                        m.title,
                        m.artist,
                        m.confidence
                    );
                } else {
                    log::debug!(
                        "No confident MusicBrainz match found for '{}' by '{}'",
                        track.name,
                        track.artist
                    );
                }

                best_match
            }
            Err(e) => {
                warn!(
                    "MusicBrainz search failed for '{}' by '{}': {}",
                    track.name, track.artist, e
                );
                if let Ok(mut cache_write) = self.cache.write() {
                    cache_write.insert(search_key, None);
                }
                None
            }
        }
    }

    /// Calculate string similarity between two strings (simple Levenshtein-based approach)
    fn calculate_similarity(&self, a: &str, b: &str) -> f32 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();

        if a_lower == b_lower {
            return 1.0;
        }

        // Simple similarity calculation based on common characters and length
        let max_len = a_lower.len().max(b_lower.len()) as f32;
        if max_len == 0.0 {
            return 1.0;
        }

        let common_chars = a_lower.chars().filter(|c| b_lower.contains(*c)).count() as f32;

        let length_penalty = (a_lower.len() as i32 - b_lower.len() as i32).abs() as f32 / max_len;

        (common_chars / max_len) - (length_penalty * 0.5)
    }

    /// Suggest corrections based on the MusicBrainz match
    fn suggest_corrections(
        &self,
        track: &Track,
        mb_match: &MusicBrainzMatch,
    ) -> Option<SuggestionWithContext> {
        let mut needs_artist_correction = false;
        let mut needs_title_correction = false;
        let mut needs_album_correction = false;

        // Check if artist needs correction
        if track.artist.trim() != mb_match.artist.trim() {
            needs_artist_correction = true;
        }

        // Check if track title needs correction
        if track.name.trim() != mb_match.title.trim() {
            needs_title_correction = true;
        }

        // Check if album needs correction (if we have album info)
        if let (Some(track_album), Some(mb_album)) = (&track.album, &mb_match.album) {
            if track_album.trim() != mb_album.trim() {
                needs_album_correction = true;
            }
        }

        // If no corrections needed, return no action
        if !needs_artist_correction && !needs_title_correction && !needs_album_correction {
            return Some(SuggestionWithContext::no_action("MusicBrainz".to_string()));
        }

        // Create the corrected edit
        let corrected_artist = if needs_artist_correction {
            mb_match.artist.clone()
        } else {
            track.artist.clone()
        };

        let corrected_title = if needs_title_correction {
            mb_match.title.clone()
        } else {
            track.name.clone()
        };

        let corrected_album = mb_match
            .album
            .clone()
            .or_else(|| track.album.clone())
            .unwrap_or_default();

        let edit = ScrobbleEdit::with_minimal_info(
            &corrected_title,
            &corrected_artist,
            &corrected_album,
            track.timestamp.unwrap_or(0),
        );

        let mut correction_details = Vec::new();
        if needs_artist_correction {
            correction_details.push(format!(
                "artist: '{}' → '{}'",
                track.artist, mb_match.artist
            ));
        }
        if needs_title_correction {
            correction_details.push(format!("title: '{}' → '{}'", track.name, mb_match.title));
        }
        if needs_album_correction && mb_match.album.is_some() {
            correction_details.push(format!(
                "album: '{}' → '{}'",
                track.album.as_deref().unwrap_or("unknown"),
                mb_match.album.as_deref().unwrap_or("unknown")
            ));
        }

        log::info!(
            "MusicBrainz suggests corrections for '{}' by '{}': {} (confidence: {:.2}, mbid: {})",
            track.name,
            track.artist,
            correction_details.join(", "),
            mb_match.confidence,
            mb_match.mbid
        );

        // MusicBrainz suggestions typically don't require confirmation since they're based on authoritative data
        Some(SuggestionWithContext::edit_with_confirmation(
            edit,
            false, // MusicBrainz corrections are generally high-confidence
            "MusicBrainz".to_string(),
        ))
    }
}

impl Default for MusicBrainzScrubActionProvider {
    fn default() -> Self {
        Self::new(0.8, 5) // Default: 80% confidence threshold, 5 max results
    }
}

#[async_trait]
impl ScrubActionProvider for MusicBrainzScrubActionProvider {
    type Error = crate::scrub_action_provider::ActionProviderError;

    fn provider_name(&self) -> &str {
        "MusicBrainzScrubActionProvider"
    }

    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        _pending_edits: Option<&[PendingEdit]>,
        _pending_rules: Option<&[PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<SuggestionWithContext>)>, Self::Error> {
        let mut results = Vec::new();

        for (index, track) in tracks.iter().enumerate() {
            log::trace!(
                "MusicBrainzScrubActionProvider analyzing track {}: '{}' by '{}'",
                index,
                track.name,
                track.artist
            );

            // Search MusicBrainz for this track
            if let Some(mb_match) = self.search_musicbrainz(track).await {
                if let Some(suggestion) = self.suggest_corrections(track, &mb_match) {
                    results.push((index, vec![suggestion]));
                }
            }

            // Add a small delay to be respectful to MusicBrainz API
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        log::trace!(
            "MusicBrainzScrubActionProvider completed analysis of {} tracks, found {} suggestions",
            tracks.len(),
            results.len()
        );

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity_calculation() {
        let provider = MusicBrainzScrubActionProvider::default();

        // Exact match
        assert_eq!(provider.calculate_similarity("Hello", "Hello"), 1.0);

        // Case insensitive
        assert_eq!(provider.calculate_similarity("Hello", "hello"), 1.0);

        // Partial match should be less than 1.0
        let similarity = provider.calculate_similarity("Hello World", "Hello");
        assert!(similarity < 1.0 && similarity > 0.0);
    }

    #[tokio::test]
    async fn test_provider_interface() {
        let provider = MusicBrainzScrubActionProvider::default();
        assert_eq!(provider.provider_name(), "MusicBrainzScrubActionProvider");

        // Test with empty tracks
        let tracks = vec![];
        let result = provider.analyze_tracks(&tracks, None, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }
}
