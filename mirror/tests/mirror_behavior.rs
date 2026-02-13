use chrono::{Datelike, TimeZone, Utc};
use scrobble_scrubber_mirror::{
    index::{MirrorIndex, MirrorIndexError, ScrobbleRow},
    records::{
        ActionResult, CustomRecord, EditAttempted, EditTarget, FetchContext, MirrorRecord,
        RecordHeader, RecordSource, Scrobble, ScrobbleObserved, SyncRun,
    },
    repo::{MirrorRepo, MirrorRepoError},
    sink::NullMirrorSink,
    writer::MirrorWriter,
};
use serde_json::{Map, Value};
use std::fs::{create_dir_all, read_to_string};
use std::io::Write;
use tempfile::tempdir;

fn header(user: &str, machine_id: &str) -> RecordHeader {
    RecordHeader {
        version: MirrorRecord::VERSION,
        user: user.to_string(),
        machine_id: machine_id.to_string(),
    }
}

fn scrobble_observed(
    shard_ts: u64,
    played_at: u64,
    user: &str,
    machine_id: &str,
    artist: &str,
    track: &str,
    album: Option<&str>,
) -> MirrorRecord {
    MirrorRecord::ScrobbleObserved(ScrobbleObserved {
        header: header(user, machine_id),
        observed_at: shard_ts,
        source: RecordSource::LastfmApi,
        scrobble: Scrobble {
            played_at,
            artist: artist.to_string(),
            track: track.to_string(),
            album: album.map(ToString::to_string),
            album_artist: None,
        },
        context: Some(FetchContext {
            range_from: Some(shard_ts.saturating_sub(10)),
            range_to: Some(shard_ts),
            page: Some(1),
            position: Some(0),
            url: Some("https://example.invalid/page?x=1".to_string()),
        }),
    })
}

fn sync_run(
    user: &str,
    machine_id: &str,
    started_at: u64,
    finished_at: u64,
    mode: &str,
) -> MirrorRecord {
    MirrorRecord::SyncRun(SyncRun {
        header: header(user, machine_id),
        started_at,
        finished_at,
        mode: mode.to_string(),
        parameters: Map::from_iter([("pages".to_string(), Value::from(1_u64))]),
        emitted_cursors: Map::new(),
    })
}

fn edit_attempted(user: &str, machine_id: &str, attempted_at: u64) -> MirrorRecord {
    MirrorRecord::EditAttempted(EditAttempted {
        header: header(user, machine_id),
        attempted_at,
        target: EditTarget {
            played_at: Some(attempted_at),
            artist: Some("Artist".to_string()),
            track: Some("Track".to_string()),
            album: None,
            album_artist: None,
            artist_original: Some("Artist".to_string()),
            track_original: Some("Track".to_string()),
            album_original: None,
            album_artist_original: None,
        },
        result: ActionResult {
            success: true,
            error: None,
        },
    })
}

fn custom(user: &str, machine_id: &str, at: u64, name: &str) -> MirrorRecord {
    MirrorRecord::Custom(CustomRecord {
        header: header(user, machine_id),
        at,
        name: name.to_string(),
        data: Map::new(),
    })
}

fn expected_shard_path(ts: u64, machine_id: &str, kind: &str) -> std::path::PathBuf {
    let date = Utc.timestamp_opt(ts as i64, 0).single().expect("valid timestamp");
    std::path::PathBuf::from("log")
        .join(kind)
        .join(machine_id)
        .join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{:02}.jsonl", date.day()))
}

