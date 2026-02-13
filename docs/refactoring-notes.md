# Notes From Architecture Discussion (2026-02-13)

This is a capture of the current pain points and the design direction we’ve been converging on.

## Pain Points Observed

- Composite operations (often calling into `lastfm-edit`) can “hang” for a long time due to internal rate limiting sleeps.
  - From the outside, the system looks stuck: it’s hard to report “we’re sleeping X seconds because of rate limit” in terminal/UI.
  - Cancellation attempts have been unreliable: state is buried and not easily interruptible.
- Some composite ops want to interleave (fair scheduling), but a single long composite op can hog the execution pipeline.
- Catch-up after not running for a long time is brittle:
  - Current structure requires scanning back to a previous point before doing useful work.
  - This creates long “no progress” periods and makes it hard to advance cursors incrementally.
- Existing caching structures (notably `TrackCache` and the “queue cache” approach) feel awkward:
  - Hard to populate incrementally.
  - Easy to invalidate accidentally, and awkward to reconcile with the website/API pagination model.

## Principles / Preferences

- Keep iterators + composite ops as the high-level interface (they’re ergonomic and expressive).
- The issue is not “composites exist”; it’s that internal waiting/cancellation/progress is too opaque.
- We want a locally cached view of Last.fm data that:
  - Is useful while partial.
  - Can be reconciled and extended over time.
  - Can be invalidated and rebuilt without losing auditability.
  - Works across machines (git-friendly is a big plus).

## Mirror Store Direction

- Maintain an append-only, human-readable log of observations and actions (JSONL) suitable for git sync.
- Maintain a local rebuildable index (sqlite) for query performance.
  - The sqlite file is machine-local state; it can be regenerated from logs.
  - Logs are the durable “source of truth”.
- This addresses:
  - Multi-machine usage (git pull/push the logs).
  - Partial-but-useful behavior (index queries return whatever we’ve observed).
  - Dynamic reconciliation (keep appending observations from different fetch paths).

## Avoid Full Back-Scan Before Doing Work

Two strategies we should support (not mutually exclusive):

- Two-frontier approach:
  - Always keep the “tip” fresh (newest pages).
  - Backfill older history as a separate ongoing job/frontier.
  - This ensures progress even after long gaps.
- Strided backward search (conceptually binary-search-ish):
  - Jump back with increasing stride until we bracket a target region, then refine.
  - Whether this is possible/effective depends on what stable anchor we can use given Last.fm pagination semantics.

## lastfm-edit Changes We Expect

We likely want to keep composite ops but change how rate limiting is modeled:

- Make rate limiting visible:
  - Emit events: “rate limited, sleeping N seconds”, “retrying request”, etc.
- Make cancellation real:
  - A cancellation token should be checked frequently in iterators/composites and during sleeps.
- Consider a “core HTTP client” layer:
  - Low-level calls return structured errors including rate-limit info.
  - Composite ops remain in a higher layer that can interleave/multiplex work and surface progress.

