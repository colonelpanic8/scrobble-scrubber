use crate::persistence::{PendingEdit, PendingRewriteRule, RewriteRulesState};
use crate::rewrite::{RewriteError, RewriteRule};
use async_trait::async_trait;
use lastfm_edit::{ScrobbleEdit, Track};
use musicbrainz_rs::entity::recording::Recording as MbRecording;
use musicbrainz_rs::entity::release::Release;
// already imported above
use musicbrainz_rs::Fetch;
use regex::Regex;
use std::error::Error;
use std::fmt;
use std::time::Duration;

/// Generic error type for action providers
#[derive(Debug)]
pub struct ActionProviderError(pub String);

impl fmt::Display for ActionProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Action provider error: {}", self.0)
    }
}

impl Error for ActionProviderError {}

impl From<RewriteError> for ActionProviderError {
    fn from(err: RewriteError) -> Self {
        Self(format!("Rewrite error: {err}"))
    }
}

impl From<String> for ActionProviderError {
    fn from(msg: String) -> Self {
        Self(msg)
    }
}

impl From<&str> for ActionProviderError {
    fn from(msg: &str) -> Self {
        Self(msg.to_string())
    }
}

/// Represents a suggested action from an external source (LLM, API, etc.)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScrubActionSuggestion {
    /// Suggest an immediate scrobble edit
    Edit(ScrobbleEdit),
    /// Propose a new rewrite rule
    ProposeRule {
        rule: RewriteRule,
        motivation: String,
    },
    /// No action needed
    NoAction,
}

/// Context wrapper for suggestions that includes confirmation requirements
#[derive(Debug, Clone)]
pub struct SuggestionWithContext {
    pub suggestion: ScrubActionSuggestion,
    pub requires_confirmation: bool,
    pub provider_name: String,
}

impl SuggestionWithContext {
    pub fn new(
        suggestion: ScrubActionSuggestion,
        requires_confirmation: bool,
        provider_name: String,
    ) -> Self {
        Self {
            suggestion,
            requires_confirmation,
            provider_name,
        }
    }

    pub fn edit_with_confirmation(
        edit: ScrobbleEdit,
        requires_confirmation: bool,
        provider_name: String,
    ) -> Self {
        Self::new(
            ScrubActionSuggestion::Edit(edit),
            requires_confirmation,
            provider_name,
        )
    }

    pub fn propose_rule_with_confirmation(
        rule: RewriteRule,
        motivation: String,
        requires_confirmation: bool,
        provider_name: String,
    ) -> Self {
        Self::new(
            ScrubActionSuggestion::ProposeRule { rule, motivation },
            requires_confirmation,
            provider_name,
        )
    }

    pub fn no_action(provider_name: String) -> Self {
        Self::new(ScrubActionSuggestion::NoAction, false, provider_name)
    }
}

/// Trait for external providers that can suggest scrobble actions
#[async_trait]
pub trait ScrubActionProvider: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Analyze multiple tracks and provide suggestions for improvements
    /// Returns a vector of (track_index, suggestions) pairs
    ///
    /// Optional context parameters help avoid duplicate suggestions:
    /// - pending_edits: tracks that already have pending edits awaiting approval
    /// - pending_rules: rewrite rules that are already pending approval
    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        pending_edits: Option<&[PendingEdit]>,
        pending_rules: Option<&[PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<SuggestionWithContext>)>, Self::Error>;

    /// Get a human-readable name for this provider
    fn provider_name(&self) -> &str;
}

/// Rewrite rules-based action provider
pub struct RewriteRulesScrubActionProvider {
    rules: Vec<RewriteRule>,
}

impl RewriteRulesScrubActionProvider {
    #[must_use]
    pub fn new(rules_state: &RewriteRulesState) -> Self {
        Self {
            rules: rules_state.rewrite_rules.clone(),
        }
    }

    #[must_use]
    pub const fn from_rules(rules: Vec<RewriteRule>) -> Self {
        Self { rules }
    }

