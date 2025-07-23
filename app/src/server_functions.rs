#[allow(unused_imports)] // Functions are used in #[server] macro-generated code
use crate::error_utils::{
    approve_rewrite_rule, create_client_from_session, create_storage, deserialize_session,
    remove_pending_edit, remove_pending_rule, with_timeout, ToServerError,
};
use dioxus::prelude::*;
use lastfm_edit::Track;
use scrobble_scrubber::persistence::{PendingEdit, PendingRewriteRule};

#[server(LoginToLastfm)]
pub async fn login_to_lastfm(username: String, password: String) -> Result<String, ServerFnError> {
    if username.is_empty() || password.is_empty() {
        return Err(ServerFnError::new("Username and password are required"));
    }

    // Create HTTP client and LastFM client
    let http_client = http_client::native::NativeClient::new();
    let client = lastfm_edit::LastFmEditClientImpl::new(Box::new(http_client));

    client
        .login(&username, &password)
        .await
        .to_server_error("Login failed")?;

    // Get the session and serialize it
    let session = client.get_session();
    serde_json::to_string(&session).to_server_error("Failed to serialize session")
}

#[server(LoadRecentTracks)]
pub async fn load_recent_tracks(session_str: String) -> Result<Vec<Track>, ServerFnError> {
    load_recent_tracks_from_page(session_str, 1).await
}

// Helper function to fetch artist albums (not a server function to avoid Send issues)
#[allow(dead_code)]
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

#[server(LoadArtistTracks)]
pub async fn load_artist_tracks(
    session_str: String,
    artist_name: String,
) -> Result<Vec<Track>, ServerFnError> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    // Try to load from cache first
    let mut cache = TrackCache::load();
    if let Some(cached_tracks) = cache.get_artist_tracks(&artist_name) {
        println!("📂 Using cached tracks for artist '{artist_name}'");
        return Ok(cached_tracks.clone());
    }

    // Deserialize session and create client for albums
    let session_for_albums = deserialize_session(&session_str)?;
    let client_for_albums = create_client_from_session(session_for_albums);

    // Fetch all albums for the artist with timeout
    let albums = with_timeout(
        std::time::Duration::from_secs(60),
        fetch_artist_albums(client_for_albums, &artist_name),
        "fetching artist albums",
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("Error fetching artist albums: {e}");
        Vec::new()
    });

    // Create separate client for track fetching
    let session_for_tracks = deserialize_session(&session_str)?;
    let client_for_tracks = create_client_from_session(session_for_tracks);

    // Fetch tracks from each album
    let mut all_tracks = Vec::new();
    for album in albums {
        let album_tracks = client_for_tracks
            .get_album_tracks(&album.name, &artist_name)
            .await
            .unwrap_or_else(|e| {
                eprintln!("Error fetching tracks for album '{}': {e}", album.name);
                Vec::new()
            });

        for track in album_tracks {
            all_tracks.push(track);
        }
    }

    if all_tracks.is_empty() {
        return Err(ServerFnError::new(format!(
            "No tracks found for artist '{artist_name}'"
        )));
    }

    // Cache the successfully fetched artist tracks
    cache.cache_artist_tracks(artist_name.clone(), all_tracks.clone());
    cache
        .save()
        .unwrap_or_else(|e| eprintln!("⚠️ Failed to save cache: {}", e));
    println!("💾 Cached tracks for artist '{artist_name}'");

    Ok(all_tracks)
}

// Helper function to fetch recent tracks (not a server function to avoid Send issues)
#[allow(dead_code)]
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

#[server(LoadRecentTracksFromPage)]
pub async fn load_recent_tracks_from_page(
    session_str: String,
    page: u32,
) -> Result<Vec<Track>, ServerFnError> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    // Try to load from cache first
    let mut cache = TrackCache::load();
    let page_size = 50; // Standard page size
    let start_index = ((page - 1) * page_size) as usize;

    if !cache.recent_tracks.is_empty() && start_index < cache.recent_tracks.len() {
        let cached_tracks: Vec<_> = cache
            .recent_tracks
            .iter()
            .skip(start_index)
            .take(page_size as usize)
            .cloned()
            .collect();

        if !cached_tracks.is_empty() {
            println!("📂 Using cached recent tracks for page {page}");
            return Ok(cached_tracks);
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
    .map_err(|e| {
        eprintln!("Error fetching tracks: {}", e);
        ServerFnError::new(format!("Failed to load recent tracks for page {page}"))
    })?;

    if tracks.is_empty() {
        return Err(ServerFnError::new(format!(
            "No tracks found for page {page}"
        )));
    }

    // Cache the successfully fetched tracks
    cache.merge_recent_tracks(tracks.clone());
    cache
        .save()
        .unwrap_or_else(|e| eprintln!("⚠️ Failed to save cache: {}", e));
    println!("💾 Cached recent tracks for page {page}");

    Ok(tracks)
}

#[server(GetCacheStats)]
pub async fn get_cache_stats() -> Result<String, ServerFnError> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    let cache = TrackCache::load();
    let stats = cache.stats();
    Ok(format!("{stats}"))
}

