use super::client::MusicBrainzClient;
use crate::persistence::{PendingEdit, PendingRewriteRule};
use crate::scrub_action_provider::{
    ActionProviderError, ScrubActionProvider, SuggestionWithContext,
};
use async_trait::async_trait;
use lastfm_edit::{ScrobbleEdit, Track};
use musicbrainz_rs::entity::recording::Recording;
use musicbrainz_rs::entity::release::{Release, ReleaseStatus};
use musicbrainz_rs::entity::release_group::{
    ReleaseGroup, ReleaseGroupPrimaryType, ReleaseGroupSecondaryType,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Type for a function that compares two releases
/// Returns Ordering::Less if a is preferred over b
pub type ReleaseComparer =
    fn(&Release, Option<&ReleaseGroup>, &Release, Option<&ReleaseGroup>) -> Ordering;

/// Details about a release and its ranking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedRelease {
    pub title: String,
    pub artist: String,
    pub release_id: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub disambiguation: Option<String>,
    pub status: Option<String>,
    pub primary_type: Option<ReleaseGroupPrimaryType>,
    pub secondary_types: Vec<ReleaseGroupSecondaryType>,
    pub is_compilation: bool,
    pub is_various_artists: bool,
    pub rank: usize,
    pub rank_reason: String,
}

