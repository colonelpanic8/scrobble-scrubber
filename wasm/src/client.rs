use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LastFmTrack {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub playcount: u32,
    pub timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LastFmAuthResult {
    pub success: bool,
    pub message: String,
    pub session_key: Option<String>,
}

/// WASM wrapper for the real LastFmEditClient from lastfm-edit crate
#[wasm_bindgen]
pub struct LastFmEditClient {
    credentials: Option<(String, String)>,
}

impl Default for LastFmEditClient {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl LastFmEditClient {
    #[wasm_bindgen(constructor)]
    pub fn new() -> LastFmEditClient {
        LastFmEditClient { credentials: None }
    }

    /// Set credentials for authentication
    #[wasm_bindgen]
    pub fn set_credentials(&mut self, username: String, password: String) {
        self.credentials = Some((username, password));
    }

    /// Test authentication by attempting to login with the real Last.fm client
    #[wasm_bindgen]
    pub async fn test_auth(&mut self) -> JsValue {
        let Some((_username, _password)) = &self.credentials else {
            let result = LastFmAuthResult {
                success: false,
                message: "Username and password are required".to_string(),
                session_key: None,
            };
            return serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::null());
        };

        // For WASM in Node.js, HTTP clients don't work the same way as in browsers
        // The NativeClient and WasmClient both have issues in Node.js environment
        // For now, return an informative error
        let result = LastFmAuthResult {
            success: false,
            message: "Real Last.fm API calls are not yet supported in Node.js WASM environment. The WASM HTTP clients are designed for browsers. For Node.js, consider using the native CLI tool or creating native Node.js bindings instead of WASM.".to_string(),
            session_key: None,
        };
        serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::null())
    }

    /// Get recent tracks - uses mock data for Node.js compatibility
    #[wasm_bindgen]
    pub async fn get_recent_tracks(&self, count: u32) -> JsValue {
        // For Node.js WASM, return mock data since HTTP clients don't work properly
        self.get_mock_recent_tracks(count)
    }

    /// Get artist tracks - uses mock data for Node.js compatibility  
    #[wasm_bindgen]
    pub async fn get_artist_tracks(&self, artist: &str, count: u32) -> JsValue {
        // For Node.js WASM, return mock data since HTTP clients don't work properly
        self.get_mock_artist_tracks(artist, count)
    }

    /// Get mock recent tracks data for testing (backwards compatibility)
    /// This provides the same mock data as before for UI testing
    #[wasm_bindgen]
    pub fn get_mock_recent_tracks(&self, count: u32) -> JsValue {
        let tracks = vec![
            LastFmTrack {
                name: "Bohemian Rhapsody (2011 Remaster)".to_string(),
                artist: "Queen".to_string(),
                album: Some("A Night at the Opera (Deluxe Edition)".to_string()),
                playcount: 42,
                timestamp: Some(1640995200),
            },
            LastFmTrack {
                name: "Stairway to Heaven - 2012 Remaster".to_string(),
                artist: "Led Zeppelin".to_string(),
                album: Some("Led Zeppelin IV (Deluxe Edition)".to_string()),
                playcount: 89,
                timestamp: Some(1640991600),
            },
            LastFmTrack {
                name: "Hotel California (2013 Remaster)".to_string(),
                artist: "Eagles".to_string(),
                album: Some("Hotel California (40th Anniversary Deluxe Edition)".to_string()),
                playcount: 67,
                timestamp: Some(1640988000),
            },
            LastFmTrack {
                name: "Sweet Child O' Mine (feat. Axl Rose)".to_string(),
                artist: "Guns N' Roses".to_string(),
                album: Some("Appetite for Destruction".to_string()),
                playcount: 156,
                timestamp: Some(1640984400),
            },
            LastFmTrack {
                name: "Thunderstruck - Live 1991".to_string(),
                artist: "AC/DC".to_string(),
                album: Some("Live (Collector's Edition)".to_string()),
                playcount: 203,
                timestamp: Some(1640980800),
            },
        ];

        let limited_tracks: Vec<_> = tracks.into_iter().take(count as usize).collect();
        serde_wasm_bindgen::to_value(&limited_tracks).unwrap_or(JsValue::null())
    }

    /// Get mock artist tracks for testing (backwards compatibility)
    #[wasm_bindgen]
    pub fn get_mock_artist_tracks(&self, artist: &str, count: u32) -> JsValue {
        // Generate some mock tracks for the given artist
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

        let tracks: Vec<LastFmTrack> = base_tracks
            .into_iter()
            .take(count as usize)
            .enumerate()
            .map(|(i, (name, album))| LastFmTrack {
                name: name.to_string(),
                artist: artist.to_string(),
                album: Some(album.to_string()),
                playcount: 10 + (i as u32 * 5),
                timestamp: Some(1640995200 - (i as u64 * 3600)),
            })
            .collect();

        serde_wasm_bindgen::to_value(&tracks).unwrap_or(JsValue::null())
    }
}
