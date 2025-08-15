use crate::config::ReleaseFilterConfig;
use musicbrainz_rs::entity::recording::{Recording, RecordingSearchQuery};
use musicbrainz_rs::entity::release::Release;
use musicbrainz_rs::{Fetch, Search};
use std::collections::HashMap;
use std::sync::RwLock;

/// A match from MusicBrainz with confidence score
#[derive(Debug, Clone)]
pub struct MusicBrainzMatch {
    pub artist: String,
    pub album: String,
    pub confidence: f32,
    pub release_id: Option<String>,
}

/// Core MusicBrainz client for API interactions
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
            max_results: max_results.max(20), // Ensure we get enough results
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

    /// Fetch a release with its release group
    pub async fn fetch_release_with_group(
        &self,
        release_id: &str,
    ) -> Result<Release, Box<dyn std::error::Error + Send + Sync>> {
        Release::fetch()
            .id(release_id)
            .with_release_groups()
            .execute()
            .await
            .map_err(|e| e.into())
    }

    /// Compare release dates for sorting (earliest first)
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

    /// Check if a release has special edition markers in its disambiguation
    pub fn is_special_edition(release: &Release) -> bool {
        release
            .disambiguation
            .as_ref()
            .map(|d| {
                let lower = d.to_lowercase();
                lower.contains("deluxe")
                    || lower.contains("remaster")
                    || lower.contains("expanded")
                    || lower.contains("anniversary")
            })
            .unwrap_or(false)
    }

    /// Find albums by artist and album name
    pub async fn find_albums_by_artist_and_name(
        &self,
        artist: &str,
        album: &str,
    ) -> Result<Vec<MusicBrainzMatch>, Box<dyn std::error::Error + Send + Sync>> {
        let recordings = self.search_recording(artist, album).await?;
        let mut matches = Vec::new();

        for recording in recordings {
            if let Some(releases) = &recording.releases {
                for release in releases {
                    // Skip if filtered by configuration
                    if self.should_filter_release(&release.status) {
                        continue;
                    }

                    // Calculate confidence based on artist and album match
                    let artist_confidence = if let Some(artist_credit) = &release.artist_credit {
                        artist_credit
                            .first()
                            .map(|ac| self.calculate_similarity(&ac.artist.name, artist))
                            .unwrap_or(0.0)
                    } else {
                        0.0
                    };

                    let album_confidence = self.calculate_similarity(&release.title, album);
                    let confidence = (artist_confidence + album_confidence) / 2.0;

                    if confidence >= self.confidence_threshold {
                        matches.push(MusicBrainzMatch {
                            artist: release
                                .artist_credit
                                .as_ref()
                                .and_then(|ac| ac.first())
                                .map(|ac| ac.artist.name.clone())
                                .unwrap_or_else(|| artist.to_string()),
                            album: release.title.clone(),
                            confidence,
                            release_id: Some(release.id.clone()),
                        });
                    }
                }
            }
        }

        // Sort by confidence and deduplicate
        matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        matches.dedup_by(|a, b| a.album == b.album && a.artist == b.artist);

        Ok(matches)
    }

    fn should_filter_release(
        &self,
        status: &Option<musicbrainz_rs::entity::release::ReleaseStatus>,
    ) -> bool {
        use musicbrainz_rs::entity::release::ReleaseStatus;

        // For now, only filter out non-official releases
        // This is a simplified version - expand based on requirements
        matches!(
            status,
            Some(ReleaseStatus::Bootleg | ReleaseStatus::PseudoRelease)
        )
    }
}
