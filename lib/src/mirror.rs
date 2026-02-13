use chrono::{DateTime, Utc};
use lastfm_edit::{
    ClientEvent, ExactScrobbleEdit, LastFmEditClient, Result as LastFmResult, Track,
};
use std::path::{Path, PathBuf};

use scrobble_scrubber_mirror::index::{MirrorIndex, MirrorIndexError, ScrobbleRow};
use scrobble_scrubber_mirror::records::{
    ActionResult, EditAttempted, EditTarget, FetchContext, MirrorRecord, RecordHeader,
    RecordSource, Scrobble, ScrobbleObserved, SyncRun,
};
use scrobble_scrubber_mirror::repo::{MirrorRepo, MirrorRepoError};

#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    #[error("mirror repo error: {0}")]
    Repo(#[from] MirrorRepoError),
    #[error("mirror index error: {0}")]
    Index(#[from] MirrorIndexError),
    #[error("lastfm error: {0}")]
    Lastfm(#[from] lastfm_edit::LastFmError),
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(u64),
}

#[derive(Debug, Clone)]
pub struct MirrorPaths {
    pub repo_root: PathBuf,
    pub index_path: PathBuf,
}

impl MirrorPaths {
    pub fn from_state_file(state_file: impl AsRef<Path>, user: &str) -> Self {
        let state_file = state_file.as_ref();
        let base = state_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let repo_root = base.join("mirror").join(user);
        let index_path = repo_root.join("mirror_index.sqlite");
        Self {
            repo_root,
            index_path,
        }
    }
}

fn default_machine_id() -> String {
    if let Ok(v) = std::env::var("SCROBBLE_SCRUBBER_MACHINE_ID") {
        if !v.trim().is_empty() {
            return v;
        }
    }

    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "unknown-machine".to_string())
}

#[derive(Debug, Clone)]
pub struct LibraryMirror {
    repo: MirrorRepo,
    index: MirrorIndex,
    user: String,
    machine_id: String,
}

impl LibraryMirror {
    pub fn open(
        user: impl Into<String>,
        state_file: impl AsRef<Path>,
    ) -> Result<Self, MirrorError> {
        let user = user.into();
        let paths = MirrorPaths::from_state_file(state_file, &user);
        let repo = MirrorRepo::new(&paths.repo_root);
        let index = MirrorIndex::open(&paths.index_path)?;
        Ok(Self {
            repo,
            index,
            user,
            machine_id: default_machine_id(),
        })
    }

    pub fn repo(&self) -> &MirrorRepo {
        &self.repo
    }

    pub fn index(&self) -> &MirrorIndex {
        &self.index
    }

    pub fn ingest_repo(&self) -> Result<(), MirrorError> {
        self.index.ingest_repo(&self.repo)?;
        Ok(())
    }

    pub fn append_scrobble_observations_from_page(
        &self,
        page: u32,
        tracks: &[Track],
    ) -> Result<usize, MirrorError> {
        let observed_at = Utc::now().timestamp() as u64;
        let mut records = Vec::new();

        for (pos, t) in tracks.iter().enumerate() {
            let Some(ts) = t.timestamp else {
                continue;
            };
            records.push(self.make_observation_record(
                observed_at,
                RecordSource::LastfmScrape,
                t,
                Some(FetchContext {
                    range_from: None,
                    range_to: None,
                    page: Some(page),
                    position: Some(pos as u32),
                    url: None,
                }),
                ts,
            ));
        }

        self.append_records(&records)?;
        Ok(records.len())
    }

    pub fn append_scrobble_observations(&self, tracks: &[Track]) -> Result<usize, MirrorError> {
        let observed_at = Utc::now().timestamp() as u64;
        let mut records = Vec::new();

        for t in tracks {
            let Some(ts) = t.timestamp else {
                continue;
            };
            records.push(self.make_observation_record(
                observed_at,
                RecordSource::LastfmScrape,
                t,
                None,
                ts,
            ));
        }

        self.append_records(&records)?;
        Ok(records.len())
    }

    pub fn append_edit_attempt_from_client_event(
        &self,
        event: &ClientEvent,
    ) -> Result<bool, MirrorError> {
        let ClientEvent::EditAttempted {
            edit,
            success,
            error_message,
            duration_ms: _,
        } = event
        else {
            return Ok(false);
        };

        let attempted_at = Utc::now().timestamp() as u64;
        let record = MirrorRecord::EditAttempted(EditAttempted {
            header: self.header(),
            attempted_at,
            target: edit_target_from_exact_scrobble_edit(edit),
            result: ActionResult {
                success: *success,
                error: error_message.clone(),
            },
        });

        self.append_records(std::slice::from_ref(&record))?;
        Ok(true)
    }

    pub fn recent_scrobbles(
        &self,
        limit: usize,
        before_played_at: Option<u64>,
    ) -> Result<Vec<ScrobbleRow>, MirrorError> {
        Ok(self
            .index
            .recent_scrobbles(&self.user, limit, before_played_at)?)
    }

    pub fn oldest_played_at(&self) -> Result<Option<u64>, MirrorError> {
        Ok(self.index.oldest_played_at(&self.user)?)
    }

    pub fn newest_played_at(&self) -> Result<Option<u64>, MirrorError> {
        Ok(self.index.newest_played_at(&self.user)?)
    }

    pub async fn sync_tip_pages(
        &self,
        client: &(dyn LastFmEditClient + Send + Sync),
        pages: u32,
    ) -> Result<usize, MirrorError> {
        let started_at = Utc::now().timestamp() as u64;
        let mut records = Vec::new();
        let observed_at = started_at;

        for page in 1..=pages.max(1) {
            let tracks = fetch_recent_page(client, page).await?;
            for (pos, t) in tracks.iter().enumerate() {
                if let Some(ts) = t.timestamp {
                    records.push(self.make_observation_record(
                        observed_at,
                        RecordSource::LastfmScrape,
                        t,
                        Some(FetchContext {
                            range_from: None,
                            range_to: None,
                            page: Some(page),
                            position: Some(pos as u32),
                            url: None,
                        }),
                        ts,
                    ));
                }
            }
        }

        let written = self.repo.append_all(&records)?;
        self.index.ingest_paths(&self.repo, &written)?;

        let finished_at = Utc::now().timestamp() as u64;
        let run = MirrorRecord::SyncRun(SyncRun {
            header: self.header(),
            started_at,
            finished_at,
            mode: "tip".to_string(),
            parameters: serde_json::Map::from_iter([(
                "pages".to_string(),
                serde_json::Value::Number(pages.into()),
            )]),
            emitted_cursors: serde_json::Map::new(),
        });
        let run_paths = self.repo.append_all(&[run])?;
        self.index.ingest_paths(&self.repo, &run_paths)?;

        Ok(records.len())
    }

    /// Extend the mirror with older pages relative to the current known floor.
    ///
    /// This uses the local index to find the oldest `played_at` we've observed, then
    /// searches for that anchor in today's pagination and fetches older pages.
    ///
    /// `overlap_seconds` is applied to be robust to timestamp ties and page drift.
    pub async fn sync_older_pages(
        &self,
        client: &(dyn LastFmEditClient + Send + Sync),
        pages: u32,
        overlap_seconds: u64,
    ) -> Result<usize, MirrorError> {
        let Some(floor) = self.oldest_played_at()? else {
            // If we have nothing yet, tip sync is the only sensible starting point.
            return self.sync_tip_pages(client, 1).await;
        };

        let anchor_ts = floor.saturating_add(overlap_seconds);
        let anchor_dt = DateTime::from_timestamp(anchor_ts as i64, 0)
            .ok_or(MirrorError::InvalidTimestamp(anchor_ts))?;

        let started_at = Utc::now().timestamp() as u64;
        let observed_at = started_at;

        let anchor = crate::page_search::find_anchor_page_with_tracks_with_progress(
            client,
            anchor_dt,
            |_| {},
        )
        .await?
        .ok_or_else(|| MirrorError::InvalidTimestamp(anchor_ts))?;

        let mut records = Vec::new();

        // First page comes from the anchor search result.
        let page_tracks = anchor.tracks;
        for (pos, t) in page_tracks.iter().enumerate() {
            if let Some(ts) = t.timestamp {
                // Keep an overlap window for correctness.
                if ts <= anchor_ts {
                    records.push(self.make_observation_record(
                        observed_at,
                        RecordSource::LastfmScrape,
                        t,
                        Some(FetchContext {
                            range_from: None,
                            range_to: None,
                            page: Some(anchor.page),
                            position: Some(pos as u32),
                            url: None,
                        }),
                        ts,
                    ));
                }
            }
        }

        // Then fetch additional older pages.
        let mut next_page = anchor.page + 1;
        let remaining = pages.saturating_sub(1);
        for _ in 0..remaining {
            let tracks = fetch_recent_page(client, next_page).await?;
            if tracks.is_empty() {
                break;
            }
            for (pos, t) in tracks.iter().enumerate() {
                if let Some(ts) = t.timestamp {
                    if ts <= anchor_ts {
                        records.push(self.make_observation_record(
                            observed_at,
                            RecordSource::LastfmScrape,
                            t,
                            Some(FetchContext {
                                range_from: None,
                                range_to: None,
                                page: Some(next_page),
                                position: Some(pos as u32),
                                url: None,
                            }),
                            ts,
                        ));
                    }
                }
            }
            next_page += 1;
        }

        let written = self.repo.append_all(&records)?;
        self.index.ingest_paths(&self.repo, &written)?;

        let finished_at = Utc::now().timestamp() as u64;
        let run = MirrorRecord::SyncRun(SyncRun {
            header: self.header(),
            started_at,
            finished_at,
            mode: "older".to_string(),
            parameters: serde_json::Map::from_iter([
                ("pages".to_string(), serde_json::Value::Number(pages.into())),
                (
                    "overlap_seconds".to_string(),
                    serde_json::Value::Number(overlap_seconds.into()),
                ),
                (
                    "anchor_ts".to_string(),
                    serde_json::Value::Number(anchor_ts.into()),
                ),
                (
                    "anchor_page".to_string(),
                    serde_json::Value::Number((anchor.page as u64).into()),
                ),
            ]),
            emitted_cursors: serde_json::Map::new(),
        });
        let run_paths = self.repo.append_all(&[run])?;
        self.index.ingest_paths(&self.repo, &run_paths)?;

        Ok(records.len())
    }

    fn header(&self) -> RecordHeader {
        RecordHeader {
            version: MirrorRecord::VERSION,
            user: self.user.clone(),
            machine_id: self.machine_id.clone(),
        }
    }

    fn append_records(&self, records: &[MirrorRecord]) -> Result<(), MirrorError> {
        if records.is_empty() {
            return Ok(());
        }
        let written = self.repo.append_all(records)?;
        self.index.ingest_paths(&self.repo, &written)?;
        Ok(())
    }

    fn make_observation_record(
        &self,
        observed_at: u64,
        source: RecordSource,
        t: &Track,
        context: Option<FetchContext>,
        played_at: u64,
    ) -> MirrorRecord {
        MirrorRecord::ScrobbleObserved(ScrobbleObserved {
            header: self.header(),
            observed_at,
            source,
            scrobble: Scrobble {
                played_at,
                artist: t.artist.clone(),
                track: t.name.clone(),
                album: t.album.clone(),
                album_artist: t.album_artist.clone(),
            },
            context,
        })
    }
}

fn edit_target_from_exact_scrobble_edit(edit: &ExactScrobbleEdit) -> EditTarget {
    EditTarget {
        played_at: Some(edit.timestamp),
        artist: Some(edit.artist_name.clone()),
        track: Some(edit.track_name.clone()),
        album: Some(edit.album_name.clone()),
        album_artist: Some(edit.album_artist_name.clone()),
        artist_original: Some(edit.artist_name_original.clone()),
        track_original: Some(edit.track_name_original.clone()),
        album_original: Some(edit.album_name_original.clone()),
        album_artist_original: Some(edit.album_artist_name_original.clone()),
    }
}

async fn fetch_recent_page(
    client: &(dyn LastFmEditClient + Send + Sync),
    page: u32,
) -> LastFmResult<Vec<Track>> {
    let mut iter = client.recent_tracks_from_page(page);
    let mut tracks = Vec::new();
    for _ in 0..50 {
        match iter.next().await? {
            Some(t) => tracks.push(t),
            None => break,
        }
    }
    Ok(tracks)
}
