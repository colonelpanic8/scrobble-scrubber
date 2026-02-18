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

async fn sleep_seconds_cancelable(
    seconds: u64,
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<()> {
    // Keep it responsive to stop/cancel without needing to plumb a `Notify` down here.
    for _ in 0..seconds {
        if should_cancel() {
            return Err(cancelled_error());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    Ok(())
}

/// Fetch a page, but if `lastfm-edit` gives up with `RateLimit { retry_after }`, keep waiting and
/// retry until success or cancellation.
///
/// This prevents the scrubber from spinning cycles that repeatedly redo anchor search without ever
/// getting to processing.
pub async fn fetch_page_resilient_cancelable(
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
    mut should_cancel: impl FnMut() -> bool,
) -> Result<Vec<Track>> {
    loop {
        if should_cancel() {
            return Err(cancelled_error());
        }

        match fetch_page(client, page).await {
            Ok(tracks) => return Ok(tracks),
            Err(LastFmError::RateLimit { retry_after }) => {
                log::warn!(
                    "Rate limited fetching page {page}; waiting {retry_after}s and retrying"
                );
                sleep_seconds_cancelable(retry_after, &mut should_cancel).await?;
            }
            Err(e) => return Err(e),
        }
    }
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

async fn page_tracks_resilient_cancelable(
    cache: &mut HashMap<u32, Vec<Track>>,
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<Vec<Track>> {
    if let Some(tracks) = cache.get(&page) {
        return Ok(tracks.clone());
    }
    let tracks = fetch_page_resilient_cancelable(client, page, || should_cancel()).await?;
    cache.insert(page, tracks.clone());
    Ok(tracks)
}

async fn check_page_timestamps_cached_resilient_cancelable(
    cache: &mut HashMap<u32, Vec<Track>>,
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
    threshold: DateTime<Utc>,
    should_cancel: &mut impl FnMut() -> bool,
) -> Result<PageCheckResult> {
    let tracks = page_tracks_resilient_cancelable(cache, client, page, should_cancel).await?;
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
    on_progress: impl FnMut(AnchorSearchProgress),
    should_cancel: impl FnMut() -> bool,
) -> Result<Option<AnchorPageWithTracks>> {
    find_anchor_page_with_tracks_with_progress_cancelable_with_hint(
        client,
        anchor,
        None,
        on_progress,
        should_cancel,
    )
    .await
}

/// Like [`find_anchor_page_with_tracks_with_progress_cancelable`], but starts from a best-effort
/// page hint.
///
/// `page_hint` is expected to be the last known anchor page for the same (or nearby) anchor
/// timestamp. It's treated as advisory only: if it is stale, the search will still converge.
pub async fn find_anchor_page_with_tracks_with_progress_cancelable_with_hint(
    client: &(dyn LastFmEditClient + Send + Sync),
    anchor: DateTime<Utc>,
    page_hint: Option<u32>,
    mut on_progress: impl FnMut(AnchorSearchProgress),
    mut should_cancel: impl FnMut() -> bool,
) -> Result<Option<AnchorPageWithTracks>> {
    fn invalid_hint(h: u32) -> bool {
        h == 0
    }

    async fn binary_search_first_contains_older_or_missing(
        cache: &mut HashMap<u32, Vec<Track>>,
        client: &(dyn LastFmEditClient + Send + Sync),
        anchor: DateTime<Utc>,
        mut low: u32, // predicate false ("all newer"); low==0 is a sentinel that is always false
        mut high: u32, // predicate true ("contains older/equal" or missing)
        on_progress: &mut impl FnMut(AnchorSearchProgress),
        should_cancel: &mut impl FnMut() -> bool,
    ) -> Result<u32> {
        debug_assert!(low < high);
        while low.saturating_add(1) < high {
            if should_cancel() {
                return Err(cancelled_error());
            }
            let mid = low + (high - low) / 2;
            if mid == 0 {
                // Sentinel page 0: never fetch.
                low = 0;
                continue;
            }
            on_progress(AnchorSearchProgress::BinarySearchChecking {
                page: mid,
                low,
                high,
            });
            let result = check_page_timestamps_cached_resilient_cancelable(
                cache,
                client,
                mid,
                anchor,
                should_cancel,
            )
            .await?;
            match result {
                PageCheckResult::AllNewer => low = mid,
                PageCheckResult::ContainsOlderOrEqual | PageCheckResult::PageNotFound => high = mid,
            }
        }
        Ok(high)
    }

    on_progress(AnchorSearchProgress::Started { anchor });
    let mut cache: HashMap<u32, Vec<Track>> = HashMap::new();
    // Phase 1: Exponential search to find upper bound
    let mut last_all_newer_page = 0u32; // Page where all tracks were newer
    let mut probe_page = 1u32;
    // When starting from a useful hint, gallop forward by additive doubling:
    // hint+1, hint+3, hint+7, ... This is much tighter than jumping straight to hint*2.
    let mut local_gallop_step: Option<u32> = None;

    log::debug!("Starting exponential search for anchor at {anchor}");

    if let Some(hint) = page_hint.filter(|h| !invalid_hint(*h)) {
        if should_cancel() {
            return Err(cancelled_error());
        }
        on_progress(AnchorSearchProgress::ProbingPage { page: hint });
        let hint_result = check_page_timestamps_cached_resilient_cancelable(
            &mut cache,
            client,
            hint,
            anchor,
            &mut should_cancel,
        )
        .await;

        match hint_result {
            Ok(PageCheckResult::ContainsOlderOrEqual) => {
                // If hint-1 is all newer, the hint itself is already the exact anchor page.
                if hint == 1 {
                    on_progress(AnchorSearchProgress::BoundaryFound {
                        last_all_newer_page: 0,
                        first_contains_older_or_equal_page: hint,
                    });
                    on_progress(AnchorSearchProgress::FoundAnchorPage { page: hint });
                    if should_cancel() {
                        return Err(cancelled_error());
                    }
                    let tracks = page_tracks_resilient_cancelable(
                        &mut cache,
                        client,
                        hint,
                        &mut should_cancel,
                    )
                    .await?;
                    return Ok(Some(AnchorPageWithTracks { page: hint, tracks }));
                }

                let prev = hint - 1;
                on_progress(AnchorSearchProgress::ProbingPage { page: prev });
                let prev_result = check_page_timestamps_cached_resilient_cancelable(
                    &mut cache,
                    client,
                    prev,
                    anchor,
                    &mut should_cancel,
                )
                .await?;

                match prev_result {
                    PageCheckResult::AllNewer => {
                        on_progress(AnchorSearchProgress::BoundaryFound {
                            last_all_newer_page: prev,
                            first_contains_older_or_equal_page: hint,
                        });
                        on_progress(AnchorSearchProgress::FoundAnchorPage { page: hint });
                        if should_cancel() {
                            return Err(cancelled_error());
                        }
                        let tracks = page_tracks_resilient_cancelable(
                            &mut cache,
                            client,
                            hint,
                            &mut should_cancel,
                        )
                        .await?;
                        return Ok(Some(AnchorPageWithTracks { page: hint, tracks }));
                    }
                    PageCheckResult::ContainsOlderOrEqual => {
                        // Hint is too old; the anchor is earlier than `hint`.
                        on_progress(AnchorSearchProgress::BoundaryFound {
                            last_all_newer_page: 0,
                            first_contains_older_or_equal_page: prev,
                        });
                        let anchor_page = binary_search_first_contains_older_or_missing(
                            &mut cache,
                            client,
                            anchor,
                            0,
                            prev,
                            &mut on_progress,
                            &mut should_cancel,
                        )
                        .await?;
                        on_progress(AnchorSearchProgress::FoundAnchorPage { page: anchor_page });
                        if should_cancel() {
                            return Err(cancelled_error());
                        }
                        let tracks = page_tracks_resilient_cancelable(
                            &mut cache,
                            client,
                            anchor_page,
                            &mut should_cancel,
                        )
                        .await?;
                        return Ok(Some(AnchorPageWithTracks {
                            page: anchor_page,
                            tracks,
                        }));
                    }
                    PageCheckResult::PageNotFound => {
                        // Unexpected (previous page missing while hint exists). Fall through to
                        // the standard search, seeded by this hint.
                        last_all_newer_page = 0;
                        probe_page = hint;
                    }
                }
            }
            Ok(PageCheckResult::AllNewer) => {
                // Anchor is on a later page than the hint; start tight local galloping.
                last_all_newer_page = hint;
                probe_page = hint.saturating_add(1);
                local_gallop_step = Some(1);
            }
            Ok(PageCheckResult::PageNotFound) => {
                // Stale hint beyond end: fall back to standard search starting at page 1.
                probe_page = 1;
                last_all_newer_page = 0;
            }
            Err(e) => return Err(e),
        }
    }

    loop {
        if should_cancel() {
            return Err(cancelled_error());
        }
        on_progress(AnchorSearchProgress::ProbingPage { page: probe_page });
        log::trace!("Probing page {probe_page}");
        let result = check_page_timestamps_cached_resilient_cancelable(
            &mut cache,
            client,
            probe_page,
            anchor,
            &mut should_cancel,
        )
        .await?;

        match result {
            PageCheckResult::AllNewer => {
                // All tracks on this page are newer than anchor, keep searching
                last_all_newer_page = probe_page;
                if let Some(step) = local_gallop_step {
                    let next_step = step.saturating_mul(2).max(1);
                    let next_probe = probe_page.saturating_add(next_step);
                    if next_probe == probe_page {
                        // Saturation guard; fall back to global exponential behavior.
                        local_gallop_step = None;
                        probe_page = probe_page.saturating_mul(2).max(probe_page + 1);
                    } else {
                        probe_page = next_probe;
                        local_gallop_step = Some(next_step);
                    }
                } else {
                    probe_page = probe_page.saturating_mul(2).max(probe_page + 1);
                }
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
                // Anchor is older than all available tracks: every existing page is "AllNewer".
                // We still need the *actual* last existing page so callers can process the entire
                // library (e.g. when starting from an anchor before the first scrobble).
                let mut low = last_all_newer_page; // exists
                let mut high = probe_page; // known missing/empty
                while low.saturating_add(1) < high {
                    if should_cancel() {
                        return Err(cancelled_error());
                    }
                    let mid = low + (high - low) / 2;
                    on_progress(AnchorSearchProgress::BinarySearchChecking {
                        page: mid,
                        low,
                        high,
                    });
                    let tracks = page_tracks_resilient_cancelable(
                        &mut cache,
                        client,
                        mid,
                        &mut should_cancel,
                    )
                    .await?;
                    if tracks.is_empty() {
                        high = mid;
                    } else {
                        low = mid;
                    }
                }

                log::info!("Anchor is older than all tracks; last existing page is {low}");
                on_progress(AnchorSearchProgress::FoundAnchorPage { page: low });
                if should_cancel() {
                    return Err(cancelled_error());
                }
                let tracks =
                    page_tracks_resilient_cancelable(&mut cache, client, low, &mut should_cancel)
                        .await?;
                return Ok(Some(AnchorPageWithTracks { page: low, tracks }));
            }
        }
    }

    // Phase 2: Binary search between last_all_newer_page and probe_page
    let low = last_all_newer_page;
    let high = probe_page;
    let anchor_page = binary_search_first_contains_older_or_missing(
        &mut cache,
        client,
        anchor,
        low,
        high,
        &mut on_progress,
        &mut should_cancel,
    )
    .await?;

    // 'anchor_page' is the first page that contains tracks at or before the anchor
    log::info!("Found anchor page: {anchor_page} (searched {} pages)", {
        // Rough estimate of pages probed
        let exp_probes = (probe_page as f64).log2().ceil() as u32;
        let bin_probes = ((probe_page - last_all_newer_page) as f64).log2().ceil() as u32;
        exp_probes + bin_probes
    });

    on_progress(AnchorSearchProgress::FoundAnchorPage { page: anchor_page });
    if should_cancel() {
        return Err(cancelled_error());
    }
    let tracks =
        page_tracks_resilient_cancelable(&mut cache, client, anchor_page, &mut should_cancel)
            .await?;
    Ok(Some(AnchorPageWithTracks {
        page: anchor_page,
        tracks,
    }))
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
