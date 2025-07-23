use chrono::{DateTime, Utc};
use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, Track};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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
    /// Recent tracks (ordered newest first)
    pub recent_tracks: Vec<Track>,
    /// Artist tracks by artist name
    pub artist_tracks: HashMap<String, Vec<Track>>,
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
            recent_tracks: Vec::new(),
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
    fn cache_file_path() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        use crate::config::ScrobbleScrubberConfig;

        // Try to load config to get the proper storage directory
        match ScrobbleScrubberConfig::load() {
            Ok(config) => {
                let state_file_path = std::path::Path::new(&config.storage.state_file);
                let cache_dir = state_file_path.parent().ok_or_else(|| {
                    std::io::Error::other("Could not determine parent directory of state file")
                })?;

                fs::create_dir_all(cache_dir)?;
                Ok(cache_dir.join("track_cache.json"))
            }
            Err(_) => {
                // Fallback to XDG cache dir if config can't be loaded
                let cache_dir = dirs::cache_dir()
                    .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
                    .ok_or_else(|| std::io::Error::other("Could not determine cache directory"))?;

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
    pub fn save(&self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let path = Self::cache_file_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        log::info!("Saved track cache to {}", path.display());
        Ok(())
    }

    /// Get recent tracks (limited to first n tracks)
    pub fn get_recent_tracks(&self, limit: usize) -> &[Track] {
        let end = std::cmp::min(limit, self.recent_tracks.len());
        &self.recent_tracks[..end]
    }

    /// Add recent tracks to the cache (merges and maintains order)
    pub fn add_recent_tracks(&mut self, mut tracks: Vec<Track>) {
        // Sort new tracks newest first
        tracks.sort_by(|a, b| {
            match (a.timestamp, b.timestamp) {
                (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts), // Reverse order for newest first
                (Some(_), None) => std::cmp::Ordering::Less, // Tracks with timestamps come first
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        // Add to front of existing tracks and maintain order
        tracks.append(&mut self.recent_tracks);
        self.recent_tracks = tracks;
        self.update_timestamp();
    }

    /// Get artist tracks
    pub fn get_artist_tracks(&self, artist: &str) -> Option<&Vec<Track>> {
        self.artist_tracks.get(artist)
    }

    /// Cache artist tracks
    pub fn cache_artist_tracks(&mut self, artist: String, tracks: Vec<Track>) {
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
        let recent_track_count = self.recent_tracks.len();
        let artist_track_count: usize = self.artist_tracks.values().map(|v| v.len()).sum();

        CacheStats {
            recent_pages: 0, // No longer using pages
            recent_track_count,
            artist_count: self.artist_tracks.len(),
            artist_track_count,
            total_tracks: recent_track_count + artist_track_count,
            last_updated: self.metadata.last_updated,
        }
    }

    /// Get all recent tracks (already sorted newest first)
    pub fn get_all_recent_tracks(&self) -> Vec<Track> {
        self.recent_tracks.clone()
    }

    /// Get the N most recent tracks
    pub fn get_recent_tracks_limited(&self, limit: usize) -> Vec<Track> {
        self.recent_tracks.iter().take(limit).cloned().collect()
    }

    /// Get the timestamp of the most recent track in cache (if any)
    pub fn get_most_recent_timestamp(&self) -> Option<DateTime<Utc>> {
        self.recent_tracks
            .first() // Since tracks are sorted newest first
            .and_then(|track| track.timestamp)
            .and_then(|ts| DateTime::from_timestamp(ts as i64, 0))
    }

    /// Merge new tracks from API into the cache
    pub fn merge_recent_tracks(&mut self, new_tracks: Vec<Track>) -> CacheMergeStats {
        let mut stats = CacheMergeStats {
            added: 0,
            updated: 0,
            duplicates: 0,
            total_processed: new_tracks.len(),
        };

        // Filter tracks with timestamps and sort newest first
        let mut filtered_new_tracks: Vec<Track> = new_tracks
            .into_iter()
            .filter(|track| track.timestamp.is_some()) // Skip tracks without timestamps
            .collect();

        // Sort new tracks newest first
        filtered_new_tracks.sort_by(|a, b| {
            match (a.timestamp, b.timestamp) {
                (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts), // Reverse order for newest first
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        // Simple deduplication: merge with existing tracks, keeping newest and avoiding duplicates
        let mut all_tracks = filtered_new_tracks;
        all_tracks.extend(self.recent_tracks.iter().cloned());

        // Remove duplicates by timestamp, keeping the first occurrence (newest)
        all_tracks.sort_by(|a, b| match (a.timestamp, b.timestamp) {
            (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        all_tracks.dedup_by(|a, b| {
            a.timestamp == b.timestamp && a.name == b.name && a.artist == b.artist
        });

        let old_count = self.recent_tracks.len();
        let new_count = all_tracks.len();
        stats.added = new_count.saturating_sub(old_count);

        self.recent_tracks = all_tracks;
        self.update_timestamp();

        log::info!(
            "Cache merge completed: {} tracks total (simplified merge)",
            self.recent_tracks.len()
        );

        stats
    }

    /// Update cache with latest tracks from Last.fm API
    /// Fetches tracks until we hit EITHER the fetch_bound OR the cache's most recent timestamp
    /// (whichever comes first chronologically). If fetch_bound is None, fetches without lower bound.
    pub async fn update_cache_from_api(
        &mut self,
        client: &(dyn LastFmEditClient + Send + Sync),
        fetch_bound: Option<DateTime<Utc>>,
    ) -> lastfm_edit::Result<()> {
        let mut recent_iterator = client.recent_tracks();
        let mut api_tracks = Vec::new();
        let mut fetched = 0;

        let cache_tip = self.get_most_recent_timestamp();

        // Compute the effective stopping bound - use the more recent (higher) of cache_tip and fetch_bound
        let stop_at = match (cache_tip, fetch_bound) {
            (Some(cache), Some(bound)) => Some(cache.max(bound)),
            (Some(cache), None) => Some(cache),
            (None, Some(bound)) => Some(bound),
            (None, None) => None,
        };

        while let Some(track) = recent_iterator.next().await? {
            fetched += 1;

            if let Some(track_ts) = track.timestamp {
                let track_time = DateTime::from_timestamp(track_ts as i64, 0);
                if let Some(track_time) = track_time {
                    // Stop if we've reached our computed stopping bound
                    if let Some(stop_time) = stop_at {
                        if track_time <= stop_time {
                            let bound_type = match (cache_tip, fetch_bound) {
                                (Some(cache), Some(_bound)) if stop_time == cache => "cache tip",
                                (Some(_), Some(_)) => "fetch bound",
                                (Some(_), None) => "cache tip",
                                (None, Some(_)) => "fetch bound",
                                _ => "unknown bound", // shouldn't happen
                            };
                            info!(
                                "Reached {} at track '{}' by '{}' at {}, stopping fetch after {} tracks",
                                bound_type, track.name, track.artist, track_time, fetched
                            );
                            break;
                        }
                    }
                }
            }

            api_tracks.push(track);
        }

        info!(
            "Fetched {} tracks from API, merging with cache...",
            api_tracks.len()
        );

        // Merge with existing cache
        self.merge_recent_tracks(api_tracks);

        // Save updated cache
        if let Err(e) = self.save() {
            log::warn!("Failed to save updated cache: {e}");
        } else {
            info!("Cache updated and saved successfully");
        }

        Ok(())
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
