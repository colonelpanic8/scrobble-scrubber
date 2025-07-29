use crate::persistence::{PendingEdit, PendingRewriteRule, RewriteRulesState};
use crate::rewrite::{RewriteError, RewriteRule};
use async_trait::async_trait;
use lastfm_edit::{ScrobbleEdit, Track};
use std::error::Error;
use std::fmt;

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

            let mut suggestions = Vec::new();

            // Check if any rules would apply
            let rules_apply = crate::rewrite::any_rules_apply(&self.rules, track)?;
            log::trace!("RewriteRulesScrubActionProvider track {index}: rules_apply={rules_apply}");

            if rules_apply {
                // Apply all rules to see what changes would be made
                let mut edit = crate::rewrite::create_no_op_edit(track);
                let changes_made = crate::rewrite::apply_all_rules(&self.rules, &mut edit)?;
                log::trace!(
                    "RewriteRulesScrubActionProvider track {index}: changes_made={changes_made}"
                );

                if changes_made {
                    log::trace!(
                        "RewriteRulesScrubActionProvider track {index}: creating edit suggestion"
                    );

                    // Check if any applicable rule requires individual confirmation
                    let requires_confirmation = self.rules.iter().any(|rule| {
                        let applies = rule.matches(track).unwrap_or(false);
                        let requires_conf = rule.requires_confirmation;
                        log::trace!(
                            "Rule '{}' applies: {}, requires confirmation: {}",
                            rule.name.as_deref().unwrap_or("Unnamed"),
                            applies,
                            requires_conf
                        );
                        applies && requires_conf
                    });

                    suggestions.push(SuggestionWithContext::edit_with_confirmation(
                        edit,
                        requires_confirmation,
                        self.provider_name().to_string(),
                    ));
                }
            } else {
                log::trace!(
                    "RewriteRulesScrubActionProvider track {index}: no rules apply, skipping"
                );
            }

            if !suggestions.is_empty() {
                results.push((index, suggestions));
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
