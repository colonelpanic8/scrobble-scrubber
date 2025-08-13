use crate::config::{ReleaseFilterConfig, ReleaseFilterType};
use musicbrainz_rs::entity::recording::{Recording, RecordingSearchQuery};
use musicbrainz_rs::entity::release::Release;
use musicbrainz_rs::Search;
use std::collections::HashMap;
use std::sync::RwLock;

/// A match from MusicBrainz with confidence score
#[derive(Debug, Clone)]
pub struct MusicBrainzMatch {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub confidence: f32,
    pub mbid: String,
    pub release_id: Option<String>,
}

/// Core MusicBrainz client with shared functionality for all providers
pub struct MusicBrainzClient {
    pub confidence_threshold: f32,
    pub max_results: usize,
    pub release_filters: ReleaseFilterConfig,
    cache: RwLock<HashMap<String, CachedResult>>,
}

#[derive(Debug, Clone)]
struct CachedResult {
    recordings: Vec<Recording>,
    timestamp: std::time::Instant,
}

impl MusicBrainzClient {
    /// Create a new MusicBrainz client with default settings
    #[must_use]
    pub fn new(confidence_threshold: f32, max_results: usize) -> Self {
        Self {
            confidence_threshold,
            max_results,
            release_filters: ReleaseFilterConfig::default(),
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new client with custom filter configuration
    #[must_use]
    pub fn with_filters(
        confidence_threshold: f32,
        max_results: usize,
        release_filters: ReleaseFilterConfig,
    ) -> Self {
        Self {
            confidence_threshold,
            max_results,
            release_filters,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Search for recordings by artist and title
    pub async fn search_recording(
        &self,
        artist: &str,
        title: &str,
    ) -> Result<Vec<Recording>, Box<dyn std::error::Error + Send + Sync>> {
        let cache_key = format!("{artist}:{title}");

        // Check cache first (expires after 5 minutes)
        if let Ok(cache) = self.cache.read() {
            if let Some(cached) = cache.get(&cache_key) {
                if cached.timestamp.elapsed().as_secs() < 300 {
                    log::trace!("Using cached MusicBrainz search for '{cache_key}'");
                    return Ok(cached.recordings.clone());
                }
            }
        }

        let query = RecordingSearchQuery::query_builder()
            .recording(title)
            .and()
            .artist(artist)
            .build();

        log::debug!("Searching MusicBrainz for recording: {query}");

        let search_results = Recording::search(query)
            .execute()
            .await
            .map_err(|e| format!("MusicBrainz search failed: {e}"))?;

        let recordings: Vec<Recording> = search_results
            .entities
            .into_iter()
            .take(self.max_results)
            .collect();

        // Cache the results
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(
                cache_key,
                CachedResult {
                    recordings: recordings.clone(),
                    timestamp: std::time::Instant::now(),
                },
            );
        }

        Ok(recordings)
    }

    /// Check if an album name is likely a compilation
    pub fn is_compilation_album(album_name: &str) -> bool {
        let lower = album_name.to_lowercase();

        // Common compilation patterns
        lower.contains("greatest")  // "greatest hits", "greatest video hits", etc
            || lower.contains("best of")
            || lower.contains("collection")
            || lower.contains("essential")
            || lower.contains("anthology")
            || lower.contains("ultimate")
            || lower.starts_with("now that's what")
            || lower.contains("soundtrack")
            || lower.contains("ost")
            || lower.contains("various artists")
            || lower.contains("compilati") // compilation, compilatie (Dutch), etc.
            || lower.contains("hits") // "hits", "greatest hits", "video hits"
            || lower.contains("the classic of") // "The Classic of..." compilations
            || lower.contains("present") // "Big Boi and Dre Present..."
            || lower.contains("introducing") // "Introducing..." compilations
            || lower.contains("definitive") // "The Definitive Collection"
            || lower.contains("singles") // Singles collections
    }

    /// Check if an album name is likely a live album
    pub fn is_live_album(album_name: &str) -> bool {
        let lower = album_name.to_lowercase();

        // Common live album patterns
        lower.contains("live at")
            || lower.contains("live in")
            || lower.contains("live from")
            || lower.contains("concert")
            || lower.contains("unplugged")
            || (lower.contains("live") && !lower.contains("alive")) // "live" but not "alive"
            || lower.contains("bootleg")
            || lower.contains("world tour")
    }

    /// Check if a release is by "Various Artists" or similar
    pub fn is_various_artists_release(release: &Release) -> bool {
        // Check if there's an artist credit that indicates various artists
        if let Some(artist_credit) = &release.artist_credit {
            for credit in artist_credit {
                let artist_name = credit.artist.name.to_lowercase();
                if artist_name == "various artists"
                    || artist_name == "various"
                    || artist_name == "va"
                    || artist_name == "v/a"
                {
                    return true;
                }
            }
        }
        false
    }

    /// Filter out compilation releases from a list
    pub fn filter_out_compilations(&self, releases: Vec<Release>) -> Vec<Release> {
        releases
            .into_iter()
            .filter(|r| {
                // Keep releases that are NOT compilations or live albums
                !Self::is_compilation_album(&r.title)
                    && !Self::is_various_artists_release(r)
                    && !Self::is_live_album(&r.title)
            })
            .collect()
    }

    /// Select the earliest non-compilation release from a list
    pub fn select_earliest_non_compilation<'a>(
        &self,
        releases: &'a [&'a Release],
    ) -> Option<&'a Release> {
        if releases.is_empty() {
            return None;
        }

        // Sort by date and return the earliest
        let mut sorted: Vec<&Release> = releases.to_vec();
        sorted.sort_by(|a, b| Self::compare_release_dates(a, b));

        sorted.first().copied()
    }

