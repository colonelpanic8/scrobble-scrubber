use crate::types::SerializableTrack;
use dioxus::prelude::*;

#[server(LoginToLastfm)]
pub async fn login_to_lastfm(username: String, password: String) -> Result<String, ServerFnError> {
    use lastfm_edit::LastFmEditClient;

    if username.is_empty() || password.is_empty() {
        return Err(ServerFnError::new("Username and password are required"));
    }

    // Create HTTP client and LastFM client
    let http_client = http_client::native::NativeClient::new();
    let client = LastFmEditClient::new(Box::new(http_client));

    match client.login(&username, &password).await {
        Ok(_) => {
            // Get the session and serialize it
            let session = client.get_session();
            match serde_json::to_string(&session) {
                Ok(session_str) => Ok(session_str),
                Err(e) => Err(ServerFnError::new(format!(
                    "Failed to serialize session: {}",
                    e
                ))),
            }
        }
        Err(e) => Err(ServerFnError::new(format!("Login failed: {}", e))),
    }
}

#[server(LoadRecentTracks)]
pub async fn load_recent_tracks(
    session_str: String,
) -> Result<Vec<SerializableTrack>, ServerFnError> {
    load_recent_tracks_from_page(session_str, 1).await
}

#[server(LoadArtistTracks)]
pub async fn load_artist_tracks(
    session_str: String,
    artist_name: String,
) -> Result<Vec<SerializableTrack>, ServerFnError> {
    use crate::cache::TrackCache;

    // Try to load from cache first
    let mut cache = TrackCache::load();
    if let Some(cached_tracks) = cache.get_artist_tracks(&artist_name) {
        println!("📂 Using cached tracks for artist '{artist_name}'");
        return Ok(cached_tracks.clone());
    }
    use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = match serde_json::from_str(&session_str) {
        Ok(s) => s,
        Err(e) => {
            return Err(ServerFnError::new(format!(
                "Failed to deserialize session: {e}"
            )))
        }
    };

    // Create HTTP client and LastFM client from session
    let http_client = http_client::native::NativeClient::new();
    let mut client = LastFmEditClient::from_session(Box::new(http_client), session);

    // First, fetch all albums for the artist
    let mut albums = Vec::new();

    match tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let mut album_iterator = client.artist_albums(&artist_name);

        while let Some(album) = album_iterator.next().await? {
            albums.push(album);
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await
    {
        Ok(Ok(_)) => {
            // Success - albums were loaded
        }
        Ok(Err(e)) => {
            eprintln!("Error fetching artist albums: {e}");
        }
        Err(_) => {
            eprintln!("Timeout fetching artist albums");
        }
    }

    // Now fetch tracks from each album to preserve album information
    let mut all_tracks = Vec::new();

    for album in albums {
        match client.get_album_tracks(&album.name, &artist_name).await {
            Ok(album_tracks) => {
                for track in album_tracks {
                    all_tracks.push(SerializableTrack::from(track));
                }
            }
            Err(e) => {
                eprintln!("Error fetching tracks for album '{}': {e}", album.name);
                // Continue with other albums instead of failing completely
            }
        }
    }

    if all_tracks.is_empty() {
        return Err(ServerFnError::new(format!(
            "No tracks found for artist '{artist_name}'"
        )));
    }

    // Cache the successfully fetched artist tracks
    cache.cache_artist_tracks(artist_name.clone(), all_tracks.clone());
    if let Err(e) = cache.save() {
        eprintln!("⚠️ Failed to save cache: {}", e);
    }
    println!("💾 Cached tracks for artist '{artist_name}'");

    Ok(all_tracks)
}

#[server(LoadRecentTracksFromPage)]
pub async fn load_recent_tracks_from_page(
    session_str: String,
    page: u32,
) -> Result<Vec<SerializableTrack>, ServerFnError> {
    use crate::cache::TrackCache;

    // Try to load from cache first
    let mut cache = TrackCache::load();
    if let Some(cached_tracks) = cache.get_recent_tracks(page) {
        println!("📂 Using cached recent tracks for page {page}");
        return Ok(cached_tracks.clone());
    }
    use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, LastFmEditSession};

    // Deserialize the session
    let session: LastFmEditSession = match serde_json::from_str(&session_str) {
        Ok(s) => s,
        Err(e) => {
            return Err(ServerFnError::new(format!(
                "Failed to deserialize session: {}",
                e
            )))
        }
    };

    // Create HTTP client and LastFM client from session
    let http_client = http_client::native::NativeClient::new();
    let mut client = LastFmEditClient::from_session(Box::new(http_client), session);

    // Try to fetch real recent tracks from specific page
    let mut tracks = Vec::new();
    let mut recent_iterator = client.recent_tracks_from_page(page);
    let mut count = 0;
    const LIMIT: u32 = 50;

    match tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(track) = recent_iterator.next().await? {
            if count >= LIMIT {
                break;
            }
            tracks.push(SerializableTrack::from(track));
            count += 1;
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await
    {
        Ok(Ok(_)) => {
            if !tracks.is_empty() {
                // Cache the successfully fetched tracks
                cache.cache_recent_tracks(page, tracks.clone());
                if let Err(e) = cache.save() {
                    eprintln!("⚠️ Failed to save cache: {}", e);
                }
                println!("💾 Cached recent tracks for page {page}");
                return Ok(tracks);
            }
        }
        Ok(Err(e)) => {
            eprintln!("Error fetching tracks: {}", e);
        }
        Err(_) => {
            eprintln!("Timeout fetching tracks");
        }
    }

    // Return error if no tracks could be fetched
    Err(ServerFnError::new(format!(
        "Failed to load recent tracks for page {page}"
    )))
}

#[server(GetCacheStats)]
pub async fn get_cache_stats() -> Result<String, ServerFnError> {
    use crate::cache::TrackCache;

    let cache = TrackCache::load();
    let stats = cache.stats();
    Ok(format!("{stats}"))
}

#[server(ClearCache)]
pub async fn clear_cache() -> Result<String, ServerFnError> {
    use crate::cache::TrackCache;

    let mut cache = TrackCache::load();
    cache.clear();
    match cache.save() {
        Ok(_) => Ok("Cache cleared successfully".to_string()),
        Err(e) => Err(ServerFnError::new(format!("Failed to clear cache: {e}"))),
    }
}

#[server(ClearArtistCache)]
pub async fn clear_artist_cache(artist_name: String) -> Result<String, ServerFnError> {
    use crate::cache::TrackCache;

    let mut cache = TrackCache::load();
    cache.clear_artist(&artist_name);
    match cache.save() {
        Ok(_) => Ok(format!("Cleared cache for artist '{artist_name}'")),
        Err(e) => Err(ServerFnError::new(format!(
            "Failed to clear artist cache: {e}"
        ))),
    }
}
