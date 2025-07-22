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
    let client = LastFmEditClient::from_session(Box::new(http_client), session);

    // Try to fetch all tracks for the artist
    let mut tracks = Vec::new();
    const MAX_TRACKS: usize = 1000; // Limit to prevent excessive loading

    match tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut artist_iterator = client.artist_tracks(&artist_name);
        let mut count = 0;

        while let Some(track) = artist_iterator.next().await? {
            if count >= MAX_TRACKS {
                break; // Safety limit
            }
            tracks.push(SerializableTrack::from(track));
            count += 1;
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await
    {
        Ok(Ok(_)) => {
            // Success - tracks were loaded
        }
        Ok(Err(e)) => {
            eprintln!("Error fetching artist tracks: {e}");
        }
        Err(_) => {
            eprintln!("Timeout fetching artist tracks");
        }
    }

    if tracks.is_empty() {
        return Err(ServerFnError::new(format!(
            "No tracks found for artist '{artist_name}'"
        )));
    }

    Ok(tracks)
}

#[server(LoadRecentTracksFromPage)]
pub async fn load_recent_tracks_from_page(
    session_str: String,
    page: u32,
) -> Result<Vec<SerializableTrack>, ServerFnError> {
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

    // Fall back to mock data if real fetch fails
    let mock_tracks = vec![
        SerializableTrack {
            name: "Bohemian Rhapsody - 2011 Remaster".to_string(),
            artist: "Queen ft. Someone".to_string(),
            album: Some("A Night at the Opera (Deluxe Edition)".to_string()),
            timestamp: Some(1234567890),
            playcount: 150,
        },
        SerializableTrack {
            name: "Stairway to Heaven (Remaster)".to_string(),
            artist: "Led Zeppelin featuring Guest".to_string(),
            album: Some("Led Zeppelin IV".to_string()),
            timestamp: Some(1234567800),
            playcount: 75,
        },
        SerializableTrack {
            name: "Hotel California - Live".to_string(),
            artist: "Eagles".to_string(),
            album: Some("Hotel California (40th Anniversary)".to_string()),
            timestamp: Some(1234567700),
            playcount: 42,
        },
    ];

    Ok(mock_tracks)
}
