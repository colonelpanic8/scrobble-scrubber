use crate::persistence::{PendingEdit, PendingRewriteRule};
use crate::scrub_action_provider::{ScrubActionProvider, SuggestionWithContext};
use async_trait::async_trait;
use lastfm_edit::{ScrobbleEdit, Track};
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
    prefer_non_japanese_releases: bool,
}

#[derive(Debug, Clone)]
pub struct MusicBrainzMatch {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub confidence: f32,
    pub mbid: String,
    pub release_id: Option<String>,
}

impl MusicBrainzScrubActionProvider {
    /// Check if a release has special edition markers in its disambiguation
    pub fn is_special_edition(release: &musicbrainz_rs::entity::release::Release) -> bool {
        release
            .disambiguation
            .as_ref()
            .map(|d| {
                let d_lower = d.to_lowercase();
                d_lower.contains("deluxe")
                    || d_lower.contains("legacy")
                    || d_lower.contains("expanded")
                    || d_lower.contains("anniversary")
                    || d_lower.contains("special")
                    || d_lower.contains("bonus")
            })
            .unwrap_or(false)
    }

    /// Compare releases by date for sorting (earliest first)
    fn compare_release_dates(
        a: &musicbrainz_rs::entity::release::Release,
        b: &musicbrainz_rs::entity::release::Release,
    ) -> std::cmp::Ordering {
        match (&a.date, &b.date) {
            (Some(date_a), Some(date_b)) => {
                // Extract year from date strings for comparison
                let year_a = date_a.0.get(..4).unwrap_or("9999");
                let year_b = date_b.0.get(..4).unwrap_or("9999");
                year_a.cmp(year_b)
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    }

    /// Get the date string for a release (for logging)
    fn get_release_date_str(release: &musicbrainz_rs::entity::release::Release) -> &str {
        release
            .date
            .as_ref()
            .map(|d| d.0.as_str())
            .unwrap_or("unknown")
    }

    /// Select the canonical release from a list of releases
    pub fn select_canonical_release(
        releases: &[musicbrainz_rs::entity::release::Release],
        prefer_non_japanese: bool,
    ) -> Option<&musicbrainz_rs::entity::release::Release> {
        if releases.is_empty() {
            return None;
        }

        // Sort releases by date
        let mut sorted_releases: Vec<_> = releases.iter().collect();
        sorted_releases.sort_by(|a, b| Self::compare_release_dates(a, b));

        // Get earliest year
        let earliest_year = sorted_releases
            .first()
            .and_then(|r| r.date.as_ref())
            .and_then(|d| d.0.get(..4))
            .and_then(|y| y.parse::<i32>().ok())
            .unwrap_or(9999);

        // Find contemporary releases (within 1 year of earliest)
        let contemporary: Vec<_> = sorted_releases
            .iter()
            .filter(|r| {
                let year = r
                    .date
                    .as_ref()
                    .and_then(|d| d.0.get(..4))
                    .and_then(|y| y.parse::<i32>().ok())
                    .unwrap_or(9999);
                (year - earliest_year).abs() <= 1
            })
            .copied()
            .collect();

        if prefer_non_japanese {
            // Check if there are any non-Japanese releases available
            let has_non_jp = contemporary
                .iter()
                .any(|r| r.country.as_ref().map(|c| c != "JP").unwrap_or(true));

            if has_non_jp {
                // First pass: non-special, non-Japanese releases
                if let Some(release) = contemporary.iter().find(|r| {
                    let is_japanese = r.country.as_ref().map(|c| c == "JP").unwrap_or(false);
                    !Self::is_special_edition(r) && !is_japanese
                }) {
                    return Some(release);
                }
            }
        }

        // Second pass: any non-special edition
        if let Some(release) = contemporary.iter().find(|r| !Self::is_special_edition(r)) {
            return Some(release);
        }

        // If all are special editions, take the earliest
        contemporary
            .first()
            .copied()
            .or_else(|| sorted_releases.first().copied())
    }

    /// Select the best matching release for a specific album
    pub fn select_matching_album_release(
        releases: &[musicbrainz_rs::entity::release::Release],
        album: &str,
        prefer_non_japanese: bool,
    ) -> Option<(String, String)> {
        // Collect and sort matching releases by date
        let mut matching: Vec<_> = releases
            .iter()
            .filter(|r| r.title.eq_ignore_ascii_case(album))
            .collect();

        if matching.is_empty() {
            return None;
        }

        matching.sort_by(|a, b| Self::compare_release_dates(a, b));

        // First try to find a non-special edition, preferring non-Japanese releases if enabled
        if prefer_non_japanese {
            // Check if there are any non-Japanese releases available
            let has_non_jp = matching
                .iter()
                .any(|r| r.country.as_ref().map(|c| c != "JP").unwrap_or(true));

            if has_non_jp {
                // First pass: non-special, non-Japanese releases
                for release in &matching {
                    let is_japanese = release.country.as_ref().map(|c| c == "JP").unwrap_or(false);
                    if !Self::is_special_edition(release) && !is_japanese {
                        log::debug!(
                            "Selected non-Japanese release '{}' from {} (country: {:?}, disamb: {:?})",
                            release.title,
                            Self::get_release_date_str(release),
                            release.country,
                            release.disambiguation
                        );
                        return Some((release.title.clone(), release.id.clone()));
                    }
                }
            }
        }

        // Second pass: any non-special edition (including Japanese if no alternatives)
        for release in &matching {
            if !Self::is_special_edition(release) {
                log::debug!(
                    "Selected release '{}' from {} (country: {:?}, disamb: {:?})",
                    release.title,
                    Self::get_release_date_str(release),
                    release.country,
                    release.disambiguation
                );
                return Some((release.title.clone(), release.id.clone()));
            }
        }

        // If all are special editions, take the earliest
        if let Some(earliest) = matching.first() {
            log::debug!(
                "All releases have special edition markers, selecting earliest: '{}' from {} (disamb: {:?})",
                earliest.title,
                Self::get_release_date_str(earliest),
                earliest.disambiguation
            );
            return Some((earliest.title.clone(), earliest.id.clone()));
        }

        None
    }

    /// Select the best release from a recording's releases
    /// Prioritizes releases matching the desired album if provided
    /// When multiple matches exist, prefers the earliest release (closest to original)
    fn select_best_release(
        &self,
        recording: &Recording,
        desired_album: Option<&str>,
    ) -> Option<(String, String)> {
        let releases = recording.releases.as_ref()?;

        // If we have a desired album, try to find a matching release
        if let Some(album) = desired_album {
            if let Some(result) = Self::select_matching_album_release(
                releases,
                album,
                self.prefer_non_japanese_releases,
            ) {
                return Some(result);
            }
        }

        // No desired album or no matches found - pick the earliest release overall
        // Apply the same Japanese release preference here
        if self.prefer_non_japanese_releases {
            // Check if there are non-Japanese releases
            let has_non_jp = releases
                .iter()
                .any(|r| r.country.as_ref().map(|c| c != "JP").unwrap_or(true));

            if has_non_jp {
                // Sort non-Japanese releases by date
                let mut non_jp_releases: Vec<_> = releases
                    .iter()
                    .filter(|r| r.country.as_ref().map(|c| c != "JP").unwrap_or(true))
                    .collect();

                if !non_jp_releases.is_empty() {
                    non_jp_releases.sort_by(|a, b| Self::compare_release_dates(a, b));
                    if let Some(earliest) = non_jp_releases.first() {
                        log::debug!(
                            "Selected non-Japanese release '{}' from {} (country: {:?})",
                            earliest.title,
                            Self::get_release_date_str(earliest),
                            earliest.country
                        );
                        return Some((earliest.title.clone(), earliest.id.clone()));
                    }
                }
            }
        }

        // Fallback: pick the earliest release overall (including Japanese)
        let mut all_releases: Vec<_> = releases.iter().collect();
        if all_releases.is_empty() {
            return None;
        }

        all_releases.sort_by(|a, b| Self::compare_release_dates(a, b));

        let earliest = all_releases.first()?;
        Some((earliest.title.clone(), earliest.id.clone()))
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
            prefer_non_japanese_releases: true, // Default to preferring non-Japanese releases
        }
    }

    /// Set whether to prefer non-Japanese releases when multiple are available
    pub fn with_japanese_preference(mut self, prefer_non_japanese: bool) -> Self {
        self.prefer_non_japanese_releases = prefer_non_japanese;
        self
    }

    /// Get the current Japanese release preference setting
    pub fn prefer_non_japanese_releases(&self) -> bool {
        self.prefer_non_japanese_releases
    }

    /// Search for album releases by artist and optional album filter
    pub async fn search_album_releases(
        artist: &str,
        album_filter: Option<&str>,
    ) -> Result<
        Vec<musicbrainz_rs::entity::release::Release>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        use musicbrainz_rs::entity::release::{Release, ReleaseSearchQuery};
        use musicbrainz_rs::Search;

        let query = if let Some(album) = album_filter {
            ReleaseSearchQuery::query_builder()
                .release(album)
                .and()
                .artist(artist)
                .build()
        } else {
            ReleaseSearchQuery::query_builder().artist(artist).build()
        };

        log::debug!("Searching for albums with query: {}", query);

        let search_results = Release::search(query)
            .execute()
            .await
            .map_err(|e| format!("Album search failed: {e}"))?;

        // Sort releases by date
        let mut releases = search_results.entities;
        releases.sort_by(Self::compare_release_dates);

        Ok(releases)
    }