/// Check if there's a significant date gap that should override type priority
/// Returns Some(ordering) if a date gap preference should apply, None otherwise
fn check_date_gap_preference(
    a: &Release,
    a_group: Option<&ReleaseGroup>,
    b: &Release,
    b_group: Option<&ReleaseGroup>,
) -> Option<Ordering> {
    // Get years from both releases
    let a_year = extract_release_year(a)?;
    let b_year = extract_release_year(b)?;

    // Get primary types
    let a_type = a_group.and_then(|rg| rg.primary_type.as_ref());
    let b_type = b_group.and_then(|rg| rg.primary_type.as_ref());

    // Check for single vs album with significant gap (10+ years)
    match (a_type, b_type) {
        (Some(ReleaseGroupPrimaryType::Single), Some(ReleaseGroupPrimaryType::Album)) => {
            if a_year + 10 <= b_year {
                // Single is 10+ years earlier than album, prefer single
                Some(Ordering::Less)
            } else {
                None
            }
        }
        (Some(ReleaseGroupPrimaryType::Album), Some(ReleaseGroupPrimaryType::Single)) => {
            if b_year + 10 <= a_year {
                // Single is 10+ years earlier than album, prefer single
                Some(Ordering::Greater)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract year from release date
fn extract_release_year(release: &Release) -> Option<i32> {
    release
        .date
        .as_ref()
        .and_then(|d| d.0.get(0..4)) // Get first 4 characters (year)
        .and_then(|year_str| year_str.parse::<i32>().ok())
}

/// Determine if a release has compilation-like secondary types
pub fn has_compilation_secondary_types(release_group: &ReleaseGroup) -> bool {
    release_group.secondary_types.iter().any(|st| {
        matches!(
            st,
            ReleaseGroupSecondaryType::Compilation
                | ReleaseGroupSecondaryType::Soundtrack
                | ReleaseGroupSecondaryType::Live
                | ReleaseGroupSecondaryType::Remix
                | ReleaseGroupSecondaryType::DjMix
                | ReleaseGroupSecondaryType::MixtapeStreet
                | ReleaseGroupSecondaryType::Interview
        )
    })
}

/// Get the priority score for a primary type (lower is better)
pub fn get_primary_type_priority(primary_type: Option<&ReleaseGroupPrimaryType>) -> u8 {
    match primary_type {
        Some(ReleaseGroupPrimaryType::Album) => 0,
        Some(ReleaseGroupPrimaryType::Ep) => 1,
        Some(ReleaseGroupPrimaryType::Single) => 2,
        Some(ReleaseGroupPrimaryType::Broadcast) => 3,
        _ => 4, // Unknown
    }
}

/// Get the priority score for a release status (lower is better)
pub fn get_release_status_priority(status: Option<&ReleaseStatus>) -> u8 {
    match status {
        Some(ReleaseStatus::Official) => 0,
        None => 1, // No status provided
        Some(ReleaseStatus::Promotion) => 2,
        Some(ReleaseStatus::Bootleg) => 3,
        Some(ReleaseStatus::PseudoRelease) => 4,
        Some(_) => 5, // Catch-all for any other status variants
    }
}

/// Check if a release should be considered a compilation
pub fn is_compilation_release(release: &Release, release_group: Option<&ReleaseGroup>) -> bool {
    // Check if it's various artists at least
    if MusicBrainzClient::is_various_artists_release(release) {
        return true;
    }

    // Without release group info, we can't determine if it's a compilation reliably
    if let Some(rg) = release_group {
        has_compilation_secondary_types(rg)
    } else {
        false
    }
}

/// Check if a release title indicates a special edition (deluxe, remaster, etc.)
fn is_special_edition(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower.contains("deluxe")
        || lower.contains("remaster")
        || lower.contains("special")
        || lower.contains("anniversary")
        || lower.contains("expanded")
        || lower.contains("collector")
        || lower.contains("limited")
        || lower.contains("super")
        || lower.contains("bonus")
}

/// Default release comparison function
/// Prefers: Official > None > Promotion > Bootleg > PseudoRelease
/// Then: Non-compilations over compilations, then regular editions over special editions,
/// then Studio Albums > EPs > Singles > Broadcast > Unknown
/// For same quality and status, prefers earlier releases
pub fn default_release_comparer(
    a: &Release,
    a_group: Option<&ReleaseGroup>,
    b: &Release,
    b_group: Option<&ReleaseGroup>,
) -> Ordering {
    // First, compare by release status (official releases are preferred)
    let a_status_priority = get_release_status_priority(a.status.as_ref());
    let b_status_priority = get_release_status_priority(b.status.as_ref());

    match a_status_priority.cmp(&b_status_priority) {
        Ordering::Equal => {
            // Same status, check if either is a compilation
            let a_is_compilation = is_compilation_release(a, a_group);
            let b_is_compilation = is_compilation_release(b, b_group);

            match (a_is_compilation, b_is_compilation) {
                (true, false) => return Ordering::Greater, // b is better (non-compilation)
                (false, true) => return Ordering::Less,    // a is better (non-compilation)
                _ => {} // Both are compilations or both are non-compilations, continue comparing
            }

            // Check if either is a special edition (prefer regular editions)
            let a_is_special = is_special_edition(&a.title);
            let b_is_special = is_special_edition(&b.title);

            match (a_is_special, b_is_special) {
                (true, false) => return Ordering::Greater, // b is better (regular edition)
                (false, true) => return Ordering::Less,    // a is better (regular edition)
                _ => {} // Both are special or both are regular, continue comparing
            }

            // Check for significant date gaps between singles and albums
            let date_comparison = MusicBrainzClient::compare_release_dates(a, b);
            if let Some(gap_preference) = check_date_gap_preference(a, a_group, b, b_group) {
                return gap_preference;
            }

            // Compare by primary type priority
            let a_primary_priority =
                get_primary_type_priority(a_group.and_then(|rg| rg.primary_type.as_ref()));
            let b_primary_priority =
                get_primary_type_priority(b_group.and_then(|rg| rg.primary_type.as_ref()));

            match a_primary_priority.cmp(&b_primary_priority) {
                Ordering::Equal => {
                    // Same primary type, prefer earlier release
                    date_comparison
                }
                other => other,
            }
        }
        other => other,
    }
}

/// Check if a release group represents a proper studio album
pub fn is_studio_album(release_group: &ReleaseGroup) -> bool {
    // Must not have compilation-like secondary types
    if has_compilation_secondary_types(release_group) {
        return false;
    }

    // Must be an album type
    matches!(
        release_group.primary_type.as_ref(),
        Some(ReleaseGroupPrimaryType::Album)
    )
}

/// Provider that suggests moving tracks from compilations to their canonical (original studio) release
/// using MusicBrainz data to find the non-compilation album where the track originally appeared
pub struct CompilationToCanonicalProvider {
    client: MusicBrainzClient,
    enabled: bool,
    #[allow(dead_code)]
    confidence_threshold: f32,
    release_comparer: ReleaseComparer,
    official_releases_only: bool,
}

impl CompilationToCanonicalProvider {
    /// Create a new compilation-to-canonical provider with default settings
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: MusicBrainzClient::new(0.8, 10),
            enabled: true,
            confidence_threshold: 0.8,
            release_comparer: default_release_comparer,
            official_releases_only: true,
        }
    }

    /// Create a provider with custom confidence threshold
    #[must_use]
    pub fn with_confidence_threshold(confidence_threshold: f32) -> Self {
        Self {
            client: MusicBrainzClient::new(confidence_threshold, 10),
            enabled: true,
            confidence_threshold,
            release_comparer: default_release_comparer,
            official_releases_only: true,
        }
    }

    /// Create a provider with custom release comparer
    #[must_use]
    pub fn with_comparer(comparer: ReleaseComparer) -> Self {
        Self {
            client: MusicBrainzClient::new(0.8, 10),
            enabled: true,
            confidence_threshold: 0.8,
            release_comparer: comparer,
            official_releases_only: true,
        }
    }

    /// Enable or disable the provider
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set whether to filter to official releases only
    #[must_use]
    pub const fn with_official_releases_only(mut self, official_only: bool) -> Self {
        self.official_releases_only = official_only;
        self
    }

    /// Rank all releases for a recording and return detailed information
    pub async fn rank_releases_for_recording(
        &self,
        artist: &str,
        title: &str,
        _current_album: Option<&str>,
    ) -> Result<Vec<RankedRelease>, Box<dyn std::error::Error + Send + Sync>> {
        // Search for recordings and collect eligible releases
        let recordings = self.client.search_recording(artist, title).await?;

        log::debug!(
            "Found {} recordings for '{}' by '{}'",
            recordings.len(),
            title,
            artist
        );

        // Log details about each recording and its releases
        for (i, recording) in recordings.iter().enumerate() {
            log::debug!("Recording {}: '{}'", i + 1, recording.title);
            if let Some(artist_credit) = &recording.artist_credit {
                if let Some(first_artist) = artist_credit.first() {
                    log::debug!("  Artist: '{}'", first_artist.artist.name);
                }
            }
            if let Some(releases) = &recording.releases {
                log::debug!("  Found {} releases for this recording:", releases.len());
                for (j, release) in releases.iter().enumerate() {
                    log::debug!(
                        "    Release {}: '{}' (ID: {}, Status: {:?})",
                        j + 1,
                        release.title,
                        release.id,
                        release.status
                    );
                    // Check if this is a various artists release
                    if MusicBrainzClient::is_various_artists_release(release) {
                        log::debug!("      -> Various artists release (will be excluded)");
                    }
                    // Check official releases only filter
                    if self.official_releases_only {
                        if let Some(status) = &release.status {
                            if !matches!(status, ReleaseStatus::Official) {
                                log::debug!("      -> Non-official release (will be excluded due to filter)");
                            }
                        }
                    }
                }
            } else {
                log::debug!("  No releases found for this recording");
            }
        }

        // Start with owned releases to make lifetime management easier
        let eligible_releases = self.collect_eligible_releases(&recordings, artist);
        log::debug!(
            "Collected {} eligible releases from recordings (after filtering)",
            eligible_releases.len()
        );

        let mut all_releases: Vec<Release> = eligible_releases.into_iter().cloned().collect();

        // Log the releases we're starting with
        for (i, release) in all_releases.iter().enumerate() {
            log::debug!(
                "  Eligible release {}: '{}' (ID: {})",
                i + 1,
                release.title,
                release.id
            );
        }

        // Always do album search in addition to recording search to get comprehensive results
        log::debug!("Performing supplementary album search for comprehensive results");

        let album_releases = self.search_releases_by_album_name(artist, title).await?;

        log::debug!(
            "Album search returned {} releases, filtering and deduplicating...",
            album_releases.len()
        );

        // Add album search results to our releases (avoid duplicates)
        let mut added_from_album_search = 0;
        for release in album_releases {
            let should_include = self.should_include_release(&release);
            let is_duplicate = all_releases.iter().any(|r| r.id == release.id);

            log::debug!(
                "Album search release '{}': should_include={}, is_duplicate={}",
                release.title,
                should_include,
                is_duplicate
            );

            if should_include && !is_duplicate {
                log::debug!("  -> Adding to results");
                all_releases.push(release);
                added_from_album_search += 1;
            }
        }

        log::debug!(
            "Added {} releases from album search. Total: {} releases",
            added_from_album_search,
            all_releases.len()
        );

        // Convert to borrowed references for the sorting function
        let borrowed_all_releases: Vec<&Release> = all_releases.iter().collect();

        if all_releases.is_empty() {
            log::debug!("No eligible releases found after filtering and fallback");
            return Ok(Vec::new());
        }

        // Fetch release groups and sort releases
        let releases_with_groups = self.fetch_and_sort_releases(borrowed_all_releases).await?;

        // Convert to ranked releases with detailed information
        let ranked_releases = self.create_ranked_releases(releases_with_groups);

        // Log the final ranked results
        log::debug!(
            "Final ranking for '{}' by '{}': {} releases",
            title,
            artist,
            ranked_releases.len()
        );
        for release in &ranked_releases {
            log::debug!(
                "  Rank {}: '{}' - {} (compilation: {}, va: {}) - {}",
                release.rank,
                release.title,
                release
                    .primary_type
                    .as_ref()
                    .map(|t| format!("{t:?}"))
                    .unwrap_or_else(|| "Unknown".to_string()),
                release.is_compilation,
                release.is_various_artists,
                release.rank_reason
            );
        }

        Ok(ranked_releases)
    }

    /// Collect all eligible releases from recordings that match the artist and title
    fn collect_eligible_releases<'a>(
        &self,
        recordings: &'a [Recording],
        artist: &str,
    ) -> Vec<&'a Release> {
        let mut all_releases: Vec<&Release> = Vec::new();

        // Collect all releases from matching recordings
        for recording in recordings {
            if !self.recording_matches_artist(recording, artist) {
                continue;
            }

            if let Some(releases) = &recording.releases {
                for release in releases {
                    if self.should_include_release(release) {
                        all_releases.push(release);
                    }
                }
            }
        }

        all_releases
    }

    /// Check if a recording matches the target artist
    fn recording_matches_artist(&self, recording: &Recording, target_artist: &str) -> bool {
        if let Some(artist_credit) = &recording.artist_credit {
            let rec_artist = artist_credit
                .first()
                .map(|ac| ac.artist.name.as_str())
                .unwrap_or("");

            rec_artist.eq_ignore_ascii_case(target_artist)
        } else {
            false
        }
    }

    /// Determine if a release should be included in the ranking
    fn should_include_release(&self, release: &Release) -> bool {
        // Skip various artists releases for initial collection
        if MusicBrainzClient::is_various_artists_release(release) {
            return false;
        }

        // Pre-filter to official releases only if configured
        if self.official_releases_only {
            if let Some(status) = &release.status {
                if !matches!(status, ReleaseStatus::Official) {
                    log::trace!(
                        "Skipping non-official release '{}' with status {:?}",
                        release.title,
                        status
                    );
                    return false;
                }
            }
            // If no status, we'll include it (could be official)
        }

        true
    }

    /// Fetch release groups for releases and sort them by priority
    async fn fetch_and_sort_releases<'a>(
        &self,
        all_releases: Vec<&'a Release>,
    ) -> Result<Vec<(&'a Release, Option<ReleaseGroup>)>, Box<dyn std::error::Error + Send + Sync>>
    {
        // Create a map to cache release group fetches
        let mut release_group_cache: HashMap<String, Option<ReleaseGroup>> = HashMap::new();

        // Sort releases with lazy loading of release groups
        let mut releases_with_indices: Vec<(usize, &Release)> = all_releases
            .iter()
            .enumerate()
            .map(|(i, r)| (i, *r))
            .collect();

        // Custom sort that fetches release groups lazily
        releases_with_indices.sort_by(|a, b| {
            // For status comparison, we don't need release groups
            let a_status_priority = get_release_status_priority(a.1.status.as_ref());
            let b_status_priority = get_release_status_priority(b.1.status.as_ref());

            if a_status_priority != b_status_priority {
                return a_status_priority.cmp(&b_status_priority);
            }

            // Only fetch release groups if we need them for further comparison
            // Since we're in a sync context here, we can't await
            // We'll need to pre-fetch for releases that pass the status check
            Ordering::Equal
        });

        // Now fetch release groups for the top releases that we'll actually use
        // Limit fetching to avoid too many API calls
        let fetch_limit = 20.min(releases_with_indices.len());
        let mut releases_with_groups: Vec<(&Release, Option<ReleaseGroup>)> = Vec::new();

        for (_, release) in releases_with_indices.iter().take(fetch_limit) {
            let release_group = self
                .fetch_release_group_cached(release, &mut release_group_cache)
                .await;
            releases_with_groups.push((release, release_group));
        }

        // Now do the full sort with release groups available
        releases_with_groups
            .sort_by(|a, b| (self.release_comparer)(a.0, a.1.as_ref(), b.0, b.1.as_ref()));

        Ok(releases_with_groups)
    }

    /// Fetch release group for a release, using cache to avoid duplicate API calls
    async fn fetch_release_group_cached(
        &self,
        release: &Release,
        cache: &mut HashMap<String, Option<ReleaseGroup>>,
    ) -> Option<ReleaseGroup> {
        if let Some(cached) = cache.get(&release.id) {
            cached.clone()
        } else if release.id.len() == 36 {
            match self.client.fetch_release_with_group(&release.id).await {
                Ok(full_release) => {
                    let rg = full_release.release_group;
                    cache.insert(release.id.clone(), rg.clone());
                    rg
                }
                Err(e) => {
                    log::debug!("Failed to fetch release group for {}: {}", release.title, e);
                    cache.insert(release.id.clone(), None);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Convert sorted releases to RankedRelease structs with detailed ranking information
    fn create_ranked_releases(
        &self,
        releases_with_groups: Vec<(&Release, Option<ReleaseGroup>)>,
    ) -> Vec<RankedRelease> {
        let mut ranked_releases = Vec::new();

        for (rank, (release, release_group)) in releases_with_groups.iter().enumerate() {
            let is_compilation = is_compilation_release(release, release_group.as_ref());
            let is_va = MusicBrainzClient::is_various_artists_release(release);

            let rank_reason =
                self.determine_rank_reason(rank, is_compilation, is_va, release_group);
            let artist_name = self.extract_artist_name(release);

            ranked_releases.push(RankedRelease {
                title: release.title.clone(),
                artist: artist_name,
                release_id: release.id.clone(),
                date: release.date.as_ref().map(|d| d.0.clone()),
                country: release.country.clone(),
                disambiguation: release.disambiguation.clone(),
                status: release.status.as_ref().map(|s| format!("{s:?}")),
                primary_type: release_group
                    .as_ref()
                    .and_then(|rg| rg.primary_type.clone()),
                secondary_types: release_group
                    .as_ref()
                    .map(|rg| rg.secondary_types.clone())
                    .unwrap_or_default(),
                is_compilation,
                is_various_artists: is_va,
                rank: rank + 1,
                rank_reason,
            });
        }

        ranked_releases
    }

    /// Determine the reason for a release's ranking position
    fn determine_rank_reason(
        &self,
        rank: usize,
        is_compilation: bool,
        is_va: bool,
        release_group: &Option<ReleaseGroup>,
    ) -> String {
        if rank == 0 {
            if is_compilation {
                "Best available (compilation)".to_string()
            } else if let Some(rg) = release_group {
                match rg.primary_type.as_ref() {
                    Some(ReleaseGroupPrimaryType::Album) => "Studio album (preferred)".to_string(),
                    Some(ReleaseGroupPrimaryType::Ep) => "EP release".to_string(),
                    Some(ReleaseGroupPrimaryType::Single) => "Single release".to_string(),
                    _ => "Best available release".to_string(),
                }
            } else {
                "Best available release".to_string()
            }
        } else if is_compilation {
            "Compilation (deprioritized)".to_string()
        } else if is_va {
            "Various artists (excluded)".to_string()
        } else if let Some(rg) = release_group {
            format!("Lower priority: {:?}", rg.primary_type)
        } else {
            "Lower priority release".to_string()
        }
    }

    /// Extract artist name from release artist credits
    fn extract_artist_name(&self, release: &Release) -> String {
        if let Some(ac) = &release.artist_credit {
            ac.first()
                .map(|a| a.artist.name.clone())
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Unknown".to_string()
        }
    }

    /// Search for releases using track name as album name (to find singles)
    async fn search_releases_by_album_name(
        &self,
        artist: &str,
        track_name: &str,
    ) -> Result<Vec<Release>, Box<dyn std::error::Error + Send + Sync>> {
        use crate::musicbrainz::MusicBrainzScrubActionProvider;

        // Try searching for track name as album name to find singles
        let releases =
            MusicBrainzScrubActionProvider::search_album_releases(artist, Some(track_name)).await?;

        log::debug!(
            "Album search for '{}' found {} releases",
            track_name,
            releases.len()
        );

        Ok(releases)
    }

    /// Check if we should attempt to find a better album for this track
    /// Returns true only if we can confirm the current album is NOT a studio album
    #[allow(dead_code)]
    async fn should_attempt_replacement(&self, current_release: &Release) -> bool {
        // First check: if it's various artists
        if MusicBrainzClient::is_various_artists_release(current_release) {
            log::debug!(
                "Album '{}' identified as various artists",
                current_release.title
            );
            return true;
        }

        // Second check: fetch the release group to check its type
        // Only do this if we have a valid MBID
        if current_release.id.len() == 36 {
            match self
                .client
                .fetch_release_with_group(&current_release.id)
                .await
            {
                Ok(full_release) => {
                    if let Some(rg) = &full_release.release_group {
                        if is_studio_album(rg) {
                            log::debug!(
                                "Album '{}' is a proper studio album (type: {:?}), will NOT attempt replacement",
                                current_release.title,
                                rg.primary_type
                            );
                            return false;
                        } else {
                            log::debug!(
                                "Album '{}' identified as non-studio album via release group (primary: {:?}, secondary: {:?})",
                                current_release.title,
                                rg.primary_type,
                                rg.secondary_types
                            );
                            return true;
                        }
                    }
                }
                Err(e) => {
                    log::debug!(
                        "Could not fetch release group for '{}': {}. Being conservative, not attempting replacement.",
                        current_release.title,
                        e
                    );
                    return false;
                }
            }
        }

        // If we can't determine the type, be conservative and don't suggest changes
        log::debug!(
            "Cannot determine type of '{}', being conservative and not attempting replacement",
            current_release.title
        );
        false
    }

    /// Check if a release is an acceptable replacement for a compilation
    #[allow(dead_code)]
    fn is_acceptable_replacement(
        &self,
        release: &Release,
        release_group: Option<&ReleaseGroup>,
    ) -> bool {
        // Anything that's not a compilation is acceptable
        !is_compilation_release(release, release_group)
    }

    /// Find the canonical (non-compilation) release for a recording
    async fn find_canonical_release_for_recording(
        &self,
        artist: &str,
        title: &str,
        current_album: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        // Get ranked releases
        let ranked_releases = self
            .rank_releases_for_recording(artist, title, Some(current_album))
            .await?;

        if ranked_releases.is_empty() {
            log::debug!("No releases found for '{title}' by '{artist}'");
            return Ok(None);
        }

        // Check if current album is in the results and whether we should attempt replacement
        log::debug!(
            "Checking if current album '{current_album}' is found in ranked releases and if it's a compilation"
        );

        let current_album_in_results = ranked_releases
            .iter()
            .find(|r| r.title.eq_ignore_ascii_case(current_album));

        match current_album_in_results {
            Some(found_release) => {
                log::debug!(
                    "Current album '{}' found in results: rank {}, compilation: {}, type: {:?}",
                    current_album,
                    found_release.rank,
                    found_release.is_compilation,
                    found_release.primary_type
                );

                if !found_release.is_compilation {
                    log::debug!(
                        "Track '{title}' by '{artist}' is on album '{current_album}' which appears to be a proper studio album. Not suggesting changes."
                    );
                    return Ok(None);
                }

                log::debug!(
                    "Album '{current_album}' confirmed as compilation/non-album. Looking for studio album replacement for '{title}'."
                );
            }
            None => {
                log::debug!(
                    "Current album '{current_album}' NOT found in search results - this is why we might miss compilations!"
                );
                log::debug!(
                    "Available albums in results: {}",
                    ranked_releases
                        .iter()
                        .map(|r| r.title.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                log::debug!(
                    "Current album not found in results - will still check if there's a better album available"
                );
                // Don't return early - continue to check if there's a better album
            }
        }

        // Find the best non-compilation release
        for release in &ranked_releases {
            // Skip if it's the current album
            if release.title.eq_ignore_ascii_case(current_album) {
                continue;
            }

            // Skip compilations and various artists
            if release.is_compilation || release.is_various_artists {
                continue;
            }

            // Found a good replacement
            let title_ref = &release.title;
            let rank = release.rank;
            let reason = &release.rank_reason;
            log::debug!(
                "Found acceptable release '{title_ref}' (rank: {rank}, reason: {reason}) to replace '{current_album}' for track '{title}' by '{artist}'"
            );

            return Ok(Some(release.title.clone()));
        }

        log::debug!("No acceptable non-compilation release found for '{title}' by '{artist}'");
        Ok(None)
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
                "Looking for canonical release of '{}' by '{}' (currently on '{}')",
                track.name,
                track.artist,
                current_album
            );

            // Try to find the canonical (non-compilation) release for this recording
            match self
                .find_canonical_release_for_recording(&track.artist, &track.name, current_album)
                .await
            {
                Ok(Some(canonical_album)) if canonical_album != *current_album => {
                    log::info!(
                        "Found canonical release for '{}' by '{}': '{}' (was '{}')",
                        track.name,
                        track.artist,
                        canonical_album,
                        current_album
                    );

                    // Create a ScrobbleEdit that changes only the album
                    let edit = ScrobbleEdit::with_minimal_info(
                        &track.name,
                        &track.artist,
                        &canonical_album,
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
                        "Track '{}' by '{}' - already on canonical release",
                        track.name,
                        track.artist
                    );
                }
                Ok(None) => {
                    log::debug!(
                        "No canonical release found for '{}' by '{}'",
                        track.name,
                        track.artist
                    );
                }
                Err(e) => {
                    log::warn!(
                        "Error finding canonical release for '{}' by '{}': {}",
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
