#![cfg(feature = "mirror")]

use lastfm_edit::{
    ClientEvent, ExactScrobbleEdit, MockLastFmEditClient, RequestInfo, Track, TrackPage,
};
use scrobble_scrubber::mirror::LibraryMirror;
use scrobble_scrubber_mirror::index::ScrobbleRow;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "scrobble-scrubber-lib-mirror-{prefix}-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn state_file(&self) -> PathBuf {
        self.root.join("state.json")
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn track(name: &str, artist: &str, timestamp: Option<u64>) -> Track {
    Track {
        name: name.to_string(),
        artist: artist.to_string(),
        playcount: 1,
        timestamp,
        album: None,
        album_artist: None,
    }
}

fn track_page(page: u32, tracks: Vec<Track>) -> TrackPage {
    TrackPage {
        tracks,
        page_number: page,
        has_next_page: false,
        total_pages: None,
    }
}

fn scrobble_rows(mirror: &LibraryMirror, limit: usize) -> Vec<ScrobbleRow> {
    mirror.recent_scrobbles(limit, None).unwrap()
}

#[test]
fn append_scrobble_observations_deduplicates_latest_scrobble() {
    let workspace = TempWorkspace::new("dedupe");
    let mirror = LibraryMirror::open("alice", workspace.state_file()).unwrap();

    let duplicate = track("Common Song", "Example Artist", Some(1_700_000_001));
    let missing_ts = track("Skipped Song", "Example Artist", None);

    let written = mirror
        .append_scrobble_observations(&[duplicate.clone(), missing_ts])
        .unwrap();
    assert_eq!(written, 1);

    let first_seen = scrobble_rows(&mirror, 10);
    assert_eq!(first_seen.len(), 1);
    assert_eq!(first_seen[0].played_at, 1_700_000_001);

    thread::sleep(Duration::from_secs(1));
    let re_written = mirror
        .append_scrobble_observations(std::slice::from_ref(&duplicate))
        .unwrap();
    assert_eq!(re_written, 1);

    let reconciled = scrobble_rows(&mirror, 10);
    assert_eq!(reconciled.len(), 1);
    assert_eq!(reconciled[0].played_at, 1_700_000_001);
    assert_eq!(reconciled[0].track, "Common Song");
    assert_eq!(reconciled[0].artist, "Example Artist");
}

#[test]
fn append_edit_attempt_event_only_accepts_edit_attempt_variants() {
    let workspace = TempWorkspace::new("edit-event");
    let mirror = LibraryMirror::open("alice", workspace.state_file()).unwrap();

    let event_edit = ClientEvent::EditAttempted {
        edit: ExactScrobbleEdit {
            track_name_original: "Old Track".to_string(),
            album_name_original: "Old Album".to_string(),
            artist_name_original: "Old Artist".to_string(),
            album_artist_name_original: "Old Album Artist".to_string(),
            track_name: "New Track".to_string(),
            album_name: "New Album".to_string(),
            artist_name: "New Artist".to_string(),
            album_artist_name: "New Album Artist".to_string(),
            timestamp: 1_700_000_111,
            edit_all: true,
        },
        success: true,
        error_message: None,
        duration_ms: 42,
    };

    let non_edit = ClientEvent::RequestCompleted {
        request: RequestInfo::from_url_and_method("https://example.com/api", "GET"),
        status_code: 200,
        duration_ms: 99,
    };

    assert!(mirror
        .append_edit_attempt_from_client_event(&event_edit)
        .unwrap());
    assert!(!mirror
        .append_edit_attempt_from_client_event(&non_edit)
        .unwrap());

    let edits = mirror.recent_edits(10, None).unwrap();
    assert_eq!(edits.len(), 1);
    assert!(edits[0].success);
    assert_eq!(edits[0].played_at, Some(1_700_000_111));
    assert_eq!(edits[0].artist.as_deref(), Some("New Artist"));
    assert_eq!(edits[0].track.as_deref(), Some("New Track"));
    assert_eq!(edits[0].album.as_deref(), Some("New Album"));
}

#[tokio::test]
async fn sync_tip_pages_deduplicates_between_runs() {
    let workspace = TempWorkspace::new("sync-tip");
    let mut client = MockLastFmEditClient::new();

    client.expect_get_recent_tracks_page().returning(|page| {
        Ok(match page {
            1 => track_page(
                1,
                vec![
                    track("Common", "Artist A", Some(3_200)),
                    track("No Timestamp", "Artist A", None),
                ],
            ),
            2 => track_page(
                2,
                vec![
                    track("Common", "Artist A", Some(3_200)),
                    track("Fresh", "Artist B", Some(2_400)),
                ],
            ),
            _ => track_page(page, vec![]),
        })
    });

    let mirror = LibraryMirror::open("alice", workspace.state_file()).unwrap();

    let first = mirror.sync_tip_pages(&client, 2).await.unwrap();
    assert_eq!(first, 3);

    let second = mirror.sync_tip_pages(&client, 2).await.unwrap();
    assert_eq!(second, 3);

    let rows = scrobble_rows(&mirror, 10);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].played_at, 3_200);
    assert_eq!(rows[1].played_at, 2_400);
}

