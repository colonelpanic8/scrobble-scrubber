# Library Mirror Spec (Git-Backed, Append-Only Logs)

This document specifies a design for mirroring a Last.fm library in a way that:

- treats Last.fm as the source of truth
- is correct-by-construction (prefer duplicates/ambiguity over missing or wrong edits)
- is useful while incomplete (partial mirror still enables work)
- is robust to drift (edits/deletions not performed by this tool)
- works across multiple machines via git sync

The key constraint is that **git is not a multi-writer database**. To make multi-machine writes
merge-friendly, the synced dataset is **append-only logs** with **machine-namespaced files**.

Everything else (indexes/materializations) is rebuildable cache.

## Definitions

- **Upstream truth**: Last.fm.
- **Mirror repo**: a git repository on disk containing append-only JSONL logs.
- **Observation**: "At time X, upstream looked like Y".
- **Action**: an attempted edit/delete (and later verification).
- **Run**: a record of a sync attempt and the cursors/checkpoints it emitted.

## Data Repo Layout

All files below live inside the mirror repo root.

- `mirror.toml`: static configuration (optional).
- `log/observations/<machine_id>/YYYY/MM/DD.jsonl`
- `log/actions/<machine_id>/YYYY/MM/DD.jsonl`
- `log/runs/<machine_id>/YYYY/MM/DD.jsonl`

Rules:

- Files are append-only.
- Sharding is by UTC date derived from the record timestamp (`observed_at`, `attempted_at`, etc).
- Writers must only append to their own `<machine_id>` directory.
- Merge conflicts should be extremely rare in normal operation.

## Record Model

All records are JSON objects, one per line (JSONL).

Every record must include:

- `kind`: string discriminator
- `version`: integer schema version (start at `1`)
- `user`: Last.fm username this record applies to
- `machine_id`: writer identity, used for file sharding and debugging

### Observation: scrobble observed

`kind = "scrobble_observed"`

- `observed_at`: unix seconds (UTC)
- `source`: `"lastfm_api"` or `"lastfm_scrape"`
- `scrobble`:
  - `played_at`: unix seconds (UTC) as reported by Last.fm
  - `artist`, `track`: strings
  - `album`: optional string
  - `album_artist`: optional string
- `context`: optional object describing fetch context:
  - `range_from`, `range_to`: optional unix seconds
  - `page`: optional integer
  - `position`: optional integer (0-based order within response)
  - `url`: optional string

Notes:

- This record asserts only what was seen at `observed_at`, not that it remained stable.
- Missing intermediate edits are acceptable; periodic rescans converge to current upstream truth.

### Action: edit attempted / delete attempted

`kind = "edit_attempted" | "delete_attempted"`

Common fields:

- `attempted_at`: unix seconds (UTC)
- `result`:
  - `success`: bool
  - `error`: optional string

Edit target should store as much as is available (especially for single-instance edits):

- `target`:
  - `played_at`: optional unix seconds (for single-instance)
  - `artist`, `track`, `album`, `album_artist` (optional where unknown)
  - if available, full edit-form tuple (ExactScrobbleEdit-equivalent):
    - `track_original`, `artist_original`, `album_original`, `album_artist_original`

### Run: sync run summary / cursors

`kind = "sync_run"`

- `started_at`, `finished_at`: unix seconds (UTC)
- `mode`: `"tail" | "backfill" | "verify_range"`
- `parameters`: freeform object (overlap seconds, from/to range, etc)
- `emitted_cursors`: freeform object

Cursor state can be derived as "the latest run record for a given mode", optionally accelerated
via a local SQLite index.

### Custom records (extension point)

`kind = "custom"`

- `at`: unix seconds (UTC)
- `name`: user-defined kind string (e.g. `"rule_applied"`)
- `data`: freeform object

This exists to support scrobble-scrubber needs without immediately committing to a stable schema.
Prefer first-class record variants for data that becomes stable/important.

## Identity and Safety

Last.fm does not provide a stable scrobble ID usable everywhere.

We use a tiered identity model:

- **Edit-grade identity** (required for safe single-instance destructive operations):
  - `played_at` plus edit-form-grade metadata (scraped from Last.fm edit forms).
- **Mirror-grade identity** (sufficient for caching and candidate discovery):
  - best-effort key like `(played_at, artist, track, album?, album_artist?)` plus optional
    tie-breakers (response `position`, neighbor hashes, etc).

Rule:

- If a target is ambiguous, the system must refuse single-instance edits unless it can be
  enriched to edit-grade identity.

## Local Index (Not Synced)

Each machine may maintain a local SQLite index/materialization derived from logs for:

- fast queries (range scans, matching, ambiguity checks)
- incremental application (track "last applied log offset")
- computing current view and coverage/frontiers

This SQLite DB is a cache, never canonical.

## Git Sync

Sync policy belongs at the edges (CLI/service), not in core mirroring logic.

Typical loop:

1. `git pull --rebase`
2. append new records
3. `git add` only appended files
4. `git commit`
5. `git push` (retry on rebase races)

## Versioning

- Record schema starts at `version = 1`.
- Backwards-compatible additions allowed (new optional fields).
- Breaking changes require version bump and migration tooling.
