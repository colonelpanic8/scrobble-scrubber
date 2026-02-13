use crate::records::MirrorRecord;
use crate::sink::MirrorSink;
use chrono::{Datelike, TimeZone, Utc};
use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum MirrorRepoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid unix timestamp: {0}")]
    InvalidTimestamp(u64),
}

#[derive(Debug, Clone)]
pub struct MirrorRepo {
    root: PathBuf,
}

impl MirrorRepo {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn append(&self, record: &MirrorRecord) -> Result<PathBuf, MirrorRepoError> {
        let rel_path = self.record_path(record)?;
        let abs_path = self.root.join(&rel_path);

        if let Some(parent) = abs_path.parent() {
            create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&abs_path)?;
        let mut w = BufWriter::new(file);
        serde_json::to_writer(&mut w, record)?;
        w.write_all(b"\n")?;
        w.flush()?;

        Ok(rel_path)
    }

    /// Append many records efficiently.
    ///
    /// Records are grouped by their shard file so we only open each file once.
    /// Returns the set of relative paths written.
    pub fn append_all(&self, records: &[MirrorRecord]) -> Result<Vec<PathBuf>, MirrorRepoError> {
        if records.is_empty() {
            return Ok(Vec::new());
        }

        let mut grouped: std::collections::BTreeMap<PathBuf, Vec<&MirrorRecord>> =
            std::collections::BTreeMap::new();
        for r in records {
            let rel = self.record_path(r)?;
            grouped.entry(rel).or_default().push(r);
        }

        let mut written = Vec::new();
        for (rel_path, group) in grouped {
            let abs_path = self.root.join(&rel_path);
            if let Some(parent) = abs_path.parent() {
                create_dir_all(parent)?;
            }

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&abs_path)?;
            let mut w = BufWriter::new(file);
            for r in group {
                serde_json::to_writer(&mut w, r)?;
                w.write_all(b"\n")?;
            }
            w.flush()?;
            written.push(rel_path);
        }

        Ok(written)
    }

    fn record_path(&self, record: &MirrorRecord) -> Result<PathBuf, MirrorRepoError> {
        let ts = record.shard_unix_seconds();
        let ts_i64 = i64::try_from(ts).map_err(|_| MirrorRepoError::InvalidTimestamp(ts))?;
        let date = Utc
            .timestamp_opt(ts_i64, 0)
            .single()
            .ok_or(MirrorRepoError::InvalidTimestamp(ts))?;

        let (top_dir, kind_dir) = match record {
            MirrorRecord::ScrobbleObserved(_) => ("log", "observations"),
            MirrorRecord::EditAttempted(_) => ("log", "actions"),
            MirrorRecord::DeleteAttempted(_) => ("log", "actions"),
            MirrorRecord::SyncRun(_) => ("log", "runs"),
            MirrorRecord::Custom(_) => ("log", "custom"),
        };

        // Machine-namespaced to keep git merges clean for multi-writer use.
        let machine_id = record.machine_id();

        Ok(PathBuf::from(top_dir)
            .join(kind_dir)
            .join(machine_id)
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}.jsonl", date.day())))
    }
}

impl MirrorSink for MirrorRepo {
    fn append(&self, record: &MirrorRecord) -> Result<PathBuf, MirrorRepoError> {
        MirrorRepo::append(self, record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::{
        FetchContext, MirrorRecord, RecordHeader, RecordSource, Scrobble, ScrobbleObserved,
    };

    #[test]
    fn append_writes_jsonl_line_and_returns_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = MirrorRepo::new(tmp.path());

        let header = RecordHeader {
            version: MirrorRecord::VERSION,
            user: "alice".to_string(),
            machine_id: "machine-a".to_string(),
        };
        let rec = MirrorRecord::ScrobbleObserved(ScrobbleObserved {
            header,
            observed_at: 1_700_000_000,
            source: RecordSource::LastfmApi,
            scrobble: Scrobble {
                played_at: 1_700_000_000,
                artist: "Artist".to_string(),
                track: "Track".to_string(),
                album: Some("Album".to_string()),
                album_artist: None,
            },
            context: Some(FetchContext {
                range_from: Some(1_699_000_000),
                range_to: Some(1_700_000_000),
                page: Some(1),
                position: Some(0),
                url: None,
            }),
        });

        let rel = repo.append(&rec).unwrap();
        assert!(rel
            .to_string_lossy()
            .starts_with("log/observations/machine-a/"));

        let abs = repo.root().join(rel);
        let content = std::fs::read_to_string(abs).unwrap();
        assert!(content.lines().count() == 1);
        assert!(content.contains("\"kind\":\"scrobble_observed\""));
        assert!(content.ends_with('\n'));
    }
}
