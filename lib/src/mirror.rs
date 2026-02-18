use chrono::{DateTime, Utc};
use lastfm_edit::{
    ClientEvent, ExactScrobbleEdit, LastFmEditClient, LastFmError, Result as LastFmResult, Track,
};
use std::path::{Path, PathBuf};

use scrobble_scrubber_mirror::index::{
    ActionRow, AnchorContextPageBounds, EditAttemptRow, MirrorIndex, MirrorIndexError, ScrobbleRow,
    WorkItemRow, WorklistOrder,
};
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
    lastfm_api_key: Option<String>,
}

impl LibraryMirror {
    pub fn open(
        user: impl Into<String>,
        state_file: impl AsRef<Path>,
    ) -> Result<Self, MirrorError> {
        Self::open_with_api_key(user, state_file, None::<String>)
    }

    pub fn open_with_api_key(
        user: impl Into<String>,
        state_file: impl AsRef<Path>,
        lastfm_api_key: Option<impl Into<String>>,
    ) -> Result<Self, MirrorError> {
        let user = user.into();
        let paths = MirrorPaths::from_state_file(state_file, &user);
        let repo = MirrorRepo::new(&paths.repo_root);
        let index = MirrorIndex::open(&paths.index_path)?;
        let lastfm_api_key = lastfm_api_key
            .map(Into::into)
            .filter(|k| !k.trim().is_empty());
        Ok(Self {
            repo,
            index,
            user,
            machine_id: default_machine_id(),
            lastfm_api_key,
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
        self.append_scrobble_observations_from_page_with_source(
            page,
            tracks,
            RecordSource::LastfmScrape,
            None,
            None,
        )
    }

    pub fn append_scrobble_observations_from_page_with_source(
        &self,
        page: u32,
        tracks: &[Track],
        source: RecordSource,
        range_from: Option<u64>,
        range_to: Option<u64>,
    ) -> Result<usize, MirrorError> {
        let observed_at = Utc::now().timestamp() as u64;
        let mut records = Vec::new();

        for (pos, t) in tracks.iter().enumerate() {
            let Some(ts) = t.timestamp else {
                continue;
            };
            records.push(self.make_observation_record(
                observed_at,
                source,
                t,
                Some(FetchContext {
                    range_from,
                    range_to,
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

    pub fn recent_edits(
        &self,
        limit: usize,
        before_attempted_at: Option<u64>,
    ) -> Result<Vec<EditAttemptRow>, MirrorError> {
        Ok(self
            .index
            .recent_edits(&self.user, limit, before_attempted_at)?)
    }

    pub fn recent_actions(
        &self,
        limit: usize,
        before_attempted_at: Option<u64>,
    ) -> Result<Vec<ActionRow>, MirrorError> {
        Ok(self
            .index
            .recent_actions(&self.user, limit, before_attempted_at)?)
    }

    pub fn oldest_played_at(&self) -> Result<Option<u64>, MirrorError> {
        Ok(self.index.oldest_played_at(&self.user)?)
    }

    pub fn newest_played_at(&self) -> Result<Option<u64>, MirrorError> {
        Ok(self.index.newest_played_at(&self.user)?)
    }

    pub fn anchor_context_page_bounds(
        &self,
        anchor_played_at: u64,
    ) -> Result<AnchorContextPageBounds, MirrorError> {
        Ok(self
            .index
            .anchor_context_page_bounds(&self.user, anchor_played_at)?)
    }

    pub fn unprocessed_scrobbles(
        &self,
        limit: usize,
        order: WorklistOrder,
    ) -> Result<Vec<WorkItemRow>, MirrorError> {
        Ok(self.index.unprocessed_scrobbles(&self.user, limit, order)?)
    }

    pub fn mark_scrobble_processed(
        &self,
        key: &str,
        processed_at: u64,
        last_result: Option<&str>,
    ) -> Result<(), MirrorError> {
        Ok(self
            .index
            .mark_scrobble_processed(key, processed_at, last_result)?)
    }

    pub fn api_sync_enabled(&self) -> bool {
        self.lastfm_api_key.is_some()
    }

    pub fn sync_record_source(&self) -> RecordSource {
        if self.api_sync_enabled() {
            RecordSource::LastfmApi
        } else {
            RecordSource::LastfmScrape
        }
    }

    pub async fn fetch_recent_page_resilient_cancelable(
        &self,
        client: &(dyn LastFmEditClient + Send + Sync),
        page: u32,
        mut should_cancel: impl FnMut() -> bool,
    ) -> Result<Vec<Track>, MirrorError> {
        loop {
            if should_cancel() {
                return Err(MirrorError::Lastfm(LastFmError::Io(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled",
                ))));
            }

            match self.fetch_recent_page_for_sync(client, page).await {
                Ok(tracks) => return Ok(tracks),
                Err(MirrorError::Lastfm(LastFmError::RateLimit { retry_after })) => {
                    for _ in 0..retry_after {
                        if should_cancel() {
                            return Err(MirrorError::Lastfm(LastFmError::Io(std::io::Error::new(
                                std::io::ErrorKind::Interrupted,
                                "cancelled",
                            ))));
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn sync_tip_pages(
        &self,
        client: &(dyn LastFmEditClient + Send + Sync),
        pages: u32,
    ) -> Result<usize, MirrorError> {
        let started_at = Utc::now().timestamp() as u64;
        let mut records = Vec::new();
        let observed_at = started_at;
        let source = self.sync_record_source();

        for page in 1..=pages.max(1) {
            let tracks = self.fetch_recent_page_for_sync(client, page).await?;
            for (pos, t) in tracks.iter().enumerate() {
                if let Some(ts) = t.timestamp {
                    records.push(self.make_observation_record(
                        observed_at,
                        source,
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
        if self.api_sync_enabled() {
            return self
                .sync_older_pages_via_api(pages, overlap_seconds, anchor_ts)
                .await;
        }

        let anchor_dt = DateTime::from_timestamp(anchor_ts as i64, 0)
            .ok_or(MirrorError::InvalidTimestamp(anchor_ts))?;

        let started_at = Utc::now().timestamp() as u64;
        let observed_at = started_at;
        let source = self.sync_record_source();

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
                        source,
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
            let tracks = self.fetch_recent_page_for_sync(client, next_page).await?;
            if tracks.is_empty() {
                break;
            }
            for (pos, t) in tracks.iter().enumerate() {
                if let Some(ts) = t.timestamp {
                    if ts <= anchor_ts {
                        records.push(self.make_observation_record(
                            observed_at,
                            source,
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

    async fn fetch_recent_page_for_sync(
        &self,
        client: &(dyn LastFmEditClient + Send + Sync),
        page: u32,
    ) -> Result<Vec<Track>, MirrorError> {
        if let Some(api_key) = self.lastfm_api_key.as_deref() {
            return Ok(fetch_recent_page_via_api(&self.user, api_key, page, None, None).await?);
        }

        Ok(fetch_recent_page(client, page).await?)
    }

    async fn sync_older_pages_via_api(
        &self,
        pages: u32,
        overlap_seconds: u64,
        anchor_ts: u64,
    ) -> Result<usize, MirrorError> {
        let Some(api_key) = self.lastfm_api_key.as_deref() else {
            return Ok(0);
        };

        let started_at = Utc::now().timestamp() as u64;
        let observed_at = started_at;
        let mut records = Vec::new();

        for page in 1..=pages.max(1) {
            let tracks =
                fetch_recent_page_via_api(&self.user, api_key, page, None, Some(anchor_ts)).await?;
            if tracks.is_empty() {
                break;
            }

            for (pos, t) in tracks.iter().enumerate() {
                if let Some(ts) = t.timestamp {
                    if ts <= anchor_ts {
                        records.push(self.make_observation_record(
                            observed_at,
                            RecordSource::LastfmApi,
                            t,
                            Some(FetchContext {
                                range_from: None,
                                range_to: Some(anchor_ts),
                                page: Some(page),
                                position: Some(pos as u32),
                                url: None,
                            }),
                            ts,
                        ));
                    }
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
                    "strategy".to_string(),
                    serde_json::Value::String("api_to".to_string()),
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
    Ok(client.get_recent_tracks_page(page).await?.tracks)
}

async fn fetch_recent_page_via_api(
    user: &str,
    api_key: &str,
    page: u32,
    from: Option<u64>,
    to: Option<u64>,
) -> LastFmResult<Vec<Track>> {
    let client = reqwest::Client::new();
    let mut params = vec![
        ("method".to_string(), "user.getrecenttracks".to_string()),
        ("user".to_string(), user.to_string()),
        ("api_key".to_string(), api_key.to_string()),
        ("format".to_string(), "json".to_string()),
        ("extended".to_string(), "1".to_string()),
        ("limit".to_string(), "200".to_string()),
        ("page".to_string(), page.to_string()),
    ];
    if let Some(from_ts) = from {
        params.push(("from".to_string(), from_ts.to_string()));
    }
    if let Some(to_ts) = to {
        params.push(("to".to_string(), to_ts.to_string()));
    }

    let response = client
        .get("https://ws.audioscrobbler.com/2.0/")
        .query(&params)
        .send()
        .await
        .map_err(|e| LastFmError::Http(e.to_string()))?;

    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(LastFmError::RateLimit { retry_after });
    }

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| LastFmError::Http(e.to_string()))?;

    if !status.is_success() {
        return Err(LastFmError::Http(format!(
            "Last.fm API request failed with status {status}: {body}"
        )));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| LastFmError::Parse(e.to_string()))?;

    if let Some(error_code) = json.get("error").and_then(|v| v.as_i64()) {
        if error_code == 29 {
            return Err(LastFmError::RateLimit { retry_after: 60 });
        }
        let message = json
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Last.fm API error");
        return Err(LastFmError::Http(format!(
            "Last.fm API error {error_code}: {message}"
        )));
    }

    parse_tracks_from_recenttracks_json(&json)
}

fn parse_tracks_from_recenttracks_json(json: &serde_json::Value) -> LastFmResult<Vec<Track>> {
    let mut tracks = Vec::new();
    let track_entries = json
        .get("recenttracks")
        .and_then(|v| v.get("track"))
        .ok_or_else(|| {
            LastFmError::Parse("Missing recenttracks.track in API response".to_string())
        })?;

    let entries: Vec<&serde_json::Value> = match track_entries {
        serde_json::Value::Array(arr) => arr.iter().collect(),
        other => vec![other],
    };

    for entry in entries {
        let timestamp = entry
            .get("date")
            .and_then(|d| d.get("uts"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        // Skip currently playing entries that don't have a committed timestamp yet.
        let Some(played_at) = timestamp else {
            continue;
        };

        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let artist = entry
            .get("artist")
            .and_then(|v| {
                v.get("name")
                    .and_then(|s| s.as_str())
                    .or_else(|| v.get("#text").and_then(|s| s.as_str()))
                    .or_else(|| v.as_str())
            })
            .unwrap_or("")
            .trim()
            .to_string();
        let album = entry
            .get("album")
            .and_then(|v| {
                v.get("#text")
                    .and_then(|s| s.as_str())
                    .or_else(|| v.get("name").and_then(|s| s.as_str()))
                    .or_else(|| v.as_str())
            })
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);

        if name.is_empty() || artist.is_empty() {
            continue;
        }

        tracks.push(Track {
            name,
            artist,
            playcount: 1,
            timestamp: Some(played_at),
            album,
            album_artist: None,
        });
    }

    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::parse_tracks_from_recenttracks_json;

    #[test]
    fn parses_extended_recenttracks_and_skips_now_playing() {
        let payload = serde_json::json!({
            "recenttracks": {
                "track": [
                    {
                        "name": "Now Playing",
                        "artist": {"name": "Artist A"},
                        "album": {"#text": "Album A"},
                        "@attr": {"nowplaying": "true"}
                    },
                    {
                        "name": "Committed Track",
                        "artist": {"name": "Artist B"},
                        "album": {"#text": "Album B"},
                        "date": {"uts": "1700000000"}
                    }
                ]
            }
        });

        let tracks = parse_tracks_from_recenttracks_json(&payload).expect("parse should succeed");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].name, "Committed Track");
        assert_eq!(tracks[0].artist, "Artist B");
        assert_eq!(tracks[0].album.as_deref(), Some("Album B"));
        assert_eq!(tracks[0].timestamp, Some(1_700_000_000));
    }
}