    // Apply rules sequentially to a track, gating on per-rule MusicBrainz confirmation when requested.
    // Returns Some((final_edit, requires_confirmation)) if any changes applied, otherwise None.
    async fn apply_rules_sequentially(
        &self,
        track: &Track,
    ) -> Result<Option<(ScrobbleEdit, bool)>, ActionProviderError> {
        let mut edit = crate::rewrite::create_no_op_edit(track);
        let mut any_changes = false;
        let mut requires_confirmation_applied = false;

        // Use a higher limit to ensure we find all recordings
        let mb_provider = crate::musicbrainz_provider::MusicBrainzScrubActionProvider::new(0.8, 20);

        for rule in &self.rules {
            if !rule.matches_scrobble_edit(&edit)? {
                continue;
            }

            let mut candidate = edit.clone();
            let changed = rule.apply(&mut candidate)?;
            if !changed {
                continue;
            }

            if rule.requires_musicbrainz_confirmation {
                let confirmed =
                    Self::verify_with_musicbrainz(&mb_provider, &candidate, track).await?;
                if !confirmed {
                    log::debug!(
                        "MB confirmation failed for rule '{}' on '{} - {}'",
                        rule.name.as_deref().unwrap_or("Unnamed"),
                        track.artist,
                        track.name
                    );
                    continue; // Skip this rule only
                }
            }

            // Accept candidate
            edit = candidate;
            any_changes = true;
            requires_confirmation_applied |= rule.requires_confirmation;
        }

        if any_changes {
            Ok(Some((edit, requires_confirmation_applied)))
        } else {
            Ok(None)
        }
    }

