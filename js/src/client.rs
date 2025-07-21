use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

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

/// Simple Last.fm client for WASM that works entirely client-side
#[wasm_bindgen]
pub struct LastFmClient {
    username: Option<String>,
    password: Option<String>,
    session_key: Option<String>,
}

#[wasm_bindgen]
impl LastFmClient {
    #[wasm_bindgen(constructor)]
    pub fn new() -> LastFmClient {
        LastFmClient {
            username: None,
            password: None,
            session_key: None,
        }
    }

    /// Set credentials for authentication
    #[wasm_bindgen]
    pub fn set_credentials(&mut self, username: String, password: String) {
        self.username = Some(username);
        self.password = Some(password);
    }

    /// Test authentication by attempting to login
    /// Note: This is a mock implementation - real Last.fm auth requires server-side handling
    #[wasm_bindgen]
    pub async fn test_auth(&self) -> JsValue {
        if self.username.is_none() || self.password.is_none() {
            let result = LastFmAuthResult {
                success: false,
                message: "Username and password are required".to_string(),
                session_key: None,
            };
            return serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::null());
        }

        // Mock authentication - in a real implementation, this would involve
        // Last.fm's authentication flow which requires server-side handling
        let result = LastFmAuthResult {
            success: true,
            message: "Mock authentication successful (real auth requires server-side implementation)".to_string(),
            session_key: Some("mock_session_key".to_string()),
        };

        serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::null())
    }

    /// Get mock recent tracks data for testing
    /// In a real implementation, this would fetch from Last.fm API
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

    /// Get mock artist tracks for testing
    #[wasm_bindgen]
    pub fn get_mock_artist_tracks(&self, artist: &str, count: u32) -> JsValue {
        // Generate some mock tracks for the given artist
        let base_tracks = vec![
            ("Track One (2009 Remaster)", "Album One (Deluxe Edition)"),
            ("Track Two - Remastered", "Album Two (Special Edition)"),
            ("Track Three (feat. Other Artist)", "Album Three"),
            ("Track Four - Live Version", "Live Album (Collector's Edition)"),
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