# Library Mirror Notes (WIP)

Notes from discussion about building a local "full but useful while partial" mirror of Last.fm
data for scrobble-scrubber.

## Goals

- Have a locally cached view of Last.fm library/recent scrobbles that:
  - is useful even when only partially populated
  - can be incrementally extended over time (fetch more, reconcile as needed)
  - supports flexible workflows (scan/discover, process/apply edits, reprocess)
  - can be invalidated gracefully (full reset or partial invalidation)
- Improve usability after long downtime:
  - reduce "time-to-first-useful-work"
  - avoid brittle "must fully scan to anchor before anything happens"

## Constraints / Realities

- Last.fm UI endpoints are page-based, newest-first, and page numbers are not stable across time.
  - Therefore, caching "page N" is inherently brittle.
- We likely care about correctness properties like "do not miss scrobbles".
  - It is acceptable to do bounded duplicate work (reprocessing) if it avoids skipping.

## Key Insight

Treat pages as transport only. Cache *items* (scrobbles/tracks) and maintain a notion of *coverage*
or *frontiers* independent of page numbers.

## Data Model Sketch (Backend-Agnostic)

- Scrobble item storage (upsert / de-dup):
  - Best-effort identity key (tradeoff: may collide, should prefer duplicates over misses):
    - candidate: `(timestamp, artist, track, album?, album_artist?)` plus a derived hash/id
  - Store:
    - metadata fields (artist, track, album, album_artist, timestamp)
    - `first_seen_at`, `last_seen_at`
    - processing status (`unprocessed`, `processed`, `failed`, etc.)
- Coverage / frontier storage:
  - Separate "scan frontier" (what time ranges we have discovered) from "processed frontier"
    (what time ranges we have actually processed/applied edits for).
  - Optional: store "coverage intervals" (time ranges known fetched) to make partial mirror useful.
- Invalidation:
  - `invalidate-all`: drop mirror state
  - `invalidate-since(ts)` / `invalidate-range(a,b)` / `invalidate-artist(name)`:
    - mark coverage gaps and/or mark items stale so they must be reverified/refetched

## Storage Backend Options (Tradeoffs)

- SQLite:
  - Pros:
    - range queries by timestamp and secondary indexing are straightforward
    - crash-safe incremental upserts; easy to represent "coverage/frontiers"
    - portable as a single file across machines (when not concurrently accessed)
  - Cons:
    - not plaintext/human-readable on disk
    - needs migration/versioning discipline
- Plaintext/human-readable:
  - A single JSON file is likely too heavy at 1-2M scrobbles (rewrite cost + corruption risk).
  - JSONL (append log) + compaction could work, but you still need an index for queries.
- RocksDB-like:
  - Pros: excellent ordered key-value store for large datasets.
  - Cons: not human-readable, more ops/deps, still need to design indexing/query patterns.

Conclusion (tentative):
- SQLite seems like the pragmatic default for the mirror/index, but it is not required to fix the
  immediate usability issues; we can stage this later.
- If human readability matters, support export/import as JSONL for inspection/debugging.

## Relationship To Near-Term Usability Work

This mirror effort is ambitious and not necessary for early wins. Near-term usability improvements
we already identified:

- surface internal waits (rate limit backoff and per-edit operational delays) as explicit events
- display those waits clearly in UI/CLI (countdown + reason)
- make long waits cancelable (stop should interrupt promptly)
- emit progress for scanning/anchor search so "nothing happening" is never the default

