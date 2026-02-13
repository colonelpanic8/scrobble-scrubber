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
- [x] Make CLI lazy-login: only authenticate to Last.fm when a command truly needs it (`lib/src/cli/mod.rs`).

## Next (High Priority)

- [ ] Integrate mirror writes into the scrubber runtime:
  - Emit `ScrobbleObserved` whenever we fetch pages for processing.
  - Emit records for edit attempts/outcomes (so the mirror is also an audit log).
- [ ] Add progress events for long-running operations:
  - Surface “waiting for rate limit (sleep Xs)” distinctly from “doing network”.
  - Provide a consistent event stream usable in both terminal and UI.
- [ ] Add cancellation that actually works end-to-end:
  - Plumb a cancellation token from CLI/UI down to the network loop.
  - Ensure composite ops / iterators check it frequently.

## Scan/Backfill Strategy (Avoid Full Scan Before Work)

- [ ] Implement a two-frontier strategy:
  - Frontier A: newest pages (keep tip fresh)
  - Frontier B: backfill older history gradually
- [ ] Add “anchor/page search” based incremental backfill (already used by mirror older sync):
  - Use an anchor timestamp and search for the page containing it, then walk older pages.
- [ ] Investigate “strided backward search” (your idea):
  - Jump back by increasing stride until we bracket the anchor region, then refine.
  - Only do this if it materially reduces API calls vs current anchor search, given Last.fm’s pagination constraints.

## Storage / Caches

- [ ] Re-evaluate `TrackCache` and “queue cache”:
  - Decide what stays vs what becomes “queries over the mirror index”.
  - Reduce the number of separate caches that can get out of sync.
- [ ] Mirror repo layout + invalidation:
  - Define log sharding strategy (by month/day) and rotation rules.
  - Add “invalidate and rebuild index” workflow (CLI exists; refine semantics).

## lastfm-edit (Usability / Control Surface)

- [ ] Expose rate limiting as a caller-controlled policy (instead of burying sleeps internally):
  - A hook/callback for “about to wait N seconds” and “retrying request”.
  - Optional “no internal sleeps” mode where library returns “rate limited, retry after N”.
- [ ] Implement cancellation support in lastfm-edit:
  - Cancellation token passed through composite ops and HTTP layer.
  - Ensure iterator/composite ops can be interrupted promptly.
- [ ] Interleaving composite operations:
  - Make it possible for callers to multiplex work fairly without head-of-line blocking.

