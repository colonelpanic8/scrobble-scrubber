# Refactoring / Redesign Tracker

This file is the running list of architectural changes and usability fixes we want to land.

Conventions:
- `[ ]` pending, `[x]` done
- Keep items small enough to review/land independently.

## Current Direction

- Keep iterator/composite operations as the *nice* interface.
- Make rate limiting / waiting / cancellation *visible* and *controllable* by the caller (CLI + UI).
- Move toward a git-backed, append-only “library mirror” (human readable logs) with a rebuildable local index for fast queries.
- Avoid “must scan all the way back before doing anything” behavior by using two-frontier/backfill strategies.

## Done

- [x] Add `mirror/` crate with git-backed append-only logs plus a local sqlite index (`mirror/src/index.rs`).
- [x] Add `scrobble-scrubber mirror ...` CLI commands (tip/older/status/recent/reindex).
- [x] Add mirror action-history queries (recent edits/actions) + CLI commands (`mirror edits`, `mirror actions`).
- [x] Make CLI lazy-login: only authenticate to Last.fm when a command truly needs it (`lib/src/cli/mod.rs`).
- [x] Surface Last.fm rate limit/backoff delays in terminal output (client events).
- [x] Add cooperative cancellation in `lastfm-edit` for internal delays/backoff (released as `lastfm-edit` v5.0.2).
- [x] Automatically append `ScrobbleObserved` + `EditAttempted` records to the mirror during scrubber runs.
- [x] Persist a unified two-frontier state (`FrontiersState`) and migrate scrubber + timestamp CLI to use it.
- [x] Make anchor-page search cooperatively cancelable (stop doesn't look like a hang).
- [x] App: set anchor by updating `FrontiersState` (avoid stale `frontiers_state`) and load timestamp track list from mirror.
- [x] Introduce `ScrubberHandle` so stop/cancel works even while the scrubber mutex is held (CLI Ctrl-C + app Stop).

## Next (High Priority)

- [ ] Expand mirror coverage beyond the main scrubber path:
  - Capture observations for other fetch paths (e.g. `process_last_n_tracks`, search flows) or centralize fetching behind an observing wrapper.
- [ ] Add progress events for long-running operations:
  - Surface “waiting for rate limit (sleep Xs)” distinctly from “doing network”.
  - Provide a consistent event stream usable in both terminal and UI.
- [ ] Add cancellation that actually works end-to-end:
  - Plumb a cancellation token from CLI/UI down to the network loop.
  - Ensure composite ops / iterators check it frequently.
  - Avoid holding a mutex over long runs so stop/cancel can be issued immediately.

## Scan/Backfill Strategy (Avoid Full Scan Before Work)

- [x] Implement a two-frontier strategy in state + scrubber loop:
  - Frontier A: newest pages (keep tip fresh, do not advance anchor)
  - Frontier B: slow-path anchor/backfill (advance anchor incrementally)
- [ ] Add “anchor/page search” based incremental backfill (already used by mirror older sync):
  - Use an anchor timestamp and search for the page containing it, then walk older pages.
- [ ] Investigate “strided backward search” (your idea):
  - Jump back by increasing stride until we bracket the anchor region, then refine.
  - Only do this if it materially reduces API calls vs current anchor search, given Last.fm’s pagination constraints.

## Storage / Caches

- [ ] Re-evaluate `TrackCache` and “queue cache”:
  - Decide what stays vs what becomes “queries over the mirror index”.
  - Reduce the number of separate caches that can get out of sync.
- [ ] Remove or migrate remaining `TrackCache` command surfaces:
  - `track-cache load-artist` currently still hits legacy cache code.
  - Prefer “mirror observations + indexed queries” instead.
- [ ] Mirror repo layout + invalidation:
  - Define log sharding strategy (by month/day) and rotation rules.
  - Add “invalidate and rebuild index” workflow (CLI exists; refine semantics).

## lastfm-edit (Usability / Control Surface)

- [ ] Expose rate limiting as a caller-controlled policy (instead of burying sleeps internally):
  - A hook/callback for “about to wait N seconds” and “retrying request”.
  - Optional “no internal sleeps” mode where library returns “rate limited, retry after N”.
- [x] Implement cancellation support in lastfm-edit:
  - Interrupts internal backoff/operational sleeps (best-effort).
  - Callers can request cancel via `LastFmEditClient::cancel()`.
- [ ] Interleaving composite operations:
  - Make it possible for callers to multiplex work fairly without head-of-line blocking.