    /// Verify that a track exists in MusicBrainz with the given metadata
    /// This is used for MusicBrainz confirmation in rewrite rules
    /// IMPORTANT: This checks if the track exists on a CANONICAL release,
    /// not just any release (e.g., excludes Japanese bonus track releases)
    pub async fn verify_track_exists_on_canonical_release(
        &self,
        artist: &str,
        title: &str,
        album: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        use musicbrainz_rs::entity::release::Release;
        use musicbrainz_rs::entity::release::ReleaseSearchQuery;
        use musicbrainz_rs::Search;

        log::debug!(
            "MB verify: Checking if track exists on canonical release - '{}' by '{}' [{}]",
            title,
            artist,
            album.unwrap_or("No Album")
        );

        if let Some(desired_album) = album {
            // First, find all releases of this album by this artist
            let album_query = ReleaseSearchQuery::query_builder()
                .release(desired_album)
                .and()
                .artist(artist)
                .build();

            log::debug!("Searching for album releases with query: {}", album_query);

            let album_search = Release::search(album_query)
                .execute()
                .await
                .map_err(|e| format!("Album search failed: {e}"))?;

            if album_search.entities.is_empty() {
                log::debug!(
                    "No album releases found for '{}' by '{}'",
                    desired_album,
                    artist
                );
                return Ok(false);
            }

            // Sort releases by date to find the canonical one
            let mut releases = album_search.entities;
            releases.sort_by(|a, b| {
                let a_date = a.date.as_ref().map(|d| d.0.as_str()).unwrap_or("9999");
                let b_date = b.date.as_ref().map(|d| d.0.as_str()).unwrap_or("9999");
                a_date.cmp(b_date)
            });

            // Select the canonical release
            let canonical_release = if self.prefer_non_japanese_releases {
                // Get earliest release year
                let earliest_year = releases
                    .first()
                    .and_then(|r| r.date.as_ref())
                    .and_then(|d| d.0.get(..4))
                    .and_then(|y| y.parse::<i32>().ok())
                    .unwrap_or(9999);

                // Find contemporary releases (within 1 year of earliest)
                let contemporary: Vec<_> = releases
                    .iter()
                    .filter(|r| {
                        let year = r
                            .date
                            .as_ref()
                            .and_then(|d| d.0.get(..4))
                            .and_then(|y| y.parse::<i32>().ok())
                            .unwrap_or(9999);
                        (year - earliest_year).abs() <= 1
                    })
                    .collect();

                // Prefer non-Japanese, non-special edition from contemporary releases
                contemporary
                    .iter()
                    .find(|r| {
                        let is_jp = r.country.as_ref().map(|c| c == "JP").unwrap_or(false);
                        !is_jp && !Self::is_special_edition(r)
                    })
                    .copied()
                    .or_else(|| {
                        // Fallback to any non-special edition
                        contemporary
                            .iter()
                            .find(|r| !Self::is_special_edition(r))
                            .copied()
                    })
                    .or_else(|| contemporary.first().copied())
                    .unwrap_or(&releases[0])
            } else {
                &releases[0] // Just take the earliest
            };

            log::debug!(
                "Selected canonical release: '{}' from {} ({})",
                canonical_release.title,
                canonical_release
                    .date
                    .as_ref()
                    .map(|d| d.0.as_str())
                    .unwrap_or("unknown"),
                canonical_release
                    .country
                    .as_ref()
                    .unwrap_or(&"??".to_string())
            );

            // Now check if the track exists on this specific canonical release
            // We need to fetch the full release with its recordings
            use musicbrainz_rs::Fetch;
            let full_release = Release::fetch()
                .id(&canonical_release.id)
                .with_recordings()
                .execute()
                .await?;

            // Check if any track on this release matches our track
            if let Some(media) = full_release.media {
                for medium in media {
                    if let Some(tracks) = medium.tracks {
                        for track in tracks {
                            // Check the track title directly (it's always present)
                            if track.title.eq_ignore_ascii_case(title) {
                                log::debug!("Track '{}' found on canonical release", title);
                                return Ok(true);
                            }
                        }
                    }
                }
            }

            log::debug!(
                "Track '{}' NOT found on canonical release '{}'",
                title,
                canonical_release.title
            );
            Ok(false)
        } else {
            // No album specified - just check if track exists
            let search_results = self
                .search_musicbrainz_multiple(artist, title, None)
                .await?;

            // Just verify the track exists with matching artist
            for result in search_results {
                if result.artist.eq_ignore_ascii_case(artist)
                    && result.title.eq_ignore_ascii_case(title)
                {
                    log::debug!("Track '{}' by '{}' exists", title, artist);
                    return Ok(true);
                }
            }

            Ok(false)
        }
    }

