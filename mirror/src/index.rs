use crate::records::{MirrorRecord, RecordSource};
use crate::repo::{MirrorRepo, MirrorRepoError};
use rusqlite::{params, Connection, OptionalExtension};
use std::io::{BufRead, Seek};
use std::path::{Path, PathBuf};
use std::{fs, io};
use walkdir::WalkDir;

#[derive(Debug, thiserror::Error)]
pub enum MirrorIndexError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("repo error: {0}")]
    Repo(#[from] MirrorRepoError),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid utf-8 in log line")]
    InvalidUtf8,
}

#[derive(Debug, Clone)]
pub struct MirrorIndex {
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrobbleRow {
    pub played_at: u64,
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditAttemptRow {
    pub attempted_at: u64,
    pub success: bool,
    pub error: Option<String>,
    pub played_at: Option<u64>,
    pub artist: Option<String>,
    pub track: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRow {
    pub kind: String, // "edit" | "delete"
    pub attempted_at: u64,
    pub success: bool,
    pub error: Option<String>,
    pub played_at: Option<u64>,
    pub artist: Option<String>,
    pub track: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
}

impl MirrorIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MirrorIndexError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let idx = Self { path };
        idx.with_conn(|conn| idx.init_schema(conn))?;
        Ok(idx)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn rebuild_from_repo(&self, repo: &MirrorRepo) -> Result<(), MirrorIndexError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                DROP TABLE IF EXISTS file_offsets;
                DROP TABLE IF EXISTS scrobble_observed;
                DROP TABLE IF EXISTS edit_attempted;
                DROP TABLE IF EXISTS delete_attempted;
                DROP TABLE IF EXISTS sync_run;
                "#,
            )?;
            self.init_schema(conn)?;
            Ok(())
        })?;

        self.ingest_repo(repo)
    }

    pub fn ingest_repo(&self, repo: &MirrorRepo) -> Result<(), MirrorIndexError> {
        let root = repo.root();
        let log_root = root.join("log");
        if !log_root.exists() {
            return Ok(());
        }

        // Deterministic ordering makes debugging easier.
        let mut files: Vec<PathBuf> = WalkDir::new(&log_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
            .collect();
        files.sort();

        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            for abs in files {
                let rel = abs
                    .strip_prefix(root)
                    .unwrap_or(&abs)
                    .to_path_buf()
                    .to_string_lossy()
                    .to_string();
                Self::ingest_file_tx(&tx, &abs, &rel)?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn ingest_paths(
        &self,
        repo: &MirrorRepo,
        rel_paths: &[PathBuf],
    ) -> Result<(), MirrorIndexError> {
        let root = repo.root();
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            for rel in rel_paths {
                let abs = root.join(rel);
                let rel_s = rel.to_string_lossy().to_string();
                Self::ingest_file_tx(&tx, &abs, &rel_s)?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn recent_scrobbles(
        &self,
        user: &str,
        limit: usize,
        before_played_at: Option<u64>,
    ) -> Result<Vec<ScrobbleRow>, MirrorIndexError> {
        self.with_conn(|conn| {
            let rows = if let Some(ts) = before_played_at {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT played_at, artist, track, album, album_artist
                    FROM (
                        SELECT
                            played_at, artist, track, album, album_artist,
                            ROW_NUMBER() OVER (
                                PARTITION BY played_at, artist, track, IFNULL(album, ''), IFNULL(album_artist, '')
                                ORDER BY observed_at DESC
                            ) AS rn
                        FROM scrobble_observed
                        WHERE user = ?1 AND played_at < ?2
                    )
                    WHERE rn = 1
                    ORDER BY played_at DESC
                    LIMIT ?3
                    "#,
                )?;
                let mut rows = Vec::new();
                let iter = stmt.query_map(params![user, ts, limit as i64], |row| {
                    Ok(ScrobbleRow {
                        played_at: row.get::<_, i64>(0)? as u64,
                        artist: row.get(1)?,
                        track: row.get(2)?,
                        album: row.get(3)?,
                        album_artist: row.get(4)?,
                    })
                })?;
                for row in iter {
                    rows.push(row?);
                }
                rows
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT played_at, artist, track, album, album_artist
                    FROM (
                        SELECT
                            played_at, artist, track, album, album_artist,
                            ROW_NUMBER() OVER (
                                PARTITION BY played_at, artist, track, IFNULL(album, ''), IFNULL(album_artist, '')
                                ORDER BY observed_at DESC
                            ) AS rn
                        FROM scrobble_observed
                        WHERE user = ?1
                    )
                    WHERE rn = 1
                    ORDER BY played_at DESC
                    LIMIT ?2
                    "#,
                )?;
                let mut rows = Vec::new();
                let iter = stmt.query_map(params![user, limit as i64], |row| {
                    Ok(ScrobbleRow {
                        played_at: row.get::<_, i64>(0)? as u64,
                        artist: row.get(1)?,
                        track: row.get(2)?,
                        album: row.get(3)?,
                        album_artist: row.get(4)?,
                    })
                })?;
                for row in iter {
                    rows.push(row?);
                }
                rows
            };

            Ok(rows)
        })
    }

    pub fn recent_edits(
        &self,
        user: &str,
        limit: usize,
        before_attempted_at: Option<u64>,
    ) -> Result<Vec<EditAttemptRow>, MirrorIndexError> {
        self.with_conn(|conn| {
            let (sql, params_vec): (&str, Vec<rusqlite::types::Value>) = if let Some(ts) =
                before_attempted_at
            {
                (
                    r#"
                    SELECT attempted_at, success, error, played_at, artist, track, album, album_artist
                    FROM edit_attempted
                    WHERE user = ?1 AND attempted_at < ?2
                    ORDER BY attempted_at DESC
                    LIMIT ?3
                    "#,
                    vec![
                        rusqlite::types::Value::from(user.to_string()),
                        rusqlite::types::Value::from(ts as i64),
                        rusqlite::types::Value::from(limit as i64),
                    ],
                )
            } else {
                (
                    r#"
                    SELECT attempted_at, success, error, played_at, artist, track, album, album_artist
                    FROM edit_attempted
                    WHERE user = ?1
                    ORDER BY attempted_at DESC
                    LIMIT ?2
                    "#,
                    vec![
                        rusqlite::types::Value::from(user.to_string()),
                        rusqlite::types::Value::from(limit as i64),
                    ],
                )
            };

            let mut stmt = conn.prepare(sql)?;
            let mut rows = Vec::new();
            let mut iter = stmt.query(rusqlite::params_from_iter(params_vec))?;
            while let Some(row) = iter.next()? {
                rows.push(EditAttemptRow {
                    attempted_at: row.get::<_, i64>(0)? as u64,
                    success: row.get::<_, i64>(1)? != 0,
                    error: row.get(2)?,
                    played_at: row
                        .get::<_, Option<i64>>(3)?
                        .map(|v| v as u64),
                    artist: row.get(4)?,
                    track: row.get(5)?,
                    album: row.get(6)?,
                    album_artist: row.get(7)?,
                });
            }
            Ok(rows)
        })
    }

    pub fn recent_actions(
        &self,
        user: &str,
        limit: usize,
        before_attempted_at: Option<u64>,
    ) -> Result<Vec<ActionRow>, MirrorIndexError> {
        self.with_conn(|conn| {
            let (sql, params_vec): (&str, Vec<rusqlite::types::Value>) = if let Some(ts) =
                before_attempted_at
            {
                (
                    r#"
                    SELECT kind, attempted_at, success, error, played_at, artist, track, album, album_artist
                    FROM (
                        SELECT 'edit' AS kind, attempted_at, success, error, played_at, artist, track, album, album_artist
                        FROM edit_attempted
                        WHERE user = ?1 AND attempted_at < ?2
                        UNION ALL
                        SELECT 'delete' AS kind, attempted_at, success, error, played_at, artist, track, album, album_artist
                        FROM delete_attempted
                        WHERE user = ?1 AND attempted_at < ?2
                    )
                    ORDER BY attempted_at DESC
                    LIMIT ?3
                    "#,
                    vec![
                        rusqlite::types::Value::from(user.to_string()),
                        rusqlite::types::Value::from(ts as i64),
                        rusqlite::types::Value::from(limit as i64),
                    ],
                )
            } else {
                (
                    r#"
                    SELECT kind, attempted_at, success, error, played_at, artist, track, album, album_artist
                    FROM (
                        SELECT 'edit' AS kind, attempted_at, success, error, played_at, artist, track, album, album_artist
                        FROM edit_attempted
                        WHERE user = ?1
                        UNION ALL
                        SELECT 'delete' AS kind, attempted_at, success, error, played_at, artist, track, album, album_artist
                        FROM delete_attempted
                        WHERE user = ?1
                    )
                    ORDER BY attempted_at DESC
                    LIMIT ?2
                    "#,
                    vec![
                        rusqlite::types::Value::from(user.to_string()),
                        rusqlite::types::Value::from(limit as i64),
                    ],
                )
            };

            let mut stmt = conn.prepare(sql)?;
            let mut rows = Vec::new();
            let mut iter = stmt.query(rusqlite::params_from_iter(params_vec))?;
            while let Some(row) = iter.next()? {
                rows.push(ActionRow {
                    kind: row.get(0)?,
                    attempted_at: row.get::<_, i64>(1)? as u64,
                    success: row.get::<_, i64>(2)? != 0,
                    error: row.get(3)?,
                    played_at: row
                        .get::<_, Option<i64>>(4)?
                        .map(|v| v as u64),
                    artist: row.get(5)?,
                    track: row.get(6)?,
                    album: row.get(7)?,
                    album_artist: row.get(8)?,
                });
            }
            Ok(rows)
        })
    }

    pub fn newest_played_at(&self, user: &str) -> Result<Option<u64>, MirrorIndexError> {
        self.with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT MAX(played_at) FROM scrobble_observed WHERE user = ?1",
                    params![user],
                    |row| row.get::<_, Option<i64>>(0),
                )?
                .map(|v| v as u64))
        })
    }

    pub fn oldest_played_at(&self, user: &str) -> Result<Option<u64>, MirrorIndexError> {
        self.with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT MIN(played_at) FROM scrobble_observed WHERE user = ?1",
                    params![user],
                    |row| row.get::<_, Option<i64>>(0),
                )?
                .map(|v| v as u64))
        })
    }

    fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, MirrorIndexError>,
    ) -> Result<T, MirrorIndexError> {
        let mut conn = Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        f(&mut conn)
    }

    fn init_schema(&self, conn: &mut Connection) -> Result<(), MirrorIndexError> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS file_offsets (
                path TEXT PRIMARY KEY,
                bytes INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS scrobble_observed (
                id INTEGER PRIMARY KEY,
                origin_file TEXT NOT NULL,
                origin_offset INTEGER NOT NULL,

                user TEXT NOT NULL,
                machine_id TEXT NOT NULL,
                observed_at INTEGER NOT NULL,
                source TEXT NOT NULL,

                played_at INTEGER NOT NULL,
                artist TEXT NOT NULL,
                track TEXT NOT NULL,
                album TEXT,
                album_artist TEXT,

                range_from INTEGER,
                range_to INTEGER,
                context_page INTEGER,
                context_position INTEGER,
                context_url TEXT,

                UNIQUE(origin_file, origin_offset)
            );
            CREATE INDEX IF NOT EXISTS idx_scrobble_user_played_at ON scrobble_observed(user, played_at DESC);
            CREATE INDEX IF NOT EXISTS idx_scrobble_user_artist ON scrobble_observed(user, artist);

            CREATE TABLE IF NOT EXISTS edit_attempted (
                id INTEGER PRIMARY KEY,
                origin_file TEXT NOT NULL,
                origin_offset INTEGER NOT NULL,

                user TEXT NOT NULL,
                machine_id TEXT NOT NULL,
                attempted_at INTEGER NOT NULL,
                success INTEGER NOT NULL,
                error TEXT,

                played_at INTEGER,
                artist TEXT,
                track TEXT,
                album TEXT,
                album_artist TEXT,

                artist_original TEXT,
                track_original TEXT,
                album_original TEXT,
                album_artist_original TEXT,

                UNIQUE(origin_file, origin_offset)
            );
            CREATE INDEX IF NOT EXISTS idx_edit_user_attempted_at ON edit_attempted(user, attempted_at DESC);
            CREATE INDEX IF NOT EXISTS idx_edit_user_success ON edit_attempted(user, success);

            CREATE TABLE IF NOT EXISTS delete_attempted (
                id INTEGER PRIMARY KEY,
                origin_file TEXT NOT NULL,
                origin_offset INTEGER NOT NULL,

                user TEXT NOT NULL,
                machine_id TEXT NOT NULL,
                attempted_at INTEGER NOT NULL,
                success INTEGER NOT NULL,
                error TEXT,

                played_at INTEGER,
                artist TEXT,
                track TEXT,
                album TEXT,
                album_artist TEXT,

                artist_original TEXT,
                track_original TEXT,
                album_original TEXT,
                album_artist_original TEXT,

                UNIQUE(origin_file, origin_offset)
            );
            CREATE INDEX IF NOT EXISTS idx_delete_user_attempted_at ON delete_attempted(user, attempted_at DESC);
            CREATE INDEX IF NOT EXISTS idx_delete_user_success ON delete_attempted(user, success);

            CREATE TABLE IF NOT EXISTS sync_run (
                id INTEGER PRIMARY KEY,
                origin_file TEXT NOT NULL,
                origin_offset INTEGER NOT NULL,

                user TEXT NOT NULL,
                machine_id TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER NOT NULL,
                mode TEXT NOT NULL,
                parameters_json TEXT NOT NULL,
                emitted_cursors_json TEXT NOT NULL,

                UNIQUE(origin_file, origin_offset)
            );
            CREATE INDEX IF NOT EXISTS idx_run_user_started_at ON sync_run(user, started_at DESC);
            "#,
        )?;
        Ok(())
    }

    fn ingest_file_tx(
        tx: &rusqlite::Transaction<'_>,
        abs_path: &Path,
        rel_path: &str,
    ) -> Result<(), MirrorIndexError> {
        if !abs_path.exists() {
            return Ok(());
        }

        let size = abs_path.metadata()?.len();
        let mut offset: u64 = tx
            .query_row(
                "SELECT bytes FROM file_offsets WHERE path = ?1",
                params![rel_path],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0) as u64;

        if offset > size {
            // Git rewrites/truncation: purge and start over for this file.
            tx.execute(
                "DELETE FROM scrobble_observed WHERE origin_file = ?1",
                params![rel_path],
            )?;
            tx.execute(
                "DELETE FROM edit_attempted WHERE origin_file = ?1",
                params![rel_path],
            )?;
            tx.execute(
                "DELETE FROM delete_attempted WHERE origin_file = ?1",
                params![rel_path],
            )?;
            tx.execute(
                "DELETE FROM sync_run WHERE origin_file = ?1",
                params![rel_path],
            )?;
            offset = 0;
        }

        let file = fs::File::open(abs_path)?;
        let mut reader = io::BufReader::new(file);
        reader.seek(io::SeekFrom::Start(offset))?;

        let mut buf = Vec::new();
        loop {
            buf.clear();
            let line_offset = offset;
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
                break;
            }
            offset += n as u64;

            // Trim trailing newline.
            if buf.ends_with(&[b'\n']) {
                buf.pop();
                if buf.ends_with(&[b'\r']) {
                    buf.pop();
                }
            }
            if buf.is_empty() {
                continue;
            }

            let line = std::str::from_utf8(&buf).map_err(|_| MirrorIndexError::InvalidUtf8)?;
            let rec: MirrorRecord = serde_json::from_str(line)?;
            Self::insert_record_tx(tx, rel_path, line_offset, &rec)?;
        }

        tx.execute(
            "INSERT INTO file_offsets(path, bytes) VALUES (?1, ?2)\n             ON CONFLICT(path) DO UPDATE SET bytes = excluded.bytes",
            params![rel_path, offset as i64],
        )?;

        Ok(())
    }

    fn insert_record_tx(
        tx: &rusqlite::Transaction<'_>,
        origin_file: &str,
        origin_offset: u64,
        rec: &MirrorRecord,
    ) -> Result<(), MirrorIndexError> {
        match rec {
            MirrorRecord::ScrobbleObserved(r) => {
                let ctx = r.context.as_ref();
                let source = match r.source {
                    RecordSource::LastfmApi => "lastfm_api",
                    RecordSource::LastfmScrape => "lastfm_scrape",
                };
                tx.execute(
                    r#"
                    INSERT OR IGNORE INTO scrobble_observed(
                        origin_file, origin_offset,
                        user, machine_id, observed_at, source,
                        played_at, artist, track, album, album_artist,
                        range_from, range_to, context_page, context_position, context_url
                    ) VALUES (
                        ?1, ?2,
                        ?3, ?4, ?5, ?6,
                        ?7, ?8, ?9, ?10, ?11,
                        ?12, ?13, ?14, ?15, ?16
                    )
                    "#,
                    params![
                        origin_file,
                        origin_offset as i64,
                        r.header.user,
                        r.header.machine_id,
                        r.observed_at as i64,
                        source,
                        r.scrobble.played_at as i64,
                        r.scrobble.artist,
                        r.scrobble.track,
                        r.scrobble.album,
                        r.scrobble.album_artist,
                        ctx.and_then(|c| c.range_from).map(|v| v as i64),
                        ctx.and_then(|c| c.range_to).map(|v| v as i64),
                        ctx.and_then(|c| c.page).map(|v| v as i64),
                        ctx.and_then(|c| c.position).map(|v| v as i64),
                        ctx.and_then(|c| c.url.clone()),
                    ],
                )?;
            }
            MirrorRecord::EditAttempted(r) => {
                tx.execute(
                    r#"
                    INSERT OR IGNORE INTO edit_attempted(
                        origin_file, origin_offset,
                        user, machine_id, attempted_at, success, error,
                        played_at, artist, track, album, album_artist,
                        artist_original, track_original, album_original, album_artist_original
                    ) VALUES (
                        ?1, ?2,
                        ?3, ?4, ?5, ?6, ?7,
                        ?8, ?9, ?10, ?11, ?12,
                        ?13, ?14, ?15, ?16
                    )
                    "#,
                    params![
                        origin_file,
                        origin_offset as i64,
                        r.header.user,
                        r.header.machine_id,
                        r.attempted_at as i64,
                        if r.result.success { 1 } else { 0 },
                        r.result.error,
                        r.target.played_at.map(|v| v as i64),
                        r.target.artist,
                        r.target.track,
                        r.target.album,
                        r.target.album_artist,
                        r.target.artist_original,
                        r.target.track_original,
                        r.target.album_original,
                        r.target.album_artist_original,
                    ],
                )?;
            }
            MirrorRecord::DeleteAttempted(r) => {
                tx.execute(
                    r#"
                    INSERT OR IGNORE INTO delete_attempted(
                        origin_file, origin_offset,
                        user, machine_id, attempted_at, success, error,
                        played_at, artist, track, album, album_artist,
                        artist_original, track_original, album_original, album_artist_original
                    ) VALUES (
                        ?1, ?2,
                        ?3, ?4, ?5, ?6, ?7,
                        ?8, ?9, ?10, ?11, ?12,
                        ?13, ?14, ?15, ?16
                    )
                    "#,
                    params![
                        origin_file,
                        origin_offset as i64,
                        r.header.user,
                        r.header.machine_id,
                        r.attempted_at as i64,
                        if r.result.success { 1 } else { 0 },
                        r.result.error,
                        r.target.played_at.map(|v| v as i64),
                        r.target.artist,
                        r.target.track,
                        r.target.album,
                        r.target.album_artist,
                        r.target.artist_original,
                        r.target.track_original,
                        r.target.album_original,
                        r.target.album_artist_original,
                    ],
                )?;
            }
            MirrorRecord::SyncRun(r) => {
                tx.execute(
                    r#"
                    INSERT OR IGNORE INTO sync_run(
                        origin_file, origin_offset,
                        user, machine_id, started_at, finished_at, mode,
                        parameters_json, emitted_cursors_json
                    ) VALUES (
                        ?1, ?2,
                        ?3, ?4, ?5, ?6, ?7,
                        ?8, ?9
                    )
                    "#,
                    params![
                        origin_file,
                        origin_offset as i64,
                        r.header.user,
                        r.header.machine_id,
                        r.started_at as i64,
                        r.finished_at as i64,
                        r.mode,
                        serde_json::to_string(&r.parameters)?,
                        serde_json::to_string(&r.emitted_cursors)?,
                    ],
                )?;
            }
            MirrorRecord::Custom(_r) => {
                // Not indexed yet.
            }
        }

        Ok(())
    }
}
