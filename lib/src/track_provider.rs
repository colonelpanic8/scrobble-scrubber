use chrono::{DateTime, Utc};
use lastfm_edit::{AsyncPaginatedIterator, LastFmEditClient, Result, Track};

pub use crate::track_cache::TrackCache;

/// Enum-based approach for track provider to avoid trait object issues with async methods
pub enum TrackProvider {
    Cached(CachedTrackProvider),
    Direct(DirectTrackProvider),
}

impl TrackProvider {
    /// Update the provider with latest tracks from the API
    pub async fn update_cache_from_api(
        &mut self,
        client: &(dyn LastFmEditClient + Send + Sync),
        fetch_bound: Option<DateTime<Utc>>,
    ) -> Result<()> {
        match self {
            TrackProvider::Cached(provider) => {
                provider.cache.update_cache_from_api(client, fetch_bound).await
            }
            TrackProvider::Direct(provider) => {
                provider.update_cache_from_api(client, fetch_bound).await
            }
        }
    }

    /// Get all recent tracks (used by the scrubber to find tracks to process)
    pub fn get_all_recent_tracks(&self) -> Vec<Track> {
        match self {
            TrackProvider::Cached(provider) => provider.cache.get_all_recent_tracks(),
            TrackProvider::Direct(provider) => provider.last_tracks.clone(),
        }
    }

    /// Get access to the underlying cache if using CachedTrackProvider
    pub fn cache(&self) -> Option<&TrackCache> {
        match self {
            TrackProvider::Cached(provider) => Some(&provider.cache),
            TrackProvider::Direct(_) => None,
        }
    }

    /// Get mutable access to the underlying cache if using CachedTrackProvider
    pub fn cache_mut(&mut self) -> Option<&mut TrackCache> {
        match self {
            TrackProvider::Cached(provider) => Some(&mut provider.cache),
            TrackProvider::Direct(_) => None,
        }
    }
}

/// Implementation that uses caching to store track data
pub struct CachedTrackProvider {
    cache: crate::track_cache::TrackCache,
}

impl Default for CachedTrackProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedTrackProvider {
    pub fn new() -> Self {
        Self {
            cache: crate::track_cache::TrackCache::load(),
        }
    }

    pub fn from_cache(cache: crate::track_cache::TrackCache) -> Self {
        Self { cache }
    }
}

/// Implementation that queries the client directly each time (no caching)
pub struct DirectTrackProvider {
    /// Store tracks from the last API call for get_all_recent_tracks
    last_tracks: Vec<Track>,
}

impl Default for DirectTrackProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectTrackProvider {
    pub fn new() -> Self {
        Self {
            last_tracks: Vec::new(),
        }
    }

    async fn update_cache_from_api(
        &mut self,
        client: &(dyn LastFmEditClient + Send + Sync),
        fetch_bound: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let mut recent_iterator = client.recent_tracks();
        let mut api_tracks = Vec::new();

        while let Some(track) = recent_iterator.next().await? {
            if let Some(track_ts) = track.timestamp {
                let track_time = DateTime::from_timestamp(track_ts as i64, 0);
                if let Some(track_time) = track_time {
                    // Stop if we've reached our fetch bound
                    if let Some(bound) = fetch_bound {
                        if track_time <= bound {
                            break;
                        }
                    }
                }
            }
            api_tracks.push(track);
        }

        // Sort newest first to match cache behavior
        api_tracks.sort_by(|a, b| {
            match (a.timestamp, b.timestamp) {
                (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts), // Reverse order for newest first
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        self.last_tracks = api_tracks;
        Ok(())
    }
}