    /// Verify that a track exists in MusicBrainz with the given metadata
    /// Wrapper for backwards compatibility
    pub async fn verify_track_exists(
        &self,
        artist: &str,
        title: &str,
        album: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        self.verify_track_exists_on_canonical_release(artist, title, album)
            .await
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

        log::debug!("MusicBrainz query string: {query_string}");

        // Perform the search
        let search_results = Recording::search(query_string)
            .execute()
            .await
            .map_err(|e| format!("MusicBrainz search failed: {e}"))?;

        log::debug!(
            "Found {} MusicBrainz results (showing up to {})",
            search_results.entities.len(),
            self.max_results
        );

        let mut results = Vec::new();

        for recording in search_results.entities.iter().take(self.max_results) {
            if let Some(artist_credit) = &recording.artist_credit {
                let mb_artist = artist_credit
                    .first()
                    .map(|ac| ac.artist.name.clone())
                    .unwrap_or_default();

                let mb_title = recording.title.clone();

                // Log all available releases for debugging
                if let Some(releases) = &recording.releases {
                    log::debug!(
                        "Recording '{}' has {} releases: {:?}",
                        mb_title,
                        releases.len(),
                        releases.iter().map(|r| &r.title).collect::<Vec<_>>()
                    );
                }

                let (mb_album, release_id) = self
                    .select_best_release(recording, album)
                    .map(|(title, id)| (Some(title), Some(id)))
                    .unwrap_or((None, None));

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
                    release_id,
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
                    log::debug!(
                        "Found MusicBrainz match for '{} - {} [{}]': '{}' by '{}' (confidence: {:.2})",
                        track.name,
                        track.artist,
                        track.album.clone().unwrap_or("No Album".to_string()),
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
                log::warn!(
                    "MusicBrainz search failed for '{}' by '{}': {}",
                    track.name,
                    track.artist,
                    e
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

        log::debug!(
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

    #[test_log::test]
    fn should_calculate_string_similarity_correctly() {
        let provider = MusicBrainzScrubActionProvider::default();

        // Exact match
        assert_eq!(provider.calculate_similarity("Hello", "Hello"), 1.0);

        // Case insensitive
        assert_eq!(provider.calculate_similarity("Hello", "hello"), 1.0);

        // Partial match should be less than 1.0
        let similarity = provider.calculate_similarity("Hello World", "Hello");
        assert!(similarity < 1.0 && similarity > 0.0);
    }

    #[test_log::test(tokio::test)]
    async fn should_implement_provider_interface_correctly() {
        let provider = MusicBrainzScrubActionProvider::default();
        assert_eq!(provider.provider_name(), "MusicBrainzScrubActionProvider");

        // Test with empty tracks
        let tracks = vec![];
        let result = provider.analyze_tracks(&tracks, None, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }
}
