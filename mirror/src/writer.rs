use crate::records::{
    ActionResult, DeleteAttempted, EditAttempted, EditTarget, FetchContext, MirrorRecord,
    RecordHeader, RecordSource, Scrobble, ScrobbleObserved, SyncRun,
};
use crate::repo::{MirrorRepo, MirrorRepoError};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MirrorWriter {
    repo: MirrorRepo,
    user: String,
    machine_id: String,
}

impl MirrorWriter {
    pub fn new(repo: MirrorRepo, user: impl Into<String>, machine_id: impl Into<String>) -> Self {
        Self {
            repo,
            user: user.into(),
            machine_id: machine_id.into(),
        }
    }

    pub fn repo(&self) -> &MirrorRepo {
        &self.repo
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    fn header(&self) -> RecordHeader {
        RecordHeader {
            version: MirrorRecord::VERSION,
            user: self.user.clone(),
            machine_id: self.machine_id.clone(),
        }
    }

    pub fn append_observation(
        &self,
        observed_at: u64,
        source: RecordSource,
        scrobble: Scrobble,
        context: Option<FetchContext>,
    ) -> Result<PathBuf, MirrorRepoError> {
        let record = MirrorRecord::ScrobbleObserved(ScrobbleObserved {
            header: self.header(),
            observed_at,
            source,
            scrobble,
            context,
        });
        self.repo.append(&record)
    }

    pub fn append_edit_attempted(
        &self,
        attempted_at: u64,
        target: EditTarget,
        result: ActionResult,
    ) -> Result<PathBuf, MirrorRepoError> {
        let record = MirrorRecord::EditAttempted(EditAttempted {
            header: self.header(),
            attempted_at,
            target,
            result,
        });
        self.repo.append(&record)
    }

    pub fn append_delete_attempted(
        &self,
        attempted_at: u64,
        target: EditTarget,
        result: ActionResult,
    ) -> Result<PathBuf, MirrorRepoError> {
        let record = MirrorRecord::DeleteAttempted(DeleteAttempted {
            header: self.header(),
            attempted_at,
            target,
            result,
        });
        self.repo.append(&record)
    }

    pub fn append_sync_run(
        &self,
        started_at: u64,
        finished_at: u64,
        mode: impl Into<String>,
        parameters: serde_json::Map<String, serde_json::Value>,
        emitted_cursors: serde_json::Map<String, serde_json::Value>,
    ) -> Result<PathBuf, MirrorRepoError> {
        let record = MirrorRecord::SyncRun(SyncRun {
            header: self.header(),
            started_at,
            finished_at,
            mode: mode.into(),
            parameters,
            emitted_cursors,
        });
        self.repo.append(&record)
    }

    /// Append a custom record for scrobble-scrubber-specific logging.
    ///
    /// Prefer using first-class `MirrorRecord` variants for stable data.
    pub fn append_custom(
        &self,
        at: u64,
        name: impl Into<String>,
        data: serde_json::Map<String, serde_json::Value>,
    ) -> Result<PathBuf, MirrorRepoError> {
        let record = MirrorRecord::Custom(crate::records::CustomRecord {
            header: self.header(),
            at,
            name: name.into(),
            data,
        });
        self.repo.append(&record)
    }
}