#[test]
fn append_all_groups_records_into_shard_paths() {
    let tmp = tempdir().unwrap();
    let repo = MirrorRepo::new(tmp.path());

    let records = vec![
        scrobble_observed(1_700_000_000, 1_700_000_000, "alice", "machine-a", "A", "track", Some("Album")),
        scrobble_observed(1_700_000_001, 1_700_000_001, "alice", "machine-a", "B", "track", None),
        edit_attempted("alice", "machine-a", 1_700_000_002),
        sync_run("alice", "machine-a", 1_700_000_003, 1_700_000_004, "tip"),
        custom("alice", "machine-a", 1_700_000_005, "check"),
    ];

    let rel_paths = repo.append_all(&records).unwrap();
    assert_eq!(rel_paths.len(), 4);

    let expected = vec![
        expected_shard_path(1_700_000_000, "machine-a", "observations"),
        expected_shard_path(1_700_000_002, "machine-a", "actions"),
        expected_shard_path(1_700_000_003, "machine-a", "runs"),
        expected_shard_path(1_700_000_005, "machine-a", "custom"),
    ];

    assert!(rel_paths.contains(&expected[0]));
    assert!(rel_paths.contains(&expected[1]));
    assert!(rel_paths.contains(&expected[2]));
    assert!(rel_paths.contains(&expected[3]));

    assert_eq!(
        read_to_string(repo.root().join(&expected[0]))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(
        read_to_string(repo.root().join(&expected[1]))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        read_to_string(repo.root().join(&expected[2]))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        read_to_string(repo.root().join(&expected[3]))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn append_all_empty_records_is_noop() {
    let tmp = tempdir().unwrap();
    let repo = MirrorRepo::new(tmp.path());
    let rel_paths = repo.append_all(&[]).unwrap();
    assert!(rel_paths.is_empty());
}

#[test]
fn append_rejects_unrepresentable_timestamp() {
    let tmp = tempdir().unwrap();
    let repo = MirrorRepo::new(tmp.path());
    let rec = scrobble_observed(u64::MAX, u64::MAX, "alice", "machine-a", "A", "T", None);

    let err = repo.append(&rec).unwrap_err();
    assert!(matches!(err, MirrorRepoError::InvalidTimestamp(ts) if ts == u64::MAX));
}

#[test]
fn null_sink_returns_empty_path_without_error() {
    let record = scrobble_observed(1_700_000_000, 1_700_000_000, "alice", "machine-a", "A", "T", None);

    let path = NullMirrorSink.append(&record).unwrap();
    assert!(path.as_os_str().is_empty());
}

#[test]
fn writer_records_include_expected_header_and_fields() {
    let tmp = tempdir().unwrap();
    let writer = MirrorWriter::new(MirrorRepo::new(tmp.path()), "alice", "machine-a");

    let observed_rel = writer
        .append_observation(
            1_700_000_010,
            RecordSource::LastfmScrape,
            Scrobble {
                played_at: 1_700_000_010,
                artist: "Artist".to_string(),
                track: "Track".to_string(),
                album: Some("Album".to_string()),
                album_artist: Some("Album Artist".to_string()),
            },
            Some(FetchContext {
                range_from: None,
                range_to: None,
                page: Some(2),
                position: Some(3),
                url: Some("https://example.invalid/observations?x=1".to_string()),
            }),
        )
        .unwrap();

    let custom_rel = writer
        .append_custom(1_700_000_011, "rule", Map::new())
        .unwrap();

    let observations = read_to_string(writer.repo().root().join(&observed_rel)).unwrap();
    let loaded: MirrorRecord = serde_json::from_str(observations.lines().next().unwrap()).unwrap();
    match loaded {
        MirrorRecord::ScrobbleObserved(record) => {
            assert_eq!(record.observed_at, 1_700_000_010);
            assert_eq!(record.scrobble.artist, "Artist");
            assert_eq!(record.scrobble.album_artist.as_deref(), Some("Album Artist"));
            assert_eq!(record.source, RecordSource::LastfmScrape);
            assert_eq!(record.header.machine_id, "machine-a");
            assert_eq!(record.header.user, "alice");
        }
        _ => panic!("expected observed record"),
    }

    let custom = read_to_string(writer.repo().root().join(custom_rel)).unwrap();
    let line = custom.lines().next().unwrap();
    let loaded: MirrorRecord = serde_json::from_str(line).unwrap();
    match loaded {
        MirrorRecord::Custom(record) => {
            assert_eq!(record.at, 1_700_000_011);
            assert_eq!(record.name, "rule");
            assert_eq!(record.header.user, "alice");
            assert_eq!(record.header.machine_id, "machine-a");
            assert_eq!(record.header.version, MirrorRecord::VERSION);
        }
        _ => panic!("expected custom record"),
    }
}

#[test]
fn ingest_repo_deduplicates_by_observation_identity_and_orders_desc() -> Result<(), MirrorIndexError> {
    let tmp = tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let index_path = tmp.path().join("mirror_index.sqlite");
    let repo = MirrorRepo::new(repo_root);

    let user = "alice";
    let machine_id = "machine-a";

    let records = vec![
        scrobble_observed(1_700_000_010, 1_700_000_020, user, machine_id, "Artist", "Track", Some("Album")),
        scrobble_observed(1_700_000_011, 1_700_000_020, user, machine_id, "Artist", "Track", Some("Album")),
        scrobble_observed(1_700_000_012, 1_700_000_030, user, machine_id, "Artist2", "Track2", Some("Album")),
    ];
    repo.append_all(&records)?;

    let index = MirrorIndex::open(index_path)?;
    index.ingest_repo(&repo)?;

    let all = index.recent_scrobbles(user, 10, None)?;
    assert_eq!(
        all,
        vec![
            ScrobbleRow {
                played_at: 1_700_000_030,
                artist: "Artist2".to_string(),
                track: "Track2".to_string(),
                album: Some("Album".to_string()),
                album_artist: None,
            },
            ScrobbleRow {
                played_at: 1_700_000_020,
                artist: "Artist".to_string(),
                track: "Track".to_string(),
                album: Some("Album".to_string()),
                album_artist: None,
            },
        ]
    );

    let before = index.recent_scrobbles(user, 10, Some(1_700_000_030))?;
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].played_at, 1_700_000_020);

    assert_eq!(index.oldest_played_at(user)?, Some(1_700_000_020));
    assert_eq!(index.newest_played_at(user)?, Some(1_700_000_030));

    Ok(())
}

#[test]
fn ingest_paths_is_incremental_and_idempotent_for_unchanged_offsets() -> Result<(), MirrorIndexError> {
    let tmp = tempdir().unwrap();
    let repo = MirrorRepo::new(tmp.path().join("repo"));
    let index = MirrorIndex::open(tmp.path().join("mirror_index.sqlite"))?;
    let user = "alice";

    let first = repo.append(&scrobble_observed(1_700_000_100, 10, user, "m1", "A", "A", None))?;
    index.ingest_paths(&repo, std::slice::from_ref(&first))?;
    assert_eq!(index.recent_scrobbles(user, 10, None)?.len(), 1);

    let second = repo.append(&scrobble_observed(1_700_000_101, 20, user, "m1", "B", "B", None))?;
    index.ingest_paths(&repo, &[second.clone()])?;
    assert_eq!(
        index.recent_scrobbles(user, 10, None)?,
        vec![
            ScrobbleRow {
                played_at: 20,
                artist: "B".to_string(),
                track: "B".to_string(),
                album: None,
                album_artist: None,
            },
            ScrobbleRow {
                played_at: 10,
                artist: "A".to_string(),
                track: "A".to_string(),
                album: None,
                album_artist: None,
            },
        ]
    );

    index.ingest_paths(&repo, &[second.clone()])?;
    assert_eq!(index.recent_scrobbles(user, 10, None)?.len(), 2);

    Ok(())
}

#[test]
fn ingest_repo_rejects_invalid_utf8_lines() {
    let tmp = tempdir().unwrap();
    let repo = MirrorRepo::new(tmp.path().join("repo"));
    let index = MirrorIndex::open(tmp.path().join("mirror_index.sqlite")).unwrap();

    let abs = repo
        .root()
        .join("log/observations/machine-a/1970/01/01.jsonl");
    create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::write(&abs, vec![0xff, b'\n']).unwrap();

    let err = index.ingest_repo(&repo).unwrap_err();
    assert!(matches!(err, MirrorIndexError::InvalidUtf8));
}

#[test]
fn ingest_repo_rebuild_from_scratch_with_shorter_replaced_file() -> Result<(), MirrorIndexError> {
    let tmp = tempdir().unwrap();
    let repo = MirrorRepo::new(tmp.path().join("repo"));
    let index = MirrorIndex::open(tmp.path().join("mirror_index.sqlite"))?;

    let user = "alice";
    let long_album = "x".repeat(2048);

    let long_record = scrobble_observed(
        1_700_000_200,
        1_700_000_201,
        user,
        "machine-a",
        "Artist",
        "Track",
        Some(&long_album),
    );
    let rel = repo.append(&long_record)?;
    index.ingest_repo(&repo)?;
    assert_eq!(index.newest_played_at(user)?, Some(1_700_000_201));

    let replacement = serde_json::to_string(&MirrorRecord::ScrobbleObserved(ScrobbleObserved {
        header: header(user, "machine-a"),
        observed_at: 1_700_000_200,
        source: RecordSource::LastfmApi,
        scrobble: Scrobble {
            played_at: 1_700_000_301,
            artist: "New Artist".to_string(),
            track: "New Track".to_string(),
            album: Some("S".to_string()),
            album_artist: None,
        },
        context: None,
    }))?;
    let abs = repo.root().join(&rel);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&abs)
        .unwrap();
    file.write_all(replacement.as_bytes()).unwrap();
    file.write_all(b"\n").unwrap();

    index.ingest_repo(&repo)?;
    let rows = index.recent_scrobbles(user, 10, None)?;
    assert_eq!(
        rows,
        vec![ScrobbleRow {
            played_at: 1_700_000_301,
            artist: "New Artist".to_string(),
            track: "New Track".to_string(),
            album: Some("S".to_string()),
            album_artist: None,
        }]
    );

    Ok(())
}

#[test]
fn missing_log_root_is_a_noop() {
    let tmp = tempdir().unwrap();
    let repo = MirrorRepo::new(tmp.path().join("repo"));
    let index = MirrorIndex::open(tmp.path().join("mirror_index.sqlite")).unwrap();

    index.ingest_repo(&repo).unwrap();
    assert_eq!(index.newest_played_at("anyone").unwrap(), None);
}
