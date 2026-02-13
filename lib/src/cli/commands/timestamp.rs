#[cfg(feature = "mirror")]
use crate::mirror::LibraryMirror;
use crate::persistence::StateStorage;
use crate::track_cache::TrackCache;
use lastfm_edit::{LastFmError, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Set timestamp anchor back N tracks from current position
pub async fn set_timestamp_anchor(
    storage: &Arc<Mutex<crate::persistence::FileStorage>>,
    tracks_back: u32,
    state_file: &str,
    username: &str,
) -> Result<()> {
    use chrono::DateTime;

    println!("⏰ Setting Timestamp Anchor");
    println!("=========================");

    #[cfg(feature = "mirror")]
    {
        if !username.trim().is_empty() {
            match LibraryMirror::open(username, state_file) {
                Ok(mirror) => {
                    // Best-effort: ensure index is current with any git-pulled changes.
                    if let Err(e) = mirror.ingest_repo() {
                        log::warn!("Mirror ingest failed, falling back to TrackCache: {e}");
                    } else {
                        let needed = (tracks_back as usize) + 1;
                        match mirror.recent_scrobbles(needed, None) {
                            Ok(rows) if rows.len() >= needed => {
                                let target = &rows[tracks_back as usize];
                                let dt = DateTime::from_timestamp(target.played_at as i64, 0)
                                    .ok_or_else(|| {
                                        LastFmError::Io(std::io::Error::other(
                                            "Invalid timestamp from mirror index",
                                        ))
                                    })?;

                                println!(
                                    "Setting anchor to mirror scrobble '{}' by '{}'",
                                    target.track, target.artist
                                );
                                println!("Timestamp: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));

                                let mut storage = storage.lock().await;
                                let mut frontiers =
                                    storage.load_frontiers_state().await.map_err(|e| {
                                        LastFmError::Io(std::io::Error::other(format!(
                                            "Failed to load frontiers state: {e}"
                                        )))
                                    })?;
                                frontiers.slow_anchor_timestamp = Some(dt);
                                storage
                                    .save_frontiers_state(&frontiers)
                                    .await
                                    .map_err(|e| {
                                        LastFmError::Io(std::io::Error::other(format!(
                                            "Failed to save frontiers state: {e}"
                                        )))
                                    })?;

                                println!("✅ Timestamp anchor set successfully (from mirror)");
                                println!(
                                    "Next scrubber run will process tracks from this point forward"
                                );
                                return Ok(());
                            }
                            Ok(_) => {
                                println!(
                                    "Mirror index does not have enough recent scrobbles; falling back to TrackCache"
                                );
                            }
                            Err(e) => {
                                log::warn!(
                                    "Mirror index query failed, falling back to TrackCache: {e}"
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Mirror open failed, falling back to TrackCache: {e}");
                }
            }
        }
    }

    let cache = TrackCache::load();
    let recent_tracks = cache.get_all_recent_tracks();

    if recent_tracks.is_empty() {
        println!("❌ No recent tracks in cache. Load some tracks first.");
        return Ok(());
    }

    if tracks_back as usize >= recent_tracks.len() {
        println!(
            "❌ Requested to go back {} tracks, but only {} tracks available",
            tracks_back,
            recent_tracks.len()
        );
        return Ok(());
    }

    let target_track = &recent_tracks[tracks_back as usize];

    if let Some(timestamp) = target_track.timestamp {
        let dt = DateTime::from_timestamp(timestamp as i64, 0)
            .ok_or_else(|| LastFmError::Io(std::io::Error::other("Invalid timestamp")))?;

        println!(
            "Setting anchor to track '{}' by '{}'",
            target_track.name, target_track.artist
        );
        println!("Timestamp: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));

        let mut storage = storage.lock().await;
        let mut frontiers = storage.load_frontiers_state().await.map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load frontiers state: {e}"
            )))
        })?;
        frontiers.slow_anchor_timestamp = Some(dt);
        storage
            .save_frontiers_state(&frontiers)
            .await
            .map_err(|e| {
                LastFmError::Io(std::io::Error::other(format!(
                    "Failed to save frontiers state: {e}"
                )))
            })?;

        println!("✅ Timestamp anchor set successfully");
        println!("Next scrubber run will process tracks from this point forward");
    } else {
        println!("❌ Target track has no timestamp information");
    }

    Ok(())
}

/// Set timestamp anchor to a specific timestamp
pub async fn set_timestamp_anchor_to_timestamp(
    storage: &Arc<Mutex<crate::persistence::FileStorage>>,
    timestamp_str: &str,
) -> Result<()> {
    use chrono::DateTime;

    println!("⏰ Setting Timestamp Anchor to Specific Time");
    println!("============================================");

    let dt = DateTime::parse_from_rfc3339(timestamp_str)
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to parse timestamp '{timestamp_str}' (expected ISO 8601 format like '2025-07-22T07:08:00Z'): {e}"
            )))
        })?
        .with_timezone(&chrono::Utc);

    println!("Setting anchor to: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));

    let mut storage = storage.lock().await;
    let mut frontiers = storage.load_frontiers_state().await.map_err(|e| {
        LastFmError::Io(std::io::Error::other(format!(
            "Failed to load frontiers state: {e}"
        )))
    })?;
    frontiers.slow_anchor_timestamp = Some(dt);
    storage
        .save_frontiers_state(&frontiers)
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to save frontiers state: {e}"
            )))
        })?;

    println!("✅ Timestamp anchor set successfully");
    println!("Next scrubber run will process tracks from this point forward");

    Ok(())
}