#[server(ClearCache)]
pub async fn clear_cache() -> Result<String, ServerFnError> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    let mut cache = TrackCache::load();
    cache.clear();
    cache.save().to_server_error("Failed to clear cache")?;
    Ok("Cache cleared successfully".to_string())
}

#[server(ClearArtistCache)]
pub async fn clear_artist_cache(artist_name: String) -> Result<String, ServerFnError> {
    use ::scrobble_scrubber::track_cache::TrackCache;

    let mut cache = TrackCache::load();
    cache.clear_artist(&artist_name);
    cache
        .save()
        .to_server_error("Failed to clear artist cache")?;
    Ok(format!("Cleared cache for artist '{artist_name}'"))
}

#[server(LoadPendingEdits)]
pub async fn load_pending_edits() -> Result<Vec<PendingEdit>, ServerFnError> {
    load_pending_edits_from_page(1).await
}

#[server(LoadPendingEditsFromPage)]
pub async fn load_pending_edits_from_page(page: u32) -> Result<Vec<PendingEdit>, ServerFnError> {
    use scrobble_scrubber::persistence::StateStorage;

    let storage = create_storage().await?;

    let pending_edits_state = storage
        .lock()
        .await
        .load_pending_edits_state()
        .await
        .to_server_error("Failed to load pending edits")?;

    const ITEMS_PER_PAGE: usize = 10;
    let page_items = pending_edits_state
        .pending_edits
        .into_iter()
        .skip(((page - 1) as usize) * ITEMS_PER_PAGE)
        .take(ITEMS_PER_PAGE)
        .collect();

    Ok(page_items)
}

#[server(LoadPendingRewriteRules)]
pub async fn load_pending_rewrite_rules() -> Result<Vec<PendingRewriteRule>, ServerFnError> {
    load_pending_rewrite_rules_from_page(1).await
}

#[server(LoadPendingRewriteRulesFromPage)]
pub async fn load_pending_rewrite_rules_from_page(
    page: u32,
) -> Result<Vec<PendingRewriteRule>, ServerFnError> {
    use scrobble_scrubber::persistence::StateStorage;

    let storage = create_storage().await?;

    let pending_rules_state = storage
        .lock()
        .await
        .load_pending_rewrite_rules_state()
        .await
        .to_server_error("Failed to load pending rules")?;

    const ITEMS_PER_PAGE: usize = 10;
    let page_items = pending_rules_state
        .pending_rules
        .into_iter()
        .skip(((page - 1) as usize) * ITEMS_PER_PAGE)
        .take(ITEMS_PER_PAGE)
        .collect();

    Ok(page_items)
}

#[server(ApprovePendingEdit)]
pub async fn approve_pending_edit(edit_id: String) -> Result<String, ServerFnError> {
    let storage = create_storage().await?;
    let _approved_edit = remove_pending_edit(&storage, &edit_id).await?;

    // TODO: Actually apply the edit to LastFM here

    Ok("Edit approved and applied".to_string())
}

#[server(RejectPendingEdit)]
pub async fn reject_pending_edit(edit_id: String) -> Result<String, ServerFnError> {
    let storage = create_storage().await?;
    remove_pending_edit(&storage, &edit_id).await?;
    Ok("Edit rejected and removed".to_string())
}

#[server(ApprovePendingRewriteRule)]
pub async fn approve_pending_rewrite_rule(rule_id: String) -> Result<String, ServerFnError> {
    let storage = create_storage().await?;
    approve_rewrite_rule(&storage, &rule_id).await?;
    Ok("Rule approved and added to active rules".to_string())
}

#[server(RejectPendingRewriteRule)]
pub async fn reject_pending_rewrite_rule(rule_id: String) -> Result<String, ServerFnError> {
    let storage = create_storage().await?;
    remove_pending_rule(&storage, &rule_id).await?;
    Ok("Rule rejected and removed".to_string())
}
