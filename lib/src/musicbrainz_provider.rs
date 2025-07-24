use crate::persistence::{PendingEdit, PendingRewriteRule};
use crate::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};
use async_trait::async_trait;
use lastfm_edit::{ScrobbleEdit, Track};
use log::{trace, warn};
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
struct MusicBrainzMatch {
    artist: String,
    title: String,
    album: Option<String>,
    confidence: f32,
    mbid: String,
}

impl MusicBrainzScrubActionProvider {
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.8, // Only suggest corrections if we're 80%+ confident
            max_results: 5,            // Check top 5 results
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_confidence_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    pub fn with_max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results;
        self
    }

    /// Search MusicBrainz for a track and return the best match
    async fn search_musicbrainz(&self, track: &Track) -> Option<MusicBrainzMatch> {
        let search_key = format!("{}:{}", track.artist, track.name);

        // Check cache first
        if let Ok(cache_read) = self.cache.read() {
            if let Some(cached_result) = cache_read.get(&search_key) {
                trace!("Using cached MusicBrainz result for '{search_key}'");
                return cached_result.clone();
            }
        }

        trace!(
            "Searching MusicBrainz for: '{}' by '{}'",
            track.name,
            track.artist
        );

        // Search for recordings (tracks) matching the artist and title
        let query = format!(
            "recording:\"{}\" AND artist:\"{}\"",
            track.name, track.artist
        );

        let search_results = match Recording::search(query).execute().await {
            Ok(results) => results,
            Err(e) => {
                warn!(
                    "MusicBrainz search failed for '{}' by '{}': {}",
                    track.name, track.artist, e
                );
                if let Ok(mut cache_write) = self.cache.write() {
                    cache_write.insert(search_key, None);
                }
                return None;
            }
        };

        trace!(
            "Found {} MusicBrainz results",
            search_results.entities.len()
        );

        let mut best_match: Option<MusicBrainzMatch> = None;
        let mut best_confidence = 0.0;

        for (i, recording) in search_results
            .entities
            .iter()
            .take(self.max_results)
            .enumerate()
        {
            if let Some(artist_credit) = &recording.artist_credit {
                let mb_artist = artist_credit
                    .first()
                    .map(|ac| ac.artist.name.clone())
                    .unwrap_or_default();

                let mb_title = recording.title.clone();
                let mb_album = recording
                    .releases
                    .as_ref()
                    .and_then(|releases| releases.first())
                    .map(|release| release.title.clone());

                // Calculate confidence based on string similarity
                let artist_confidence = self.calculate_similarity(&track.artist, &mb_artist);
                let title_confidence = self.calculate_similarity(&track.name, &mb_title);
                let overall_confidence = (artist_confidence + title_confidence) / 2.0;

                trace!(
                    "MusicBrainz result {}: '{}' by '{}' (confidence: {:.2})",
                    i + 1,
                    mb_title,
                    mb_artist,
                    overall_confidence
                );

                if overall_confidence > best_confidence
                    && overall_confidence >= self.confidence_threshold
                {
                    best_confidence = overall_confidence;
                    best_match = Some(MusicBrainzMatch {
                        artist: mb_artist,
                        title: mb_title,
                        album: mb_album,
                        confidence: overall_confidence,
                        mbid: recording.id.clone(),
                    });
                }
            }
        }

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

    /// Check if the MusicBrainz match suggests any corrections
    fn suggest_corrections(
        &self,
        track: &Track,
        mb_match: &MusicBrainzMatch,
    ) -> Option<ScrubActionSuggestion> {
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
            return Some(ScrubActionSuggestion::NoAction);
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

        Some(ScrubActionSuggestion::Edit(edit))
    }
}

impl Default for MusicBrainzScrubActionProvider {
    fn default() -> Self {
        Self::new()
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
    ) -> Result<Vec<(usize, Vec<ScrubActionSuggestion>)>, Self::Error> {
        let mut results = Vec::new();

        for (index, track) in tracks.iter().enumerate() {
            trace!(
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

        trace!(
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
        let provider = MusicBrainzScrubActionProvider::new();

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
        let provider = MusicBrainzScrubActionProvider::new();
        assert_eq!(provider.provider_name(), "MusicBrainzScrubActionProvider");

        // Test with empty tracks
        let tracks = vec![];
        let result = provider.analyze_tracks(&tracks, None, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }
}
