use crate::records::MirrorRecord;
use crate::repo::MirrorRepoError;
use std::path::PathBuf;

/// Minimal interface for integrations (scrobble-scrubber, CLIs, services).
///
/// This intentionally stays narrow: append records, get back the relative path written.
/// Query/indexing is expected to be provided by a separate local indexer layer.
pub trait MirrorSink: Send + Sync {
    fn append(&self, record: &MirrorRecord) -> Result<PathBuf, MirrorRepoError>;
}

/// No-op sink to keep call sites simple when mirroring is disabled.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullMirrorSink;

impl MirrorSink for NullMirrorSink {
    fn append(&self, _record: &MirrorRecord) -> Result<PathBuf, MirrorRepoError> {
        Ok(PathBuf::new())
    }
}
