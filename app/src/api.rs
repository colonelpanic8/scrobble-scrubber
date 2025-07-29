use crate::error_utils::{
    approve_rewrite_rule, create_client_from_session, create_storage, deserialize_session,
    remove_pending_edit, remove_pending_rule, with_timeout, ToBoxError,
};
use lastfm_edit::{LastFmEditClient, Track};
use scrobble_scrubber::persistence::{PendingEdit, PendingRewriteRule};

pub async fn login_to_lastfm(
    username: String,
    password: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if username.is_empty() || password.is_empty() {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(
            "Username and password are required",
        ));
    }

    // Use SessionManager to create and save session
    use scrobble_scrubber::session_manager::SessionManager;
    let session_manager = SessionManager::new(&username);

    let session = session_manager
        .create_and_save_session(&username, &password)
        .await
        .to_box_error("Login failed")?;

    // Update recent user
    use scrobble_scrubber::recent_user_manager::RecentUserManager;
    let recent_user_manager = RecentUserManager::new();
    let _ = recent_user_manager.update_recent_user(&username);

    // Return serialized session for compatibility
    serde_json::to_string(&session).to_box_error("Failed to serialize session")
}

pub async fn try_restore_session(
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    use scrobble_scrubber::recent_user_manager::RecentUserManager;

    // Get the most recent username
    let recent_user_manager = RecentUserManager::new();
    if let Some(username) = recent_user_manager.get_recent_username() {
        use scrobble_scrubber::session_manager::SessionManager;
        let session_manager = SessionManager::new(&username);

        // Try to restore the session
        if let Some(session) = session_manager.try_restore_session().await {
            // Return serialized session for compatibility
            let session_str =
                serde_json::to_string(&session).to_box_error("Failed to serialize session")?;
            return Ok(Some(session_str));
        }
    }

    Ok(None)
}

#[allow(dead_code)]
pub async fn load_recent_tracks(
    session_str: String,
) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
    load_recent_tracks_from_page(session_str, 1).await
}

// Helper function to fetch artist albums (not a server function to avoid Send issues)
async fn fetch_artist_albums(
    client: lastfm_edit::LastFmEditClientImpl,
    artist_name: &str,
) -> Result<Vec<lastfm_edit::Album>, Box<dyn std::error::Error + Send + Sync>> {
    let mut albums = Vec::new();
    let mut page = 1;
    const MAX_PAGES: u32 = 10; // Reasonable limit

    loop {
        let album_page = client.get_artist_albums_page(artist_name, page).await?;

        if album_page.albums.is_empty() {
            break;
        }

        albums.extend(album_page.albums);

        if !album_page.has_next_page || page >= MAX_PAGES {
            break;
        }

        page += 1;
    }

    Ok(albums)
}

pub async fn load_artist_tracks(
    session_str: String,
    artist_name: String,
) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    // Try to load from cache first
    let mut cache = TrackCache::load();
    if let Some(cached_tracks) = cache.get_artist_tracks(&artist_name) {
        return Ok(cached_tracks.clone());
    }

    // Deserialize session and create client for albums
    let session_for_albums = match deserialize_session(&session_str) {
        Ok(session) => {
            println!("✅ Session deserialized successfully");
            session
        }
        Err(e) => {
            eprintln!("❌ Failed to deserialize session: {e}");
            return Err(e);
        }
    };
    let client_for_albums = create_client_from_session(session_for_albums);

    println!("🔍 Fetching albums for artist: '{artist_name}'");

    // Fetch all albums for the artist with timeout
    let albums = with_timeout(
        std::time::Duration::from_secs(60),
        fetch_artist_albums(client_for_albums, &artist_name),
        "fetching artist albums",
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("❌ Error fetching artist albums: {e}");
        Vec::new()
    });

    println!(
        "📀 Found {} albums for artist '{artist_name}'",
        albums.len()
    );

    // Fetch tracks from each album
    let mut all_tracks = Vec::new();
    let session_for_tracks = deserialize_session(&session_str)?;

    for (idx, album) in albums.iter().enumerate() {
        let client = create_client_from_session(session_for_tracks.clone());
        let album_name = album.name.clone();
        let artist_name_clone = artist_name.clone();

        println!(
            "🎧 Fetching tracks for album {}/{}: '{album_name}'",
            idx + 1,
            albums.len()
        );

        // Use the proper get_album_tracks method with timeout
        let album_tracks = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.get_album_tracks(&album_name, &artist_name_clone),
        )
        .await
        {
            Ok(Ok(tracks)) => {
                println!(
                    "✅ Successfully fetched {} tracks from album '{album_name}'",
                    tracks.len()
                );
                tracks
            }
            Ok(Err(e)) => {
                eprintln!("❌ Error fetching tracks for album '{album_name}': {e}");
                Vec::new()
            }
            Err(_) => {
                eprintln!("⏱️ Timeout fetching tracks for album '{album_name}'");
                Vec::new()
            }
        };

        all_tracks.extend(album_tracks);
    }

    println!("🎵 Total tracks collected: {}", all_tracks.len());

    if all_tracks.is_empty() {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "No tracks found for artist '{artist_name}'"
        )));
    }

    // Cache the successfully fetched artist tracks
    cache.cache_artist_tracks(artist_name.clone(), all_tracks.clone());
    cache
        .save()
        .unwrap_or_else(|e| eprintln!("⚠️ Failed to save cache: {e}"));
    println!("💾 Cached tracks for artist '{artist_name}'");

    Ok(all_tracks)
}