    /// Compare releases by date for sorting (earliest first)
    pub fn compare_release_dates(a: &Release, b: &Release) -> std::cmp::Ordering {
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

    /// Find the earliest release for a recording
    pub async fn find_earliest_release_for_recording(
        &self,
        artist: &str,
        title: &str,
        current_album: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        // Import the ReleaseStatus enum for filtering
        use musicbrainz_rs::entity::release::ReleaseStatus;

        // Search for the recording
        let recordings = self.search_recording(artist, title).await?;

        for recording in recordings {
            // Check if this recording matches our track
            if let Some(artist_credit) = &recording.artist_credit {
                let rec_artist = artist_credit
                    .first()
                    .map(|ac| ac.artist.name.as_str())
                    .unwrap_or("");

                // Check artist match (case-insensitive)
                if !rec_artist.eq_ignore_ascii_case(artist) {
                    continue;
                }
            }

            // Get all releases for this recording
            if let Some(releases) = &recording.releases {
                // Get all releases except the current one, filtering out singles, live albums, and bootlegs
                let other_releases: Vec<&Release> = releases
                    .iter()
                    .filter(|r| {
                        // Exclude the current album
                        !r.title.eq_ignore_ascii_case(current_album)
                            // Exclude live albums
                            && !Self::is_live_album(&r.title)
                            // Avoid suggesting singles - they often have just the track name
                            && !r.title.eq_ignore_ascii_case(title)
                            // IMPORTANT: Only include official releases (exclude bootlegs, promos, etc.)
                            && match &r.status {
                                Some(ReleaseStatus::Official) => true,
                                None => true, // If status is not provided, we'll include it
                                _ => false, // Exclude Bootleg, Promotion, PseudoRelease, etc.
                            }
                    })
                    .collect();

                if other_releases.is_empty() {
                    continue;
                }

                // Sort by date to find the earliest
                let mut sorted_releases = other_releases;
                sorted_releases.sort_by(|a, b| Self::compare_release_dates(a, b));

                // Return the earliest release
                if let Some(earliest) = sorted_releases.first() {
                    log::debug!(
                        "Found earliest release for '{}' by '{}': '{}' from {} (was '{}', status: {:?})",
                        title,
                        artist,
                        earliest.title,
                        Self::get_release_date_str(earliest),
                        current_album,
                        earliest.status
                    );
                    return Ok(Some(earliest.title.clone()));
                }
            }
        }

        Ok(None)
    }

    /// Find the canonical (non-compilation) release for a recording
    pub async fn find_canonical_release_for_recording(
        &self,
        artist: &str,
        title: &str,
        current_album: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        // Import the ReleaseStatus enum for filtering
        use musicbrainz_rs::entity::release::ReleaseStatus;

        // Search for the recording
        let recordings = self.search_recording(artist, title).await?;

        for recording in recordings {
            // Check if this recording matches our track
            if let Some(artist_credit) = &recording.artist_credit {
                let rec_artist = artist_credit
                    .first()
                    .map(|ac| ac.artist.name.as_str())
                    .unwrap_or("");

                // Check artist match (case-insensitive)
                if !rec_artist.eq_ignore_ascii_case(artist) {
                    continue;
                }
            }

            // Get all releases for this recording
            if let Some(releases) = &recording.releases {
                // Filter out compilations, live albums, bootlegs, and the current album
                let non_compilation_releases: Vec<&Release> = releases
                    .iter()
                    .filter(|r| {
                        !r.title.eq_ignore_ascii_case(current_album)
                            && !Self::is_compilation_album(&r.title)
                            && !Self::is_various_artists_release(r)
                            && !Self::is_live_album(&r.title)
                            // IMPORTANT: Only include official releases (exclude bootlegs, promos, etc.)
                            && match &r.status {
                                Some(ReleaseStatus::Official) => true,
                                None => true, // If status is not provided, we'll include it
                                _ => false, // Exclude Bootleg, Promotion, PseudoRelease, etc.
                            }
                    })
                    .collect();

                // Select the earliest non-compilation release
                let non_comp_refs: Vec<&Release> = non_compilation_releases.to_vec();
                if let Some(canonical) = self.select_earliest_non_compilation(&non_comp_refs) {
                    log::debug!(
                        "Found canonical release for '{}' by '{}': '{}' (was '{}')",
                        title,
                        artist,
                        canonical.title,
                        current_album
                    );
                    return Ok(Some(canonical.title.clone()));
                }
            }
        }

        Ok(None)
    }

    /// Calculate string similarity between two strings
    pub fn calculate_similarity(&self, a: &str, b: &str) -> f32 {
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

    /// Check if a release has special edition markers in its disambiguation
    pub fn is_special_edition(release: &Release) -> bool {
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

    /// Check if a release is a demo in its disambiguation
    pub fn is_demo(release: &Release) -> bool {
        release
            .disambiguation
            .as_ref()
            .map(|d| {
                let d_lower = d.to_lowercase();
                d_lower.contains("demo")
            })
            .unwrap_or(false)
    }

    /// Check if a release should be excluded based on configured filters
    pub fn should_exclude_release(&self, release: &Release) -> bool {
        for filter in &self.release_filters.filters {
            match filter {
                ReleaseFilterType::ExcludeDemo => {
                    if Self::is_demo(release) {
                        return true;
                    }
                }
                ReleaseFilterType::ExcludeSpecialEdition => {
                    if Self::is_special_edition(release) {
                        return true;
                    }
                }
                ReleaseFilterType::ExcludeByDisambiguation { terms } => {
                    if let Some(disambiguation) = &release.disambiguation {
                        let d_lower = disambiguation.to_lowercase();
                        if terms
                            .iter()
                            .any(|term| d_lower.contains(&term.to_lowercase()))
                        {
                            return true;
                        }
                    }
                }
                ReleaseFilterType::ExcludeByCountry { countries } => {
                    if let Some(country) = &release.country {
                        if countries.iter().any(|c| c.eq_ignore_ascii_case(country)) {
                            return true;
                        }
                    }
                }
                // PreferNonJapanese is handled differently (deprioritization, not exclusion)
                ReleaseFilterType::PreferNonJapanese => {}
            }
        }

        // Check custom exclusion terms
        if !self.release_filters.custom_exclusion_terms.is_empty() {
            if let Some(disambiguation) = &release.disambiguation {
                let d_lower = disambiguation.to_lowercase();
                if self
                    .release_filters
                    .custom_exclusion_terms
                    .iter()
                    .any(|term| d_lower.contains(&term.to_lowercase()))
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a release should be deprioritized based on configured filters
    pub fn should_deprioritize_release(&self, release: &Release) -> bool {
        // Currently only Japanese releases are deprioritized rather than excluded
        if self.prefer_non_japanese_releases() {
            if let Some(country) = &release.country {
                return country == "JP";
            }
        }
        false
    }

    /// Check if Japanese releases should be deprioritized
    pub fn prefer_non_japanese_releases(&self) -> bool {
        self.release_filters
            .filters
            .iter()
            .any(|f| matches!(f, ReleaseFilterType::PreferNonJapanese))
    }

    /// Get the date string for a release (for logging)
    pub fn get_release_date_str(release: &Release) -> &str {
        release
            .date
            .as_ref()
            .map(|d| d.0.as_str())
            .unwrap_or("unknown")
    }

    /// Select the canonical release from a list of releases
    pub fn select_canonical_release<'a>(&self, releases: &'a [Release]) -> Option<&'a Release> {
        if releases.is_empty() {
            return None;
        }

        // Filter out excluded releases based on configured filters
        let filtered_releases: Vec<_> = releases
            .iter()
            .filter(|r| !self.should_exclude_release(r))
            .collect();

        // If all releases are excluded, fall back to all releases
        let working_releases = if filtered_releases.is_empty() {
            releases.iter().collect()
        } else {
            filtered_releases
        };

        // Sort releases by date
        let mut sorted_releases: Vec<_> = working_releases;
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

        // Apply deprioritization filters (e.g., Japanese releases)
        if self.prefer_non_japanese_releases() {
            // Check if there are any non-deprioritized releases available
            let has_non_deprioritized = contemporary
                .iter()
                .any(|r| !self.should_deprioritize_release(r));

            if has_non_deprioritized {
                // First pass: non-special, non-deprioritized releases
                // Prefer US releases when multiple are available
                if let Some(release) = contemporary.iter().find(|r| {
                    !Self::is_special_edition(r)
                        && !self.should_deprioritize_release(r)
                        && r.country.as_ref().map(|c| c == "US").unwrap_or(false)
                }) {
                    return Some(release);
                }

                // If no US release, take any non-special, non-deprioritized release
                if let Some(release) = contemporary
                    .iter()
                    .find(|r| !Self::is_special_edition(r) && !self.should_deprioritize_release(r))
                {
                    return Some(release);
                }
            }
        }

        // Second pass: any non-special edition (including deprioritized ones)
        // Still prefer US releases
        if let Some(release) = contemporary.iter().find(|r| {
            !Self::is_special_edition(r) && r.country.as_ref().map(|c| c == "US").unwrap_or(false)
        }) {
            return Some(release);
        }

        if let Some(release) = contemporary.iter().find(|r| !Self::is_special_edition(r)) {
            return Some(release);
        }

        // If all are special editions, take the earliest
        contemporary
            .first()
            .copied()
            .or_else(|| sorted_releases.first().copied())
    }

    /// Build a MusicBrainz search query for a track
    pub fn build_track_query(artist: &str, title: &str, album: Option<&str>) -> String {
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

                // Select best release for this recording
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

    /// Select the best release from a recording's releases
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
                self.prefer_non_japanese_releases(),
            ) {
                return Some(result);
            }
        }

        // No desired album or no matches found - pick the earliest release overall
        // Apply the same Japanese release preference here
        if self.prefer_non_japanese_releases() {
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

    /// Select the best matching release for a specific album
    pub fn select_matching_album_release(
        releases: &[Release],
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
                            "Selected non-Japanese release '{}' from {} (country: {:?})",
                            release.title,
                            Self::get_release_date_str(release),
                            release.country
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
                    "Selected release '{}' from {} (country: {:?})",
                    release.title,
                    Self::get_release_date_str(release),
                    release.country
                );
                return Some((release.title.clone(), release.id.clone()));
            }
        }

        // If all are special editions, take the earliest
        if let Some(earliest) = matching.first() {
            log::debug!(
                "All releases have special edition markers, selecting earliest: '{}' from {}",
                earliest.title,
                Self::get_release_date_str(earliest)
            );
            return Some((earliest.title.clone(), earliest.id.clone()));
        }

        None
    }
}
