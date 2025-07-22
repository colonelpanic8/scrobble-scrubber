use napi_derive::napi;
use std::sync::{Arc, Mutex};

#[napi(object)]
pub struct Track {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub playcount: u32,
    pub timestamp: Option<f64>,
}

#[napi(object)]
pub struct AuthResult {
    pub success: bool,
    pub message: String,
    pub session_key: Option<String>,
}

impl From<&lastfm_edit::Track> for Track {
    fn from(track: &lastfm_edit::Track) -> Self {
        Track {
            name: track.name.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            playcount: track.playcount,
            timestamp: track.timestamp.map(|t| t as f64),
        }
    }
}

#[napi]
pub struct LastFmEditClient {
    credentials: Arc<Mutex<Option<(String, String)>>>,
    is_authenticated: Arc<Mutex<bool>>,
}

#[napi]
impl LastFmEditClient {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            credentials: Arc::new(Mutex::new(None)),
            is_authenticated: Arc::new(Mutex::new(false)),
        }
    }
}

impl Default for LastFmEditClient {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl LastFmEditClient {
    #[napi]
    pub fn set_credentials(&self, username: String, password: String) {
        *self.credentials.lock().unwrap() = Some((username, password));
    }

    #[napi]
    pub async fn test_auth(&self) -> napi::Result<AuthResult> {
        let credentials = self.credentials.lock().unwrap();
        let Some((username, password)) = credentials.as_ref() else {
            return Ok(AuthResult {
                success: false,
                message: "Username and password are required".to_string(),
                session_key: None,
            });
        };

        // For now, demonstrate with mock authentication
        // The lastfm-edit crate has Send issues that need to be resolved upstream
        if username.is_empty() || password.is_empty() {
            Ok(AuthResult {
                success: false,
                message: "Invalid credentials".to_string(),
                session_key: None,
            })
        } else {
            *self.is_authenticated.lock().unwrap() = true;
            Ok(AuthResult {
                success: true,
                message: "Native Node.js addon authentication successful! (mock for now - real HTTP calls work but lastfm-edit has Send issues)".to_string(),
                session_key: Some("native_authenticated".to_string()),
            })
        }
    }

    #[napi]
    pub async fn get_recent_tracks(&self, count: u32) -> napi::Result<Vec<Track>> {
        let is_authenticated = *self.is_authenticated.lock().unwrap();
        if !is_authenticated {
            return Err(napi::Error::from_reason("Not authenticated"));
        }

        // Return enhanced mock data showing native addon capabilities
        Ok(self.get_mock_recent_tracks(count))
    }

    #[napi]
    pub async fn get_artist_tracks(&self, artist: String, count: u32) -> napi::Result<Vec<Track>> {
        let is_authenticated = *self.is_authenticated.lock().unwrap();
        if !is_authenticated {
            return Err(napi::Error::from_reason("Not authenticated"));
        }

        // Return enhanced mock data showing native addon capabilities
        Ok(self.get_mock_artist_tracks(artist, count))
    }

    #[napi]
    pub fn get_mock_recent_tracks(&self, count: u32) -> Vec<Track> {
        let tracks = vec![
            Track {
                name: "Bohemian Rhapsody (2011 Remaster) [NATIVE ADDON]".to_string(),
                artist: "Queen".to_string(),
                album: Some("A Night at the Opera (Deluxe Edition)".to_string()),
                playcount: 42,
                timestamp: Some(1640995200.0),
            },
            Track {
                name: "Stairway to Heaven - 2012 Remaster".to_string(),
                artist: "Led Zeppelin".to_string(),
                album: Some("Led Zeppelin IV (Deluxe Edition)".to_string()),
                playcount: 89,
                timestamp: Some(1640991600.0),
            },
            Track {
                name: "Hotel California (2013 Remaster)".to_string(),
                artist: "Eagles".to_string(),
                album: Some("Hotel California (40th Anniversary Deluxe Edition)".to_string()),
                playcount: 67,
                timestamp: Some(1640988000.0),
            },
            Track {
                name: "Sweet Child O' Mine (feat. Axl Rose)".to_string(),
                artist: "Guns N' Roses".to_string(),
                album: Some("Appetite for Destruction".to_string()),
                playcount: 156,
                timestamp: Some(1640984400.0),
            },
            Track {
                name: "Thunderstruck - Live 1991".to_string(),
                artist: "AC/DC".to_string(),
                album: Some("Live (Collector's Edition)".to_string()),
                playcount: 203,
                timestamp: Some(1640980800.0),
            },
        ];

        tracks.into_iter().take(count as usize).collect()
    }

    #[napi]
    pub fn get_mock_artist_tracks(&self, artist: String, count: u32) -> Vec<Track> {
        let base_tracks = vec![
            ("Track One (2009 Remaster)", "Album One (Deluxe Edition)"),
            ("Track Two - Remastered", "Album Two (Special Edition)"),
            ("Track Three (feat. Other Artist)", "Album Three"),
            (
                "Track Four - Live Version",
                "Live Album (Collector's Edition)",
            ),
            ("Track Five (Radio Edit)", "Greatest Hits"),
        ];

        base_tracks
            .into_iter()
            .take(count as usize)
            .enumerate()
            .map(|(i, (name, album))| Track {
                name: name.to_string(),
                artist: artist.clone(),
                album: Some(album.to_string()),
                playcount: 10 + (i as u32 * 5),
                timestamp: Some(1640995200.0 - (i as f64 * 3600.0)),
            })
            .collect()
    }
}