// Helper function to fetch recent tracks (not a server function to avoid Send issues)
async fn fetch_recent_tracks_from_page(
    client: lastfm_edit::LastFmEditClientImpl,
    page: u32,
    limit: u32,
) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
    let tracks = client.get_recent_scrobbles(page).await?;

    // Take only the requested limit
    let limited_tracks = tracks.into_iter().take(limit as usize).collect();

    Ok(limited_tracks)
}

pub async fn load_recent_tracks_from_page(
    session_str: String,
    page: u32,
) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    // Try to load from cache first
    let mut cache = TrackCache::load();
    let page_size = 50; // Standard page size
    let start_index = ((page - 1) * page_size) as usize;

    // Make educated guess: if we have enough tracks cached for this page, use cache
    // If we have 1000 tracks, we likely have pages 1-20 (1000/50)
    let cached_page_count = cache.recent_tracks.len() / (page_size as usize);

    if !cache.recent_tracks.is_empty() {
        // If requested page is within our cached range, serve from cache
        if page <= cached_page_count as u32 && start_index < cache.recent_tracks.len() {
            let cached_tracks: Vec<_> = cache
                .recent_tracks
                .iter()
                .skip(start_index)
                .take(page_size as usize)
                .cloned()
                .collect();

            if !cached_tracks.is_empty() {
                println!("📂 Using cached recent tracks for page {page} (cache has ~{cached_page_count} pages)");
                return Ok(cached_tracks);
            }
        }

        // If we're requesting a page that's likely beyond our cache, but close enough,
        // still check if we have partial data
        if start_index < cache.recent_tracks.len() {
            let cached_tracks: Vec<_> = cache
                .recent_tracks
                .iter()
                .skip(start_index)
                .take(page_size as usize)
                .cloned()
                .collect();

            if cached_tracks.len() == page_size as usize {
                println!("📂 Using cached recent tracks for page {page} (partial cache hit)");
                return Ok(cached_tracks);
            }
        }
    }

    // Deserialize session and create client
    let session = deserialize_session(&session_str)?;
    let client = create_client_from_session(session);

    // Fetch recent tracks with timeout
    const LIMIT: u32 = 50;
    let tracks = with_timeout(
        std::time::Duration::from_secs(10),
        fetch_recent_tracks_from_page(client, page, LIMIT),
        "fetching recent tracks",
    )
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
        eprintln!("Error fetching tracks: {e}");
        format!("Failed to load recent tracks for page {page}").into()
    })?;

    if tracks.is_empty() {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "No tracks found for page {page}"
        )));
    }

    // Cache the successfully fetched tracks
    cache.merge_recent_tracks(tracks.clone());
    cache
        .save()
        .unwrap_or_else(|e| eprintln!("⚠️ Failed to save cache: {e}"));
    println!("💾 Cached recent tracks for page {page}");

    Ok(tracks)
}

pub async fn clear_cache() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    let mut cache = TrackCache::load();
    cache.clear();
    cache.save().to_box_error("Failed to clear cache")?;
    Ok("Cache cleared successfully".to_string())
}