#[tokio::test]
async fn sync_older_pages_falls_back_to_tip_when_no_floor_exists() {
    let workspace = TempWorkspace::new("older-fallback");
    let mut client = MockLastFmEditClient::new();
    let calls = Arc::new(Mutex::new(Vec::<u32>::new()));
    let calls_clone = calls.clone();

    client
        .expect_get_recent_tracks_page()
        .returning(move |page| {
            calls_clone.lock().unwrap().push(page);
            Ok(match page {
                1 => track_page(
                    1,
                    vec![
                        track("Recovered", "Artist", Some(1_600)),
                        track("Ignored", "Artist", None),
                    ],
                ),
                _ => track_page(page, vec![]),
            })
        });

    let mirror = LibraryMirror::open("alice", workspace.state_file()).unwrap();
    let written = mirror.sync_older_pages(&client, 4, 10).await.unwrap();
    assert_eq!(written, 1);

    let calls_seen = calls.lock().unwrap().clone();
    assert_eq!(calls_seen, vec![1]);

    let rows = scrobble_rows(&mirror, 10);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].played_at, 1_600);
}

#[tokio::test]
async fn sync_older_pages_overlap_filter_blocks_recent_tracks() {
    let workspace = TempWorkspace::new("older-overlap-filter");
    let mut client = MockLastFmEditClient::new();

    let mut pages: HashMap<u32, Vec<Track>> = HashMap::new();
    pages.insert(
        1,
        vec![
            track("Still New", "Artist A", Some(2_000)),
            track("Too New", "Artist B", Some(1_500)),
        ],
    );

    let pages = Arc::new(pages);
    let pages = pages.clone();
    client
        .expect_get_recent_tracks_page()
        .returning(move |page| {
            Ok(match pages.get(&page) {
                Some(rows) => track_page(page, rows.clone()),
                None => track_page(page, Vec::new()),
            })
        });

    let mirror = LibraryMirror::open("alice", workspace.state_file()).unwrap();

    let seeded = mirror
        .append_scrobble_observations(&[track("Historical", "Artist B", Some(1_000))])
        .unwrap();
    assert_eq!(seeded, 1);

    let written = mirror.sync_older_pages(&client, 1, 100).await.unwrap();
    assert_eq!(written, 0);

    let rows = scrobble_rows(&mirror, 10);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].played_at, 1_000);
}

#[tokio::test]
async fn sync_older_pages_uses_overlap_filter_and_reconciles_existing_floor() {
    let workspace = TempWorkspace::new("older-reconcile");
    let mut client = MockLastFmEditClient::new();

    let mut pages: HashMap<u32, Vec<Track>> = HashMap::new();
    pages.insert(
        1,
        vec![
            track("Future", "Artist A", Some(6_100)),
            track("Middle", "Artist B", Some(5_900)),
        ],
    );
    pages.insert(
        2,
        vec![
            track("Older", "Artist C", Some(5_000)),
            track("Historical", "Artist B", Some(1_000)),
        ],
    );
    pages.insert(
        3,
        vec![
            track("Never Reached", "Artist Z", Some(700)),
            track("No Timestamp", "Artist Y", None),
        ],
    );

    let pages = Arc::new(pages);
    let pages = pages.clone();

    client
        .expect_get_recent_tracks_page()
        .returning(move |page| {
            Ok(match pages.get(&page) {
                Some(rows) => track_page(page, rows.clone()),
                None => track_page(page, Vec::new()),
            })
        });

    let mirror = LibraryMirror::open("alice", workspace.state_file()).unwrap();

    let seeded = mirror
        .append_scrobble_observations(&[track("Historical", "Artist B", Some(1_000))])
        .unwrap();
    assert_eq!(seeded, 1);

    let written = mirror.sync_older_pages(&client, 2, 5_100).await.unwrap();
    assert_eq!(written, 4);

    let rows = scrobble_rows(&mirror, 10);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].played_at, 6_100);
    assert_eq!(rows[1].played_at, 5_900);
    assert_eq!(rows[2].played_at, 5_000);
    assert_eq!(rows[3].played_at, 1_000);
}

#[test]
fn ingest_repo_rebuilds_missing_index_state() {
    let workspace = TempWorkspace::new("rebuild-index");
    let state_file = workspace.state_file();

    let mirror = LibraryMirror::open("alice", &state_file).unwrap();
    let inserted = mirror
        .append_scrobble_observations(&[track("First", "Artist", Some(10_000))])
        .unwrap();
    assert_eq!(inserted, 1);

    let index_path = mirror.index().path().to_path_buf();
    drop(mirror);

    fs::remove_file(&index_path).unwrap();

    let rebuilt = LibraryMirror::open("alice", &state_file).unwrap();
    let before_rebuild = scrobble_rows(&rebuilt, 10);
    assert!(before_rebuild.is_empty());

    rebuilt.ingest_repo().unwrap();
    let after_rebuild = scrobble_rows(&rebuilt, 10);
    assert_eq!(after_rebuild.len(), 1);
    assert_eq!(after_rebuild[0].played_at, 10_000);
}
