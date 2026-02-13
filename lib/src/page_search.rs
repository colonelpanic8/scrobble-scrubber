//! Exponential search for finding the anchor page efficiently.
//!
//! When there are many new scrobbles since the last run, this module helps find
//! the page containing the anchor timestamp in O(log N) API calls instead of O(N).

use chrono::{DateTime, Utc};
use lastfm_edit::{LastFmEditClient, LastFmError, Result, Track};
use std::collections::HashMap;

fn cancelled_error() -> LastFmError {
    LastFmError::Io(std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        "cancelled",
    ))
}

/// Result of checking a page for tracks relative to a timestamp threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageCheckResult {
    /// All tracks on the page are newer than the threshold
    AllNewer,
    /// The page contains tracks at or before the threshold (anchor is on this page or earlier)
    ContainsOlderOrEqual,
    /// The page doesn't exist (past the end of available tracks)
    PageNotFound,
}

/// Progress events emitted by the anchor-page search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorSearchProgress {
    /// Starting search for the anchor timestamp.
    Started { anchor: DateTime<Utc> },
    /// Probing a page during exponential search.
    ProbingPage { page: u32 },
    /// Exponential search found a boundary.
    BoundaryFound {
        last_all_newer_page: u32,
        first_contains_older_or_equal_page: u32,
    },
    /// Binary search is checking a specific page.
    BinarySearchChecking { page: u32, low: u32, high: u32 },
    /// Search completed and found the anchor page.
    FoundAnchorPage { page: u32 },
    /// No tracks exist at all.
    NoTracks,
}

/// Fetch all tracks from a specific page.
pub async fn fetch_page(
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
) -> Result<Vec<Track>> {
    // Use the explicit page endpoint to guarantee we only make a single HTTP request.
    Ok(client.get_recent_tracks_page(page).await?.tracks)
}

/// Check if a page contains tracks at or before the given timestamp threshold.
///
/// Returns:
/// - `AllNewer` if all tracks on the page are strictly newer than the threshold
/// - `ContainsOlderOrEqual` if at least one track is at or before the threshold
/// - `PageNotFound` if the page doesn't exist (empty)
pub async fn check_page_timestamps(
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
    threshold: DateTime<Utc>,
) -> Result<PageCheckResult> {
    let tracks = fetch_page(client, page).await?;
    check_page_timestamps_from_tracks(&tracks, threshold)
}

fn check_page_timestamps_from_tracks(
    tracks: &[Track],
    threshold: DateTime<Utc>,
) -> Result<PageCheckResult> {
    if tracks.is_empty() {
        return Ok(PageCheckResult::PageNotFound);
    }

    // Tracks are sorted newest-first, so the last track is the oldest
    let oldest_track = tracks.last().unwrap();

    if let Some(ts) = oldest_track.timestamp {
        if let Some(track_time) = DateTime::from_timestamp(ts as i64, 0) {
            if track_time <= threshold {
                return Ok(PageCheckResult::ContainsOlderOrEqual);
            }
        }
    }

    Ok(PageCheckResult::AllNewer)
}

async fn page_tracks(
    cache: &mut HashMap<u32, Vec<Track>>,
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
) -> Result<Vec<Track>> {
    if let Some(tracks) = cache.get(&page) {
        return Ok(tracks.clone());
    }

    let tracks = fetch_page(client, page).await?;
    cache.insert(page, tracks.clone());
    Ok(tracks)
}

async fn check_page_timestamps_cached(
    cache: &mut HashMap<u32, Vec<Track>>,
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
    threshold: DateTime<Utc>,
) -> Result<PageCheckResult> {
    let tracks = page_tracks(cache, client, page).await?;
    check_page_timestamps_from_tracks(&tracks, threshold)
}

/// Find the page containing the anchor timestamp using exponential search.
///
/// This uses a two-phase approach:
/// 1. Exponential search: probe pages 1, 2, 4, 8, ... until finding one with older tracks
/// 2. Binary search: narrow down between the last "all newer" page and the "contains older" page
///
/// Returns the page number that contains the anchor (or first page with older tracks).
/// Returns `None` if no tracks exist at all.
///
/// # Arguments
/// * `client` - The Last.fm API client
/// * `anchor` - The timestamp to search for (the last processed track's timestamp)
pub async fn find_anchor_page(
    client: &(dyn LastFmEditClient + Send + Sync),
    anchor: DateTime<Utc>,
) -> Result<Option<u32>> {
    find_anchor_page_with_progress(client, anchor, |_| {}).await
}

/// Find the page containing the anchor timestamp using exponential+binary search, with progress.
pub async fn find_anchor_page_with_progress(
    client: &(dyn LastFmEditClient + Send + Sync),
    anchor: DateTime<Utc>,
    on_progress: impl FnMut(AnchorSearchProgress),
) -> Result<Option<u32>> {
    Ok(
        find_anchor_page_with_tracks_with_progress(client, anchor, on_progress)
            .await?
            .map(|r| r.page),
    )
}

/// Anchor page number plus the tracks from that page.
///
/// This exists to avoid an immediate refetch of the anchor page during processing.
#[derive(Debug, Clone)]
pub struct AnchorPageWithTracks {
    pub page: u32,
    pub tracks: Vec<Track>,
}