// Load tracks at the end of the cache (for Load Next Page button)
pub async fn load_tracks_at_cache_end(
    session_str: String,
) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    let mut cache = TrackCache::load();

    // Calculate which page to load based on cache size
    let current_count = cache.recent_tracks.len();
    let estimated_page = if current_count > 0 {
        ((current_count / 50) + 1) as u32 // 50 tracks per page, load next page
    } else {
        1u32
    };

    // Deserialize session and create client
    let session = deserialize_session(&session_str)?;
    let client = create_client_from_session(session);

    // Fetch tracks with timeout
    const LIMIT: u32 = 50;
    let tracks = with_timeout(
        std::time::Duration::from_secs(10),
        fetch_recent_tracks_from_page(client, estimated_page, LIMIT),
        "fetching tracks at cache end",
    )
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
        eprintln!("Error fetching tracks at cache end: {e}");
        format!("Failed to load tracks at cache end from page {estimated_page}").into()
    })?;

    if tracks.is_empty() {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "No tracks found at cache end (page {estimated_page})"
        )));
    }

    // Cache the successfully fetched tracks
    cache.merge_recent_tracks(tracks.clone());
    cache
        .save()
        .unwrap_or_else(|e| eprintln!("⚠️ Failed to save cache: {e}"));
    println!("💾 Cached tracks at end from page {estimated_page}");

    Ok(tracks)
}

// Load newer tracks, always bypassing cache and forcing fresh API request
pub async fn load_newer_tracks_fresh(
    session_str: String,
) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    // Deserialize session and create client
    let session = deserialize_session(&session_str)?;
    let client = create_client_from_session(session);

    // Always fetch fresh tracks from page 1 (newest tracks)
    const LIMIT: u32 = 50;
    let tracks = with_timeout(
        std::time::Duration::from_secs(10),
        fetch_recent_tracks_from_page(client, 1, LIMIT),
        "fetching fresh newer tracks",
    )
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
        eprintln!("Error fetching fresh newer tracks: {e}");
        "Failed to load fresh newer tracks from page 1".into()
    })?;

    if tracks.is_empty() {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(
            "No newer tracks found on page 1",
        ));
    }

    // Cache the fresh tracks (this will merge with existing cache)
    let mut cache = TrackCache::load();
    cache.merge_recent_tracks(tracks.clone());
    cache
        .save()
        .unwrap_or_else(|e| eprintln!("⚠️ Failed to save cache: {e}"));
    println!("💾 Cached fresh newer tracks from page 1");

    Ok(tracks)
}

pub async fn load_pending_edits(
) -> Result<Vec<PendingEdit>, Box<dyn std::error::Error + Send + Sync>> {
    use scrobble_scrubber::persistence::StateStorage;

    let storage = create_storage().await?;

    let pending_edits_state = storage
        .lock()
        .await
        .load_pending_edits_state()
        .await
        .to_box_error("Failed to load pending edits")?;

    Ok(pending_edits_state.pending_edits)
}

pub async fn load_pending_rewrite_rules(
) -> Result<Vec<PendingRewriteRule>, Box<dyn std::error::Error + Send + Sync>> {
    use scrobble_scrubber::persistence::StateStorage;

    let storage = create_storage().await?;

    let pending_rules_state = storage
        .lock()
        .await
        .load_pending_rewrite_rules_state()
        .await
        .to_box_error("Failed to load pending rules")?;

    Ok(pending_rules_state.pending_rules)
}

pub async fn approve_pending_edit(
    session_str: String,
    edit_id: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let storage = create_storage().await?;
    let approved_edit = remove_pending_edit(&storage, &edit_id).await?;

    // Deserialize session
    let session = deserialize_session(&session_str)?;

    // Convert PendingEdit to ScrobbleEdit
    let edit = approved_edit.to_scrobble_edit();

    // Apply the edit to Last.fm with timeout
    let result = crate::error_utils::apply_edit_with_timeout(session, edit)
        .await
        .map_err(|e| {
            eprintln!("Error applying edit to Last.fm: {e}");
            e
        })?;

    log::info!("Successfully applied edit to Last.fm: {result:?}");
    Ok("Edit approved and applied to Last.fm".to_string())
}

pub async fn reject_pending_edit(
    edit_id: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let storage = create_storage().await?;
    remove_pending_edit(&storage, &edit_id).await?;
    Ok("Edit rejected and removed".to_string())
}

pub async fn approve_pending_rewrite_rule(
    rule_id: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let storage = create_storage().await?;
    approve_rewrite_rule(&storage, &rule_id).await?;
    Ok("Rule approved and added to active rules".to_string())
}

