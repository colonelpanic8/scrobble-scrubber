use async_trait::async_trait;
use lastfm_edit::Track;
use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::chat_completion::{
    self, ChatCompletionRequest, Tool, ToolChoiceType, ToolType,
};
use openai_api_rs::v1::common::GPT4_O;
use openai_api_rs::v1::types::{Function, FunctionParameters, JSONSchemaDefine, JSONSchemaType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::rewrite::RewriteRule;

use crate::config::DEFAULT_CLAUDE_SYSTEM_PROMPT;
use crate::scrub_action_provider::{
    ActionProviderError, ScrubActionProvider, ScrubActionSuggestion,
};

#[derive(Deserialize)]
struct ScrobbleEditWithIndex {
    track_index: usize,
    track_name: Option<String>,
    artist_name: Option<String>,
    album_name: Option<String>,
    album_artist_name: Option<String>,
    #[allow(dead_code)]
    reason: String,
}

#[derive(Deserialize)]
struct RewriteRuleSuggestionWithIndex {
    track_index: usize,
    track_name: Option<SdRuleData>,
    album_name: Option<SdRuleData>,
    artist_name: Option<SdRuleData>,
    album_artist_name: Option<SdRuleData>,
    requires_confirmation: Option<bool>,
    motivation: String,
}

/// OpenAI-based action provider using function calling
pub struct OpenAIScrubActionProvider {
    client: Arc<Mutex<OpenAIClient>>,
    model: String,
    system_prompt: String,
    rewrite_rules: Vec<RewriteRule>,
    rule_focus_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RewriteRuleSuggestion {
    /// Optional transformation for track name
    track_name: Option<SdRuleData>,
    /// Optional transformation for album name
    album_name: Option<SdRuleData>,
    /// Optional transformation for artist name
    artist_name: Option<SdRuleData>,
    /// Optional transformation for album artist name
    album_artist_name: Option<SdRuleData>,
    /// Whether this rule requires user confirmation before applying
    requires_confirmation: bool,
    /// Explanation of why this rule would be helpful
    motivation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SdRuleData {
    /// The pattern to search for (regex by default, or literal if `is_literal` is true)
    find: String,
    /// The replacement string (supports $1, $2, ${named}, etc.)
    replace: String,
    /// Whether to use literal string matching instead of regex
    is_literal: bool,
    /// Regex flags (e.g., "i" for case insensitive)
    flags: Option<String>,
    /// Maximum number of replacements (0 = unlimited, defaults to 0 for wholesale replacement)
    #[serde(default)]
    max_replacements: usize,
}

impl From<SdRuleData> for crate::rewrite::SdRule {
    fn from(data: SdRuleData) -> Self {
        let mut sd_rule = if data.is_literal {
            crate::rewrite::SdRule::new_literal(&data.find, &data.replace)
        } else {
            crate::rewrite::SdRule::new_regex(&data.find, &data.replace)
        };

        if let Some(flags) = &data.flags {
            sd_rule = sd_rule.with_flags(flags);
        }

        if data.max_replacements > 0 {
            sd_rule = sd_rule.with_max_replacements(data.max_replacements);
        }

        sd_rule
    }
}

impl RewriteRuleSuggestion {
    /// Convert this suggestion into a `RewriteRule` and motivation pair
    fn into_rule_and_motivation(self) -> (RewriteRule, String) {
        let mut rule = RewriteRule::new();

        if let Some(track_rule) = self.track_name {
            rule = rule.with_track_name(track_rule.into());
        }

        if let Some(album_rule) = self.album_name {
            rule = rule.with_album_name(album_rule.into());
        }

        if let Some(artist_rule) = self.artist_name {
            rule = rule.with_artist_name(artist_rule.into());
        }

        if let Some(album_artist_rule) = self.album_artist_name {
            rule = rule.with_album_artist_name(album_artist_rule.into());
        }

        rule = rule.with_confirmation_required(self.requires_confirmation);

        (rule, self.motivation)
    }
}

impl OpenAIScrubActionProvider {
    pub fn new(
        api_key: String,
        model: Option<String>,
        system_prompt: Option<String>,
        rewrite_rules: Vec<RewriteRule>,
    ) -> Result<Self, ActionProviderError> {
        let client = OpenAIClient::builder()
            .with_api_key(api_key)
            .build()
            .map_err(|e| ActionProviderError(format!("Failed to create OpenAI client: {e}")))?;

        let model = match model.as_deref() {
            Some("gpt-4") => "gpt-4".to_string(),
            Some("gpt-4-turbo") => "gpt-4-turbo".to_string(),
            Some("gpt-4o") => GPT4_O.to_string(),
            Some("gpt-4o-mini") => "gpt-4o-mini".to_string(),
            Some("gpt-3.5-turbo") => "gpt-3.5-turbo".to_string(),
            _ => "gpt-4o-mini".to_string(), // default to GPT-4o mini
        };

        let system_prompt =
            system_prompt.unwrap_or_else(|| DEFAULT_CLAUDE_SYSTEM_PROMPT.to_string());

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            model,
            system_prompt,
            rewrite_rules,
            rule_focus_mode: false,
        })
    }

    /// Enable rule focus mode for pattern analysis
    pub fn enable_rule_focus_mode(&mut self) {
        self.rule_focus_mode = true;
    }

    /// Get the effective system prompt based on current mode
    fn get_effective_system_prompt(&self) -> String {
        if self.rule_focus_mode {
            format!(
                "{}\n\nIMPORTANT: You are in PATTERN ANALYSIS MODE. Your primary goal is to identify patterns across many tracks and suggest rewrite rules that can systematically clean similar issues. Focus heavily on proposing rewrite rules rather than individual track edits. Look for common patterns like:\n- Remastered/version information that should be removed\n- Featuring/collaboration notation that should be standardized\n- Brackets, parentheses, or other formatting inconsistencies\n- Common misspellings or variations in artist/album names\n\nWhen you see the same type of issue across multiple tracks, always prefer suggesting a rewrite rule over individual edits.",
                self.system_prompt
            )
        } else {
            self.system_prompt.clone()
        }
    }

    fn create_edit_function_properties() -> HashMap<String, Box<JSONSchemaDefine>> {
        let mut properties = HashMap::new();

        properties.insert(
            "track_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("The corrected track name".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "artist_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("The corrected artist name".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "album_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("The corrected album name".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "album_artist_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("The corrected album artist name".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "reason".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("Brief explanation of why this change is suggested".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties
    }

    fn create_sd_rule_properties() -> HashMap<String, Box<JSONSchemaDefine>> {
        let mut properties = HashMap::new();

        properties.insert(
            "find".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("The pattern to search for (regex by default, or literal if is_literal is true)".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "replace".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some(
                    "The replacement string (supports $1, $2, ${named}, etc.)".to_string(),
                ),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "is_literal".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Boolean),
                description: Some(
                    "Whether to use literal string matching instead of regex".to_string(),
                ),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "flags".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("Regex flags (e.g., \"i\" for case insensitive)".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "max_replacements".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Number),
                description: Some("Maximum number of replacements (0 = unlimited)".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties
    }

    fn create_rule_function_properties() -> HashMap<String, Box<JSONSchemaDefine>> {
        let mut properties = HashMap::new();

        properties.insert(
            "track_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Object),
                description: Some("Optional transformation for track name".to_string()),
                enum_values: None,
                properties: Some(Self::create_sd_rule_properties()),
                required: Some(vec![
                    "find".to_string(),
                    "replace".to_string(),
                    "is_literal".to_string(),
                ]),
                items: None,
            }),
        );

        properties.insert(
            "album_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Object),
                description: Some("Optional transformation for album name".to_string()),
                enum_values: None,
                properties: Some(Self::create_sd_rule_properties()),
                required: Some(vec![
                    "find".to_string(),
                    "replace".to_string(),
                    "is_literal".to_string(),
                ]),
                items: None,
            }),
        );

        properties.insert(
            "artist_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Object),
                description: Some("Optional transformation for artist name".to_string()),
                enum_values: None,
                properties: Some(Self::create_sd_rule_properties()),
                required: Some(vec![
                    "find".to_string(),
                    "replace".to_string(),
                    "is_literal".to_string(),
                ]),
                items: None,
            }),
        );

        properties.insert(
            "album_artist_name".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Object),
                description: Some("Optional transformation for album artist name".to_string()),
                enum_values: None,
                properties: Some(Self::create_sd_rule_properties()),
                required: Some(vec![
                    "find".to_string(),
                    "replace".to_string(),
                    "is_literal".to_string(),
                ]),
                items: None,
            }),
        );

        properties.insert(
            "requires_confirmation".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Boolean),
                description: Some(
                    "Whether this rule requires user confirmation before applying".to_string(),
                ),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties.insert(
            "motivation".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::String),
                description: Some("Explanation of why this rule would be helpful".to_string()),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        properties
    }

    fn format_existing_rules(&self) -> String {
        if self.rewrite_rules.is_empty() {
            return "EXISTING REWRITE RULES: None configured yet.".to_string();
        }

        match serde_json::to_string_pretty(&self.rewrite_rules) {
            Ok(json) => format!("EXISTING REWRITE RULES:\n{json}"),
            Err(_) => "EXISTING REWRITE RULES: (serialization error)".to_string(),
        }
    }

    /// Analyze tracks with context about pending items (implementation method)
    pub async fn analyze_tracks_with_context_impl(
        &self,
        tracks: &[Track],
        pending_edits: &[crate::persistence::PendingEdit],
        pending_rules: &[crate::persistence::PendingRewriteRule],
    ) -> Result<Vec<(usize, Vec<ScrubActionSuggestion>)>, ActionProviderError> {
        if tracks.is_empty() {
            return Ok(Vec::new());
        }

        let existing_rules = self.format_existing_rules();

        // Format pending edits information
        let pending_edits_info = if pending_edits.is_empty() {
            "PENDING EDITS: None".to_string()
        } else {
            let edits_list = pending_edits
                .iter()
                .map(|edit| {
                    format!(
                        "- \"{}\" by \"{}\" → changes pending approval",
                        edit.original_track_name, edit.original_artist_name
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("PENDING EDITS (already suggested, avoid duplicates):\n{edits_list}")
        };

        // Format pending rewrite rules information
        let pending_rules_info = if pending_rules.is_empty() {
            "PENDING REWRITE RULES: None".to_string()
        } else {
            let rules_list = pending_rules
                .iter()
                .map(|rule| {
                    format!(
                        "- {} (triggered by: \"{}\" by \"{}\")",
                        rule.reason, rule.example_track_name, rule.example_artist_name
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("PENDING REWRITE RULES (already suggested, avoid duplicates):\n{rules_list}")
        };

        // Create a message that includes all tracks for batch analysis
        let tracks_info = tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                let album_info = if let Some(album) = &track.album {
                    format!(" from album \"{album}\"")
                } else {
                    " (no album info)".to_string()
                };
                let timestamp_info = if let Some(timestamp) = track.timestamp {
                    format!(" [scrobbled: {timestamp}]")
                } else {
                    String::new()
                };
                format!(
                    "Track {}: \"{}\" by \"{}\"{}{} (play count: {})",
                    idx, track.name, track.artist, album_info, timestamp_info, track.playcount
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let user_message = format!(
            "Analyze these Last.fm scrobbles and provide suggestions for each track that needs improvement.\n\nIMPORTANT: Check the pending items below to avoid suggesting duplicates.\n\n{tracks_info}\n\n{existing_rules}\n\n{pending_edits_info}\n\n{pending_rules_info}"
        );

        self.make_openai_request(&user_message, tracks).await
    }

    fn process_track_edit_suggestion(
        &self,
        arguments: &str,
        tracks: &[Track],
    ) -> Result<(usize, ScrubActionSuggestion), ActionProviderError> {
        let args: ScrobbleEditWithIndex = serde_json::from_str(arguments)
            .map_err(|e| ActionProviderError(format!("Failed to parse function arguments: {e}")))?;

        if args.track_index >= tracks.len() {
            return Err(ActionProviderError(format!(
                "Invalid track index {} for batch size {}",
                args.track_index,
                tracks.len()
            )));
        }

        let track = &tracks[args.track_index];
        let mut edit = crate::rewrite::create_no_op_edit(track);

        if let Some(track_name) = args.track_name {
            edit.track_name = track_name;
        }
        if let Some(artist_name) = args.artist_name {
            edit.artist_name = artist_name;
        }
        if let Some(album_name) = args.album_name {
            edit.album_name = album_name;
        }
        if let Some(album_artist_name) = args.album_artist_name {
            edit.album_artist_name = album_artist_name;
        }

        Ok((args.track_index, ScrubActionSuggestion::Edit(edit)))
    }

    fn process_rewrite_rule_suggestion(
        &self,
        arguments: &str,
        tracks: &[Track],
    ) -> Result<(usize, ScrubActionSuggestion), ActionProviderError> {
        let args: RewriteRuleSuggestionWithIndex =
            serde_json::from_str(arguments).map_err(|e| {
                ActionProviderError(format!("Failed to parse rewrite rule arguments: {e}"))
            })?;

        if args.track_index >= tracks.len() {
            return Err(ActionProviderError(format!(
                "Invalid track index {} for batch size {}",
                args.track_index,
                tracks.len()
            )));
        }

        let suggestion = RewriteRuleSuggestion {
            track_name: args.track_name,
            album_name: args.album_name,
            artist_name: args.artist_name,
            album_artist_name: args.album_artist_name,
            requires_confirmation: args.requires_confirmation.unwrap_or(false),
            motivation: args.motivation.clone(),
        };

        let (rule, motivation) = suggestion.into_rule_and_motivation();
        Ok((
            args.track_index,
            ScrubActionSuggestion::ProposeRule { rule, motivation },
        ))
    }

    fn add_suggestion_to_results(
        results: &mut Vec<(usize, Vec<ScrubActionSuggestion>)>,
        track_index: usize,
        suggestion: ScrubActionSuggestion,
    ) {
        if let Some(existing) = results.iter_mut().find(|(idx, _)| *idx == track_index) {
            existing.1.push(suggestion);
        } else {
            results.push((track_index, vec![suggestion]));
        }
    }

    fn process_tool_calls(
        &self,
        response: &openai_api_rs::v1::chat_completion::ChatCompletionResponse,
        tracks: &[Track],
        results: &mut Vec<(usize, Vec<ScrubActionSuggestion>)>,
    ) -> Result<(), ActionProviderError> {
        let Some(choice) = response.choices.first() else {
            return Ok(());
        };

        let Some(tool_calls) = &choice.message.tool_calls else {
            return Ok(());
        };

        for tool_call in tool_calls {
            let Some(name) = &tool_call.function.name else {
                continue;
            };

            let Some(arguments) = &tool_call.function.arguments else {
                continue;
            };

            match name.as_str() {
                "suggest_track_edit" => {
                    match self.process_track_edit_suggestion(arguments, tracks) {
                        Ok((track_index, suggestion)) => {
                            Self::add_suggestion_to_results(results, track_index, suggestion);
                        }
                        Err(e) => {
                            log::warn!("Failed to process track edit suggestion: {e}");
                        }
                    }
                }
                "suggest_rewrite_rule" => {
                    match self.process_rewrite_rule_suggestion(arguments, tracks) {
                        Ok((track_index, suggestion)) => {
                            Self::add_suggestion_to_results(results, track_index, suggestion);
                        }
                        Err(e) => {
                            log::warn!("Failed to process rewrite rule suggestion: {e}");
                        }
                    }
                }
                _ => {
                    log::warn!("Unknown function call: {name}");
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ScrubActionProvider for OpenAIScrubActionProvider {
    type Error = ActionProviderError;

    async fn analyze_tracks(
        &self,
        tracks: &[Track],
        pending_edits: Option<&[crate::persistence::PendingEdit]>,
        pending_rules: Option<&[crate::persistence::PendingRewriteRule]>,
    ) -> Result<Vec<(usize, Vec<ScrubActionSuggestion>)>, Self::Error> {
        if tracks.is_empty() {
            return Ok(Vec::new());
        }

        // If context is provided, use the context-aware implementation
        if let (Some(pending_edits), Some(pending_rules)) = (pending_edits, pending_rules) {
            return self
                .analyze_tracks_with_context_impl(tracks, pending_edits, pending_rules)
                .await;
        }

        // Otherwise, use basic analysis without context
        let existing_rules = self.format_existing_rules();

        // Create a message that includes all tracks for batch analysis
        let tracks_info = tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                let album_info = if let Some(album) = &track.album {
                    format!(" from album \"{album}\"")
                } else {
                    " (no album info)".to_string()
                };
                let timestamp_info = if let Some(timestamp) = track.timestamp {
                    format!(" [scrobbled: {timestamp}]")
                } else {
                    String::new()
                };
                format!(
                    "Track {}: \"{}\" by \"{}\"{}{} (play count: {})",
                    idx, track.name, track.artist, album_info, timestamp_info, track.playcount
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let user_message = format!(
            "Analyze these Last.fm scrobbles and provide suggestions for each track that needs improvement:\n\n{tracks_info}\n\n{existing_rules}"
        );

        self.make_openai_request(&user_message, tracks).await
    }

    fn provider_name(&self) -> &'static str {
        "OpenAI"
    }
}

impl OpenAIScrubActionProvider {
    /// Common OpenAI request logic extracted from analyze_tracks
    async fn make_openai_request(
        &self,
        user_message: &str,
        tracks: &[Track],
    ) -> Result<Vec<(usize, Vec<ScrubActionSuggestion>)>, ActionProviderError> {
        // Add track_index parameter to edit function
        let mut edit_properties = Self::create_edit_function_properties();
        edit_properties.insert(
            "track_index".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Number),
                description: Some(
                    "Index of the track this suggestion applies to (0-based)".to_string(),
                ),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        let suggest_edit_function = Function {
            name: "suggest_track_edit".to_string(),
            description: Some(
                "Suggest metadata corrections for a specific track from the batch".to_string(),
            ),
            parameters: FunctionParameters {
                schema_type: JSONSchemaType::Object,
                properties: Some(edit_properties),
                required: Some(vec!["track_index".to_string(), "reason".to_string()]),
            },
        };

        // Add track_index parameter to rule function
        let mut rule_properties = Self::create_rule_function_properties();
        rule_properties.insert(
            "track_index".to_string(),
            Box::new(JSONSchemaDefine {
                schema_type: Some(JSONSchemaType::Number),
                description: Some(
                    "Index of the track that triggered this rule suggestion (0-based)".to_string(),
                ),
                enum_values: None,
                properties: None,
                required: None,
                items: None,
            }),
        );

        let suggest_rule_function = Function {
            name: "suggest_rewrite_rule".to_string(),
            description: Some(
                "Propose a new rewrite rule based on patterns found in the tracks".to_string(),
            ),
            parameters: FunctionParameters {
                schema_type: JSONSchemaType::Object,
                properties: Some(rule_properties),
                required: Some(vec!["track_index".to_string(), "motivation".to_string()]),
            },
        };

        let req = ChatCompletionRequest::new(
            self.model.clone(),
            vec![
                chat_completion::ChatCompletionMessage {
                    role: chat_completion::MessageRole::system,
                    content: chat_completion::Content::Text(self.get_effective_system_prompt()),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                chat_completion::ChatCompletionMessage {
                    role: chat_completion::MessageRole::user,
                    content: chat_completion::Content::Text(user_message.to_string()),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
        )
        .tools(vec![
            Tool {
                r#type: ToolType::Function,
                function: suggest_edit_function,
            },
            Tool {
                r#type: ToolType::Function,
                function: suggest_rule_function,
            },
        ])
        .tool_choice(ToolChoiceType::Auto);

        // Log the request being sent to OpenAI
        log::info!(
            "Making OpenAI request for {} tracks: {}",
            tracks.len(),
            tracks
                .iter()
                .map(|t| format!("\"{}\" by \"{}\"", t.name, t.artist))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let response = self
            .client
            .lock()
            .await
            .chat_completion(req)
            .await
            .map_err(|e| ActionProviderError(format!("OpenAI API error: {e}")))?;

        // Log OpenAI response details
        let tool_calls_count = response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.as_ref())
            .map(|calls| calls.len())
            .unwrap_or(0);
        log::info!("OpenAI response received with {tool_calls_count} tool calls");

        // Log the full response for debugging
        if let Ok(response_json) = serde_json::to_string_pretty(&response) {
            log::debug!("OpenAI response: {response_json}");
        }

        // Log individual tool calls for easier debugging
        if let Some(choice) = response.choices.first() {
            if let Some(tool_calls) = &choice.message.tool_calls {
                for (i, tool_call) in tool_calls.iter().enumerate() {
                    log::info!(
                        "Tool call {}: {} with args: {}",
                        i + 1,
                        tool_call.function.name.as_deref().unwrap_or("unknown"),
                        tool_call.function.arguments.as_deref().unwrap_or("none")
                    );
                }
            }
        }

        let mut results: Vec<(usize, Vec<ScrubActionSuggestion>)> = Vec::new();

        // Process the response
        self.process_tool_calls(&response, tracks, &mut results)?;

        Ok(results)
    }
}
