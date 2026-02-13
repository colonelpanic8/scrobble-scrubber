use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordSource {
    LastfmApi,
    LastfmScrape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scrobble {
    /// Unix seconds (UTC) as reported by Last.fm.
    pub played_at: u64,
    pub artist: String,
    pub track: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_from: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_to: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// 0-based order within a response/page (best-effort tie-breaker).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordHeader {
    pub version: u32,
    pub user: String,
    pub machine_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrobbleObserved {
    #[serde(flatten)]
    pub header: RecordHeader,
    pub observed_at: u64,
    pub source: RecordSource,
    pub scrobble: Scrobble,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<FetchContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played_at: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,

    // Edit-form-grade originals (ExactScrobbleEdit-equivalent), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_original: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_original: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_original: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist_original: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditAttempted {
    #[serde(flatten)]
    pub header: RecordHeader,
    pub attempted_at: u64,
    pub target: EditTarget,
    pub result: ActionResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteAttempted {
    #[serde(flatten)]
    pub header: RecordHeader,
    pub attempted_at: u64,
    pub target: EditTarget,
    pub result: ActionResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRun {
    #[serde(flatten)]
    pub header: RecordHeader,
    pub started_at: u64,
    pub finished_at: u64,
    pub mode: String,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub parameters: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub emitted_cursors: serde_json::Map<String, serde_json::Value>,
}

/// Extension point for scrobble-scrubber-specific logging without a schema bump.
///
/// This is intentionally "data-shaped" and should be used sparingly for records that
/// are not yet stable enough to deserve a first-class variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomRecord {
    #[serde(flatten)]
    pub header: RecordHeader,
    /// Unix seconds (UTC) for sharding and time ordering.
    pub at: u64,
    /// User-defined kind name (e.g. "rule_proposed", "rule_applied", "scan_progress").
    pub name: String,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub data: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MirrorRecord {
    ScrobbleObserved(ScrobbleObserved),
    EditAttempted(EditAttempted),
    DeleteAttempted(DeleteAttempted),
    SyncRun(SyncRun),
    Custom(CustomRecord),
}

impl MirrorRecord {
    pub const VERSION: u32 = 1;

    pub fn user(&self) -> &str {
        match self {
            MirrorRecord::ScrobbleObserved(r) => &r.header.user,
            MirrorRecord::EditAttempted(r) => &r.header.user,
            MirrorRecord::DeleteAttempted(r) => &r.header.user,
            MirrorRecord::SyncRun(r) => &r.header.user,
            MirrorRecord::Custom(r) => &r.header.user,
        }
    }

    pub fn machine_id(&self) -> &str {
        match self {
            MirrorRecord::ScrobbleObserved(r) => &r.header.machine_id,
            MirrorRecord::EditAttempted(r) => &r.header.machine_id,
            MirrorRecord::DeleteAttempted(r) => &r.header.machine_id,
            MirrorRecord::SyncRun(r) => &r.header.machine_id,
            MirrorRecord::Custom(r) => &r.header.machine_id,
        }
    }

    /// Used for sharding paths. Each record type uses its own primary timestamp.
    pub fn shard_unix_seconds(&self) -> u64 {
        match self {
            MirrorRecord::ScrobbleObserved(r) => r.observed_at,
            MirrorRecord::EditAttempted(r) => r.attempted_at,
            MirrorRecord::DeleteAttempted(r) => r.attempted_at,
            MirrorRecord::SyncRun(r) => r.started_at,
            MirrorRecord::Custom(r) => r.at,
        }
    }
}