pub async fn reject_pending_rewrite_rule(
    rule_id: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let storage = create_storage().await?;
    remove_pending_rule(&storage, &rule_id).await?;
    Ok("Rule rejected and removed".to_string())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MusicBrainzResult {
    pub mbid: String,
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub confidence: f32,
}

pub async fn search_musicbrainz_for_track(
    artist: String,
    title: String,
    album: Option<String>,
) -> Result<Vec<MusicBrainzResult>, Box<dyn std::error::Error + Send + Sync>> {
    use scrobble_scrubber::musicbrainz_provider::MusicBrainzScrubActionProvider;

    log::info!("Searching MusicBrainz for: '{title}' by '{artist}'");

    // Create provider instance
    let provider = MusicBrainzScrubActionProvider::default();

    // Use the new search method
    let matches = provider
        .search_musicbrainz_multiple(&artist, &title, album.as_deref())
        .await?;

    // Convert to API result format
    let results = matches
        .into_iter()
        .map(|m| MusicBrainzResult {
            mbid: m.mbid,
            artist: m.artist,
            title: m.title,
            album: m.album,
            confidence: m.confidence,
        })
        .collect();

    Ok(results)
}

pub async fn suggest_rule_for_track(
    _session_str: String,
    track: Track,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use scrobble_scrubber::config::ScrobbleScrubberConfig;
    use scrobble_scrubber::openai_provider::OpenAIScrubActionProvider;
    use scrobble_scrubber::persistence::{PendingRewriteRule, StateStorage};
    use scrobble_scrubber::scrub_action_provider::{ScrubActionProvider, ScrubActionSuggestion};
    use uuid::Uuid;

    log::info!(
        "Requesting LLM rule suggestion for: '{}' by '{}'",
        track.name,
        track.artist
    );

    // Load configuration to get OpenAI settings
    let config =
        ScrobbleScrubberConfig::load().unwrap_or_else(|_| ScrobbleScrubberConfig::default());

    let openai_config = match config.providers.openai {
        Some(config) => config,
        None => return Err("OpenAI provider not configured".into()),
    };

    if openai_config.api_key.trim().is_empty() {
        return Err("OpenAI API key not configured".into());
    }

    // Load existing rules and pending items for context
    let storage = create_storage().await?;
    let saved_rules = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| format!("Failed to load saved rules: {e}"))?
        .rewrite_rules;
    let pending_edits = storage
        .lock()
        .await
        .load_pending_edits_state()
        .await
        .map_err(|e| format!("Failed to load pending edits: {e}"))?
        .pending_edits;
    let pending_rules = storage
        .lock()
        .await
        .load_pending_rewrite_rules_state()
        .await
        .map_err(|e| format!("Failed to load pending rules: {e}"))?
        .pending_rules;

    // Create OpenAI provider with rule focus mode
    let mut provider = OpenAIScrubActionProvider::new(
        openai_config.api_key,
        openai_config.model,
        openai_config.system_prompt,
        saved_rules,
    )
    .map_err(|e| format!("Failed to create OpenAI provider: {e}"))?;

    // Enable rule focus mode for pattern analysis
    provider.enable_rule_focus_mode();

    // Analyze the single track for rule suggestions
    let suggestions = provider
        .analyze_tracks(&[track.clone()], Some(&pending_edits), Some(&pending_rules))
        .await
        .map_err(|e| format!("Failed to get LLM suggestions: {e}"))?;

    // Process suggestions and add rule suggestions to pending rules
    let mut rules_added = 0;
    for (track_index, track_suggestions) in suggestions {
        if track_index != 0 {
            continue; // Should only be index 0 since we passed one track
        }

        for suggestion_with_context in track_suggestions {
            if let ScrubActionSuggestion::ProposeRule { rule, motivation } =
                suggestion_with_context.suggestion
            {
                let pending_rule = PendingRewriteRule {
                    id: Uuid::new_v4().to_string(),
                    rule,
                    reason: motivation.clone(),
                    example_track_name: track.name.clone(),
                    example_artist_name: track.artist.clone(),
                    example_album_name: track.album.clone(),
                    example_album_artist_name: track.album_artist.clone(),
                };

                // Load current pending rules, add the new one, and save back
                let mut current_pending_rules = storage
                    .lock()
                    .await
                    .load_pending_rewrite_rules_state()
                    .await
                    .map_err(|e| format!("Failed to load pending rules: {e}"))?;
                current_pending_rules.pending_rules.push(pending_rule);
                storage
                    .lock()
                    .await
                    .save_pending_rewrite_rules_state(&current_pending_rules)
                    .await
                    .map_err(|e| format!("Failed to save pending rule: {e}"))?;

                rules_added += 1;
                log::info!("Added pending rule suggestion: {motivation}");
            }
        }
    }

    if rules_added == 0 {
        Ok("No rule suggestions generated for this track".to_string())
    } else {
        Ok(format!(
            "Added {rules_added} rule suggestion(s) to pending rules"
        ))
    }
}
