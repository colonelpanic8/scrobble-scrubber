use lastfm_edit::Track;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Serializable version of Track for caching
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializableTrack {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub timestamp: Option<u64>,
    pub playcount: u32,
}

impl From<Track> for SerializableTrack {
    fn from(track: Track) -> Self {
        Self {
            name: track.name,
            artist: track.artist,
            album: track.album,
            timestamp: track.timestamp,
            playcount: track.playcount,
        }
    }
}

impl From<SerializableTrack> for Track {
    fn from(strack: SerializableTrack) -> Self {
        Self {
            name: strack.name,
            artist: strack.artist,
            album: strack.album,
            album_artist: None, // SerializableTrack doesn't have album_artist
            timestamp: strack.timestamp,
            playcount: strack.playcount,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheMergeStats {
    pub added: usize,
    pub updated: usize,
    pub duplicates: usize,
    pub total_processed: usize,
}

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

impl TrackCache {
    /// Get the cache file path using the config
    fn cache_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        #[cfg(feature = "cli")]
        use crate::config::ScrobbleScrubberConfig;

        // Try to load config to get the proper storage directory
        #[cfg(feature = "cli")]
        {
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

        // Fallback for non-CLI builds
        #[cfg(not(feature = "cli"))]
        {
            Err("Cannot determine cache directory without cli feature".into())
        }
    }

    /// Load cache from disk, returns default cache if file doesn't exist or can't be read
    pub fn load() -> Self {
        match Self::cache_file_path() {
            Ok(path) => {
                match fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<Self>(&content) {
                        Ok(cache) => {
                            log::info!("Loaded track cache from {}", path.display());
                            cache
                        }
                        Err(e) => {
                            log::warn!("Failed to parse cache file: {e}, using empty cache");
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
                log::warn!("Could not determine cache path: {e}, using empty cache");
                Self::default()
            }
        }
    }

    /// Save cache to disk
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::cache_file_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        log::info!("Saved track cache to {}", path.display());
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

    /// Get all recent tracks across all pages, sorted by timestamp (newest first)
    pub fn get_all_recent_tracks(&self) -> Vec<SerializableTrack> {
        let mut tracks: Vec<SerializableTrack> =
            self.recent_tracks.values().flatten().cloned().collect();

        // Sort by timestamp, newest first (higher timestamp = more recent)
        tracks.sort_by(|a, b| {
            match (a.timestamp, b.timestamp) {
                (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts), // Reverse order for newest first
                (Some(_), None) => std::cmp::Ordering::Less, // Tracks with timestamps come first
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        tracks
    }

    /// Get the N most recent tracks
    pub fn get_recent_tracks_limited(&self, limit: usize) -> Vec<SerializableTrack> {
        let all_tracks = self.get_all_recent_tracks();
        all_tracks.into_iter().take(limit).collect()
    }

    /// Merge new tracks from API into the cache
    pub fn merge_recent_tracks(&mut self, new_tracks: Vec<Track>) -> CacheMergeStats {
        let mut stats = CacheMergeStats {
            added: 0,
            updated: 0,
            duplicates: 0,
            total_processed: new_tracks.len(),
        };

        // Convert existing tracks to timestamp-indexed map for efficient merging
        let mut existing_tracks: std::collections::BTreeMap<u64, SerializableTrack> = self
            .get_all_recent_tracks()
            .into_iter()
            .filter_map(|t| t.timestamp.map(|ts| (ts, t)))
            .collect();

        // Process new tracks
        for track in new_tracks {
            let Some(timestamp) = track.timestamp else {
                continue; // Skip tracks without timestamps
            };

            let serializable_track = SerializableTrack::from(track);

            match existing_tracks.entry(timestamp) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    // New track, add it
                    entry.insert(serializable_track);
                    stats.added += 1;
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let existing = entry.get();
                    // Check if it's the same track (same name and artist)
                    if existing.name == serializable_track.name
                        && existing.artist == serializable_track.artist
                    {
                        // Same track, check if we should update (e.g., higher playcount)
                        if serializable_track.playcount > existing.playcount {
                            entry.insert(serializable_track);
                            stats.updated += 1;
                        } else {
                            stats.duplicates += 1;
                        }
                    } else {
                        // Different track with same timestamp - rare but possible
                        // Keep the one with higher playcount as tiebreaker
                        if serializable_track.playcount > existing.playcount {
                            entry.insert(serializable_track);
                            stats.updated += 1;
                        } else {
                            stats.duplicates += 1;
                        }
                    }
                }
            }
        }

        // Rebuild the page-based cache structure from merged data
        self.rebuild_from_timestamp_map(existing_tracks);
        self.update_timestamp();

        log::info!(
            "Cache merge completed: {} added, {} updated, {} duplicates",
            stats.added,
            stats.updated,
            stats.duplicates
        );

        stats
    }

    /// Rebuild page-based structure from timestamp-indexed tracks
    fn rebuild_from_timestamp_map(
        &mut self,
        tracks: std::collections::BTreeMap<u64, SerializableTrack>,
    ) {
        // Clear existing recent tracks
        self.recent_tracks.clear();

        // Convert back to sorted vector (newest first)
        let mut sorted_tracks: Vec<SerializableTrack> = tracks.into_values().collect();
        sorted_tracks.sort_by(|a, b| {
            match (a.timestamp, b.timestamp) {
                (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts), // Reverse order for newest first
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        // Rebuild pages (50 tracks per page, matching API pagination)
        const TRACKS_PER_PAGE: usize = 50;
        for (page_index, chunk) in sorted_tracks.chunks(TRACKS_PER_PAGE).enumerate() {
            let page_number = (page_index + 1) as u32;
            self.recent_tracks.insert(page_number, chunk.to_vec());
        }
    }
}

#[derive(Debug)]
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
        let last_updated = if self.last_updated > 0 {
            chrono::DateTime::from_timestamp(self.last_updated as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Never".to_string()
        };

        write!(
            f,
            "Cache Statistics:\n  Recent: {} pages ({} tracks)\n  Artists: {} artists ({} tracks)\n  Total: {} tracks\n  Last Updated: {}",
            self.recent_pages,
            self.recent_track_count,
            self.artist_count,
            self.artist_track_count,
            self.total_tracks,
            last_updated
        )
    }
}