pub async fn find_anchor_page_with_tracks_with_progress(
    client: &(dyn LastFmEditClient + Send + Sync),
    anchor: DateTime<Utc>,
    on_progress: impl FnMut(AnchorSearchProgress),
) -> Result<Option<AnchorPageWithTracks>> {
    find_anchor_page_with_tracks_with_progress_cancelable(client, anchor, on_progress, || false)
        .await
}

/// Like [`find_anchor_page_with_tracks_with_progress`], but allows cooperative cancellation.
///
/// This is intended for long-running catch-up after downtime: callers can plumb a `stop_requested`
/// flag (or similar) to avoid looking "hung" during multi-page searching.
pub async fn find_anchor_page_with_tracks_with_progress_cancelable(
    client: &(dyn LastFmEditClient + Send + Sync),
    anchor: DateTime<Utc>,
    mut on_progress: impl FnMut(AnchorSearchProgress),
    mut should_cancel: impl FnMut() -> bool,
) -> Result<Option<AnchorPageWithTracks>> {
    on_progress(AnchorSearchProgress::Started { anchor });
    let mut cache: HashMap<u32, Vec<Track>> = HashMap::new();
    // Phase 1: Exponential search to find upper bound
    let mut last_all_newer_page = 0u32; // Page where all tracks were newer
    let mut probe_page = 1u32;

    log::debug!("Starting exponential search for anchor at {anchor}");

    loop {
        if should_cancel() {
            return Err(cancelled_error());
        }
        on_progress(AnchorSearchProgress::ProbingPage { page: probe_page });
        log::trace!("Probing page {probe_page}");
        let result = check_page_timestamps_cached(&mut cache, client, probe_page, anchor).await?;

        match result {
            PageCheckResult::AllNewer => {
                // All tracks on this page are newer than anchor, keep searching
                last_all_newer_page = probe_page;
                probe_page = probe_page.saturating_mul(2).max(probe_page + 1);
            }
            PageCheckResult::ContainsOlderOrEqual => {
                // Found a page with older tracks, anchor is between last_all_newer_page and probe_page
                log::debug!(
                    "Exponential search found boundary: anchor is between pages {last_all_newer_page} and {probe_page}"
                );
                on_progress(AnchorSearchProgress::BoundaryFound {
                    last_all_newer_page,
                    first_contains_older_or_equal_page: probe_page,
                });
                break;
            }
            PageCheckResult::PageNotFound => {
                // Reached end of available tracks
                if last_all_newer_page == 0 {
                    // No tracks exist at all
                    log::debug!("No tracks found");
                    on_progress(AnchorSearchProgress::NoTracks);
                    return Ok(None);
                }
                // Anchor is beyond all available tracks, return the last page with tracks
                log::debug!(
                    "Anchor is beyond all available tracks, last page with tracks: {last_all_newer_page}"
                );
                // Need to find the actual last page with binary search
                probe_page = last_all_newer_page;
                break;
            }
        }
    }

    // Phase 2: Binary search between last_all_newer_page and probe_page
    if last_all_newer_page == 0 {
        // Anchor is on page 1 or earlier
        log::debug!("Anchor found on page 1");
        on_progress(AnchorSearchProgress::FoundAnchorPage { page: 1 });
        if should_cancel() {
            return Err(cancelled_error());
        }
        let tracks = page_tracks(&mut cache, client, 1).await?;
        return Ok(Some(AnchorPageWithTracks { page: 1, tracks }));
    }

    let mut low = last_all_newer_page;
    let mut high = probe_page;

    while low < high {
        if should_cancel() {
            return Err(cancelled_error());
        }
        let mid = low + (high - low) / 2;

        // Check page mid+1 (we know 'low' is all newer, so check the next one)
        let check_page = mid + 1;
        if check_page > high {
            break;
        }

        log::trace!("Binary search: checking page {check_page} (low={low}, high={high})");
        on_progress(AnchorSearchProgress::BinarySearchChecking {
            page: check_page,
            low,
            high,
        });

        if should_cancel() {
            return Err(cancelled_error());
        }
        let result = check_page_timestamps_cached(&mut cache, client, check_page, anchor).await?;

        match result {
            PageCheckResult::AllNewer => {
                // Page check_page is all newer, so anchor is on a later page
                low = check_page;
            }
            PageCheckResult::ContainsOlderOrEqual | PageCheckResult::PageNotFound => {
                // Page check_page contains older tracks or doesn't exist
                high = check_page;
            }
        }
    }

    // 'high' is the first page that contains tracks at or before the anchor
    log::info!("Found anchor page: {high} (searched {} pages)", {
        // Rough estimate of pages probed
        let exp_probes = (probe_page as f64).log2().ceil() as u32;
        let bin_probes = ((probe_page - last_all_newer_page) as f64).log2().ceil() as u32;
        exp_probes + bin_probes
    });

    on_progress(AnchorSearchProgress::FoundAnchorPage { page: high });
    if should_cancel() {
        return Err(cancelled_error());
    }
    let tracks = page_tracks(&mut cache, client, high).await?;
    Ok(Some(AnchorPageWithTracks { page: high, tracks }))
}

/// Information about the anchor page search result.
#[derive(Debug, Clone)]
pub struct AnchorPageInfo {
    /// The page number containing the anchor
    pub page: u32,
    /// Total pages that need to be processed (from anchor page down to page 1)
    pub total_pages_to_process: u32,
}

impl AnchorPageInfo {
    pub fn new(anchor_page: u32) -> Self {
        Self {
            page: anchor_page,
            total_pages_to_process: anchor_page,
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests would go here with mock client
}
