# Refactoring Notes (WIP)

This file tracks architecture issues and an ordered backlog for a redesign of scrobble-scrubber
and (optionally) lastfm-edit, focused on: (1) visibility of long-running work and waits, (2)
cancelability, and (3) reducing "time-to-first-useful-work" during long catch-up periods.

Related notes:
- `library-mirror-notes.md` (discussion about a partial/full local mirror and storage options)

## Implemented (So Far)

- lastfm-edit:
  - Added `ClientEvent::Delaying` for internal sleeps (retry backoff and operational edit pacing).
- scrobble-scrubber:
  - UI: added a delay indicator banner for `ClientEvent::Delaying`.
  - Cancellation: stop requests now interrupt page fetch, analysis, and edit application (best-effort).
  - Anchor search: emits progress info events during exponential/binary search.
  - Catch-up responsiveness:
    - `ScrubberConfig.max_pages_per_cycle` to chunk catch-up work across cycles.
    - `ScrubberConfig.preprocess_recent_pages` to process newest pages first without advancing the main anchor.
    - Persisted fast-path state (`CatchUpState`) to avoid reprocessing the same newest window every cycle.
    - When slow-path catch-up reaches the fast-path window, the main anchor can jump forward to the fast-path tip.

## Problem Statements

- Hidden waiting:
  - lastfm-edit performs internal sleeps (retry backoff, and also "operational delays" between
    edits) that can look like a hang from scrobble-scrubber UI/CLI.
- Composite ops are black boxes:
  - Some composite operations are implemented as "do everything internally", which makes it
    hard to interleave with other tasks, report granular progress, or cancel precisely.
- Catch-up brittleness / time-to-first-work:
  - When the scrubber hasn't run in a while, the current processing model tends to require
    scanning a large range of recent scrobbles to find/confirm the anchor boundary before it
    can safely advance state. That increases perceived latency and brittleness.

## Current Observations (Code Pointers)

- scrobble-scrubber forwards lastfm-edit events:
  - `lib/src/scrubber.rs` subscribes to `client.subscribe()` and forwards via
    `ScrubberEvent::client_event(...)` (`lib/src/events.rs`).
- lastfm-edit already emits:
  - Request lifecycle: `ClientEvent::{RequestStarted, RequestCompleted}`
  - Rate limiting: `ClientEvent::{RateLimited, RateLimitEnded}`
  - Edit attempts: `ClientEvent::EditAttempted`
- lastfm-edit also sleeps between edits (not rate limiting):
  - `lastfm-edit/src/client.rs` sleeps using `config.operational_delays.edit_delay_ms`
  - There is currently no event specifically describing that "cooldown" sleep.
- scrobble-scrubber incremental page processing exists:
  - `lib/src/page_search.rs` implements exponential+binary search for an anchor page.
  - `lib/src/scrubber.rs` processes pages `(1..=anchor_page).rev()` and advances the anchor
    after each processed page.

## Backlog (Ordered)

### 1) Make All Waiting Visible (lastfm-edit + scrubber UI/CLI)

- Add explicit events for non-rate-limit sleeps in lastfm-edit.
  - Add a `ClientEvent` variant like `Delaying { reason, delay_ms, until_timestamp, request? }`.
  - Emit it before sleeping for:
    - per-edit operational delays (`operational_delays.edit_delay_ms`)
    - any delete delay if implemented
    - optionally also retry sleeps (even though `RateLimited` exists, unify UX)
- Teach scrobble-scrubber UI and CLI to display these events clearly.
  - Reuse existing rate-limit indicator patterns; add a "cooldown" indicator/state.

Exit criteria:
- When lastfm-edit is sleeping for any reason, scrobble-scrubber shows a countdown and cause.

### 2) Make Long Operations Cancelable (Without Undefined Behavior)

- Ensure stopping the scrubber interrupts long waits promptly.
  - Option A (cheap): in scrobble-scrubber, wrap long-running client futures in
    `tokio::select!` with `should_stop` and drop the future on stop.
  - Option B (more correct): add a cancellation token API to lastfm-edit and implement sleeps
    as `select(sleep, cancel)` -> `Err(Cancelled)`.

Exit criteria:
- Stop request reliably ends "waiting" within <= 1s and returns to idle.

### 3) Encourage Streaming/Interleaving-Friendly APIs (Keep Composites, Build on Streams)

- Treat composite operations as wrappers around streaming primitives.
  - Prefer incremental iterators like `discover_scrobbles(...)` for large edits.
  - Consider adding "plan/execute" split:
    - plan/discover yields work items (edits)
    - execute applies work items with explicit pacing/policy controlled by caller
- Ensure composites emit granular progress:
  - per discovered item
  - per edit attempted
  - per page fetched (for discovery)

Exit criteria:
- UI/CLI can show progress continuously and the caller can interleave multiple workflows.

### 4) Catch-Up Redesign: Reduce Time-To-First-Useful-Work

Goal: after a long downtime, start doing useful work quickly without sacrificing correctness.

Options to evaluate (not mutually exclusive):
- Two-phase pipeline with a persisted queue/backlog:
  - Phase A: scan/discover and enqueue track timestamps/ids to process (persist as you go)
  - Phase B: process queue items (edits/rules) independently
  - Anchor can advance based on persisted queue state rather than "we processed everything"
- Process newest-first for responsiveness, but delay anchor advancement:
  - begin processing newest pages immediately
  - only commit anchor after confirming boundary coverage (may reprocess on crash, but avoids skip)
- Improve boundary search / cache support:
  - keep additional "page boundary hints" (timestamp per page) in cache so anchor-page search
    uses fewer requests and can resume safely
  - store "last scanned page and timestamp window" to resume scanning after crash

Exit criteria:
- First edits/rule applications can begin within seconds, even with a large backlog.
- No missed scrobbles; worst-case on crash is bounded reprocessing, not skipping.

### 5) Cache/State Model Changes (Likely Needed for 4)

- Re-evaluate what persistence stores:
  - currently: a single `TimestampState.last_processed_timestamp`
  - likely needed: a backlog cursor and/or durable queue metadata
- Add durable "scan progress" state separate from "processed progress".

Exit criteria:
- State model supports resuming mid-catch-up without a full rescan.

### 6) Versioning/Integration Work

- scrobble-scrubber currently depends on crates.io `lastfm-edit` (e.g. `4.0.0` in some crates).
  - For rapid iteration, switch to a path dependency to `/home/imalison/Projects/lastfm-edit`
    behind a feature flag or workspace override.
- Ensure tests cover:
  - "delay events" emission
  - cancellation during delay
  - catch-up pipeline invariants

Exit criteria:
- We can develop and test cross-repo changes quickly and reproducibly.

## Open Questions

- What is the "unit of work" we want to interleave?
  - track-level processing vs edit-level processing vs page-level scanning
- What correctness guarantee matters most?
  - never miss any scrobbles (no skipping) vs minimizing duplicate work vs fastest responsiveness
- Should the UI surface two independent progress bars?
  - scanning/discovery progress
  - processing/execution progress
