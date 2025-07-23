use crate::types::SerializableTrack;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Cache structure for storing track data on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackCache {
    /// Recent tracks by page number
    pub recent_tracks: HashMap<u32, Vec<SerializableTrack>>,
    /// Artist tracks by artist name
    pub artist_tracks: HashMap<String, Vec<SerializableTrack>>,
    /// Cache metadata
    pub metadata: CacheMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// When the cache was last updated
    pub last_updated: u64, // Unix timestamp
    /// Cache format version for future compatibility
    pub version: u32,
}

impl Default for TrackCache {
    fn default() -> Self {
        Self {
            recent_tracks: HashMap::new(),
            artist_tracks: HashMap::new(),
            metadata: CacheMetadata {
                last_updated: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                version: 1,
            },
        }
    }
}

#[allow(dead_code)]
impl TrackCache {
    /// Get the cache file path
    fn cache_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        use scrobble_scrubber::config::ScrobbleScrubberConfig;

        // Try to load config to get the proper storage directory
        match ScrobbleScrubberConfig::load() {
            Ok(config) => {
                let state_file_path = std::path::Path::new(&config.storage.state_file);
                let cache_dir = state_file_path
                    .parent()
                    .ok_or("Could not determine parent directory of state file")?;

                fs::create_dir_all(cache_dir)?;
                Ok(cache_dir.join("track_cache.json"))
            }
            Err(_) => {
                // Fallback to XDG cache dir if config can't be loaded
                let cache_dir = dirs::cache_dir()
                    .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
                    .ok_or("Could not determine cache directory")?;

                let app_cache_dir = cache_dir.join("scrobble-scrubber");
                fs::create_dir_all(&app_cache_dir)?;

                Ok(app_cache_dir.join("track_cache.json"))
            }
        }
    }

    /// Load cache from disk, returns default cache if file doesn't exist or can't be read
    pub fn load() -> Self {
        match Self::cache_file_path() {
            Ok(path) => {
                match fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<Self>(&content) {
                        Ok(cache) => {
                            println!("✅ Loaded track cache from {}", path.display());
                            cache
                        }
                        Err(e) => {
                            eprintln!("⚠️ Failed to parse cache file: {e}, using empty cache");
                            Self::default()
                        }
                    },
                    Err(_) => {
                        // File doesn't exist or can't be read, return default
                        Self::default()
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️ Could not determine cache path: {e}, using empty cache");
                Self::default()
            }
        }
    }

    /// Save cache to disk
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::cache_file_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        println!("💾 Saved track cache to {}", path.display());
        Ok(())
    }

    /// Get recent tracks for a specific page
    pub fn get_recent_tracks(&self, page: u32) -> Option<&Vec<SerializableTrack>> {
        self.recent_tracks.get(&page)
    }

    /// Cache recent tracks for a specific page
    pub fn cache_recent_tracks(&mut self, page: u32, tracks: Vec<SerializableTrack>) {
        self.recent_tracks.insert(page, tracks);
        self.update_timestamp();
    }

    /// Get artist tracks
    pub fn get_artist_tracks(&self, artist: &str) -> Option<&Vec<SerializableTrack>> {
        self.artist_tracks.get(artist)
    }

    /// Cache artist tracks
    pub fn cache_artist_tracks(&mut self, artist: String, tracks: Vec<SerializableTrack>) {
        self.artist_tracks.insert(artist, tracks);
        self.update_timestamp();
    }

    /// Clear all cached data
    pub fn clear(&mut self) {
        self.recent_tracks.clear();
        self.artist_tracks.clear();
        self.update_timestamp();
    }

    /// Clear cached data for a specific artist
    pub fn clear_artist(&mut self, artist: &str) {
        self.artist_tracks.remove(artist);
        self.update_timestamp();
    }

    /// Update the last updated timestamp
    fn update_timestamp(&mut self) {
        self.metadata.last_updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let recent_track_count: usize = self.recent_tracks.values().map(|v| v.len()).sum();
        let artist_track_count: usize = self.artist_tracks.values().map(|v| v.len()).sum();

        CacheStats {
            recent_pages: self.recent_tracks.len(),
            recent_track_count,
            artist_count: self.artist_tracks.len(),
            artist_track_count,
            total_tracks: recent_track_count + artist_track_count,
            last_updated: self.metadata.last_updated,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct CacheStats {
    pub recent_pages: usize,
    pub recent_track_count: usize,
    pub artist_count: usize,
    pub artist_track_count: usize,
    pub total_tracks: usize,
    pub last_updated: u64,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache: {} recent pages ({} tracks), {} artists ({} tracks), total: {} tracks",
            self.recent_pages,
            self.recent_track_count,
            self.artist_count,
            self.artist_track_count,
            self.total_tracks
        )
    }
}