    /// Check if a release should be accepted for validation
    /// We want to verify tracks exist on the ORIGINAL release, not later expanded editions
    async fn should_accept_release(release: &Release) -> bool {
        // First check: reject releases with special edition keywords in disambiguation
        if let Some(disamb) = &release.disambiguation {
            let d = disamb.to_lowercase();
            // These keywords indicate special editions with bonus/extra tracks
            if d.contains("deluxe")
                || d.contains("expanded")
                || d.contains("bonus")
                || d.contains("anniversary")
                || d.contains("legacy")
                || d.contains("special")
                || d.contains("collector")
                || d.contains("limited")
                || d.contains("edition")
            // catch "special edition", "deluxe edition" etc
            {
                log::debug!(
                    "Rejecting due to disambiguation indicating special edition: '{disamb}'"
                );
                return false;
            }
        }

        // Second check: only accept releases from the original release year (or within 1 year)
        // This helps filter out later reissues that added bonus tracks without proper disambiguation
        if let Some(rg) = &release.release_group {
            if let (Some(first), Some(date)) = (&rg.first_release_date, &release.date) {
                let first_year = first.0.get(..4).and_then(|y| y.parse::<i32>().ok());
                let rel_year = date.0.get(..4).and_then(|y| y.parse::<i32>().ok());

                if let (Some(fy), Some(ry)) = (first_year, rel_year) {
                    log::debug!("Release year: {ry}, First release year: {fy}");
                    // Allow the original year and one year after (for different regions/formats)
                    if ry > fy + 1 {
                        log::debug!(
                            "Rejecting - release from {ry} is too far after original release year {fy}"
                        );
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Check if a release from a specific ID passes our validation rules
    async fn validate_release(release_id: &str) -> Result<bool, ActionProviderError> {
        let release = Release::fetch()
            .id(release_id)
            .with_release_groups()
            .execute()
            .await
            .map_err(|e| ActionProviderError(format!("Failed to fetch release: {e}")))?;

        log::debug!(
            "Fetched release details - disambiguation: {:?}",
            release.disambiguation
        );

        let accepted = Self::should_accept_release(&release).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(accepted)
    }

    /// Try to find and validate a specific album from a recording's releases
    async fn find_album_in_recording_releases(
        recording_id: &str,
        desired_album: &str,
    ) -> Result<bool, ActionProviderError> {
        let full_rec = MbRecording::fetch()
            .id(recording_id)
            .with_releases()
            .execute()
            .await
            .map_err(|e| ActionProviderError(format!("Failed to fetch recording: {e}")))?;

        let Some(releases) = full_rec.releases else {
            return Ok(false);
        };

        log::debug!("Found {} releases for recording", releases.len());

        let matching_release = releases
            .iter()
            .find(|r| r.title.eq_ignore_ascii_case(desired_album));

        let Some(rel) = matching_release else {
            return Ok(false);
        };

        log::debug!("Found matching release with title '{}'", rel.title);
        Self::validate_release(&rel.id).await
    }

    /// Helper to normalize album titles for comparison
    fn normalize_album_title(title: &str) -> String {
        let lower = title.to_lowercase();
        // strip anything starting with space + ( or [ to the end
        let re = Regex::new(r"\s*(\(|\[).*$").unwrap();
        re.replace(&lower, "").to_string()
    }

    /// Check if album titles match (with normalization)
    fn albums_match(album1: Option<&String>, album2: Option<&String>) -> bool {
        match (album1, album2) {
            (None, _) => true,
            (Some(a1), Some(a2)) => {
                a2.eq_ignore_ascii_case(a1)
                    || Self::normalize_album_title(a2) == Self::normalize_album_title(a1)
            }
            (Some(_), None) => false,
        }
    }

    // Verify that the candidate edit corresponds to a real MB match
    async fn verify_with_musicbrainz(
        mb_provider: &crate::musicbrainz_provider::MusicBrainzScrubActionProvider,
        candidate: &ScrobbleEdit,
        track: &Track,
    ) -> Result<bool, ActionProviderError> {
        let artist = candidate.artist_name.clone();
        let title = candidate
            .track_name
            .clone()
            .unwrap_or_else(|| track.name.clone());
        let album = candidate.album_name.clone();

        log::debug!(
            "MB verify: Starting verification for '{}' by '{}' [{}]",
            title,
            artist,
            album.as_deref().unwrap_or("No Album")
        );

        let search_results = mb_provider
            .search_musicbrainz_multiple(&artist, &title, album.as_deref())
            .await
            .map_err(|e| ActionProviderError(format!("MusicBrainz verification failed: {e}")))?;

        log::debug!("MB verify: Found {} search results", search_results.len());

        // Be gentle to MB API
        tokio::time::sleep(Duration::from_millis(100)).await;

        for result in search_results {
            let artist_match = result.artist.eq_ignore_ascii_case(&artist);
            let title_match = result.title.eq_ignore_ascii_case(&title);
            let album_match = Self::albums_match(album.as_ref(), result.album.as_ref());

            log::debug!(
                "MB result: artist='{}' (match={}), title='{}' (match={}), album={:?} (match={})",
                result.artist,
                artist_match,
                result.title,
                title_match,
                result.album,
                album_match
            );

            // Direct match found
            if artist_match && title_match && album_match {
                log::debug!(
                    "Found potential match - checking release details for release_id: {:?}",
                    result.release_id
                );

                // If we have a release_id, validate it
                if let Some(rel_id) = &result.release_id {
                    log::debug!("Validating release ID: {rel_id}");
                    let is_valid = Self::validate_release(rel_id).await?;
                    if !is_valid {
                        log::debug!("Release {rel_id} was rejected during validation");
                        continue;
                    }
                } else {
                    log::debug!("No release_id available for validation");
                }

                log::debug!("MB verification successful - track confirmed to exist");
                return Ok(true);
            }

            // Fallback: if artist and title match but album doesn't, check recording's releases
            if artist_match && title_match {
                if let Some(desired_album) = &album {
                    let needs_fallback = result.album.is_none()
                        || !result
                            .album
                            .as_ref()
                            .is_some_and(|ma| ma.eq_ignore_ascii_case(desired_album));

                    if needs_fallback {
                        log::debug!(
                            "Album mismatch or missing - attempting fallback search for album '{desired_album}'"
                        );

                        let found =
                            Self::find_album_in_recording_releases(&result.mbid, desired_album)
                                .await?;
                        if found {
                            log::debug!("Fallback: MB verification successful via release fetch");
                            return Ok(true);
                        }

                        // Space out MB requests
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        }

        log::debug!(
            "MB verification failed - no matching track found for '{artist} - {title}' [album: {album:?}]"
        );
        Ok(false)
    }
}

#[async_trait]
impl ScrubActionProvider for RewriteRulesScrubActionProvider {
    type Error = ActionProviderError;

    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        _pending_edits: Option<&[crate::persistence::PendingEdit]>,
        _pending_rules: Option<&[crate::persistence::PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<SuggestionWithContext>)>, Self::Error> {
        let mut results = Vec::new();

        for (index, track) in tracks.iter().enumerate() {
            log::trace!("RewriteRulesScrubActionProvider analyzing track {index}: '{track_name}' by '{track_artist}' against {rules_count} rules",
                   track_name = track.name, track_artist = track.artist, rules_count = self.rules.len());

            // Early continue if no rules apply
            if !crate::rewrite::any_rules_apply(&self.rules, track)? {
                log::trace!(
                    "RewriteRulesScrubActionProvider track {index}: no rules apply, skipping"
                );
                continue;
            }

            // Apply rules with per-rule MB gating
            if let Some((final_edit, requires_confirmation)) =
                self.apply_rules_sequentially(track).await?
            {
                results.push((
                    index,
                    vec![SuggestionWithContext::edit_with_confirmation(
                        final_edit,
                        requires_confirmation,
                        self.provider_name().to_string(),
                    )],
                ));
            }
        }

        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "RewriteRules"
    }
}

/// Combines multiple providers, trying each one in order until one returns a non-NoAction result
pub struct OrScrubActionProvider {
    providers: Vec<Box<dyn ScrubActionProvider<Error = ActionProviderError>>>,
    provider_names: Vec<String>,
}

impl Default for OrScrubActionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl OrScrubActionProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            provider_names: Vec::new(),
        }
    }

    pub fn add_provider<P>(mut self, provider: P) -> Self
    where
        P: ScrubActionProvider + 'static,
        P::Error: Into<ActionProviderError>,
    {
        let name = provider.provider_name().to_string();
        self.provider_names.push(name);

        // Wrap the provider to match our error type
        let wrapped_provider = ErrorAdapter { inner: provider };
        self.providers.push(Box::new(wrapped_provider));
        self
    }

    #[must_use]
    pub fn with_providers<P>(providers: Vec<P>) -> Self
    where
        P: ScrubActionProvider + 'static,
        P::Error: Into<ActionProviderError>,
    {
        let mut or_provider = Self::new();
        for provider in providers {
            or_provider = or_provider.add_provider(provider);
        }
        or_provider
    }
}

// Adapter to convert different error types to our unified error type
struct ErrorAdapter<P> {
    inner: P,
}

#[async_trait]
impl<P> ScrubActionProvider for ErrorAdapter<P>
where
    P: ScrubActionProvider + Send + Sync,
    P::Error: Into<ActionProviderError>,
{
    type Error = ActionProviderError;

    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        pending_edits: Option<&[PendingEdit]>,
        pending_rules: Option<&[PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<SuggestionWithContext>)>, Self::Error> {
        self.inner
            .analyze_tracks(tracks, pending_edits, pending_rules)
            .await
            .map_err(std::convert::Into::into)
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
}

#[async_trait]
impl ScrubActionProvider for OrScrubActionProvider {
    type Error = ActionProviderError;

    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        pending_edits: Option<&[PendingEdit]>,
        pending_rules: Option<&[PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<SuggestionWithContext>)>, Self::Error> {
        let mut combined_results: Vec<(usize, Vec<SuggestionWithContext>)> = Vec::new();

        // Try each provider in sequence and combine results
        for (provider_idx, provider) in self.providers.iter().enumerate() {
            match provider
                .analyze_tracks(tracks, pending_edits, pending_rules)
                .await
            {
                Ok(provider_results) => {
                    // Add these results to our combined results
                    for (track_idx, suggestions) in provider_results {
                        // Check if we already have suggestions for this track
                        if let Some(existing) = combined_results
                            .iter_mut()
                            .find(|(idx, _)| *idx == track_idx)
                        {
                            // Add to existing suggestions
                            existing.1.extend(suggestions);
                        } else {
                            // Add new entry
                            combined_results.push((track_idx, suggestions));
                        }
                    }
                }
                Err(e) => {
                    // Log error but continue to next provider
                    log::warn!(
                        "Error from provider '{}': {}",
                        self.provider_names
                            .get(provider_idx)
                            .unwrap_or(&"unknown".to_string()),
                        e
                    );
                }
            }
        }

        Ok(combined_results)
    }

    fn provider_name(&self) -> &'static str {
        "OrProvider"
    }
}
