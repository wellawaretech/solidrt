---
title: Stats overlay phase shares go stale when frames are reused
description: LAY/PNT/PST/HOV divide a phase average that only updates on a full rebuild by a frame period that updates every frame, so an app in steady-state display-list reuse shows a frozen number as a live share (PNT 350% in the-third-dimension); the same stale figures are served over /stats.
created: 2026-08-27
completed: 2026-08-27
---

# Stats overlay phase shares go stale when frames are reused

## Symptom

`packages/3d/demos/src/the-third-dimension.tsx`, running normally at 61 fps,
shows `PNT 350%` in the stats overlay. Nothing is wrong with the app: it is
painting nothing at all.

## Mechanism

`overlay.rs` renders each phase as `phase_ms / frame_ms`. The two numbers are
written by different callers on different schedules:

- `Stats::record_js` writes `frame_ms` on **every** render-handler invocation,
  gated or not.
- `Stats::record_frame` writes the phase EMAs (`layout/post/paint/hover`) only
  on a **full rebuild**. `RenderInner::render` returns before it on both the
  skip path (`driver.begin` gave no frame) and the reuse path
  (`Commit::Reused`).

An app whose tree is static but whose pixels change - the whole GPU/3d shape,
where content damage forces a present but the display list is unchanged - takes
the reuse path on every frame forever. The numerator then freezes at the last
rebuild's value while the denominator stays live, and the ratio is whatever
those two unrelated moments happen to divide to.

The smoothing makes the freeze exact rather than approximate: `smooth()` weights
by the gap since the previous sample, so a rebuild after a long gap has
alpha ~ 1 and the EMA lands **on** that one sample. The displayed figure is one
frame's measurement, not an average, presented as a live share.

## Evidence (2026-08-27, port 34886)

```
fps 61   frameMs 18.8-20.6   jsMs 0.24   paintMs 70.61
reusedPerSec 60-61   skippedPerSec 0   window.frames 0
nodes 10   setPropsPerFrame 0   dirtiedNodes 0   nodesPainted 14
```

`paintMs` is bit-identical at 70.61 across samples minutes apart; `frameMs`
moves. The log names the frame it froze on, ~6 minutes before the reading:

```
Slow frame: 71.2 ms (budget 16.7): js 0.5, layout 0.0, postLayout 0.0,
paint 70.6, hover 0.0; ... nodesPainted 14
```

70.61 / 19 = 372%.

## Blast radius

- LAY/PST/HOV are stale in the same way. They froze on small numbers, so only
  PNT is loud enough to notice.
- The raw counters latched in the same block are equally stale: `nodesPainted`,
  the boundary/snapshot counts, and the `LayoutCounters` set (`paraShapes`,
  `measureCalls`, `dirtiedNodes`, `cacheHits/cacheGets`). Current stats report
  `nodesPainted 14` from a frame six minutes gone.
- Not overlay-only. `StatsSnapshot` is what the control API serves, so
  `/stats` (and every agent, the console app, and any MCP reader of it) sees
  the same frozen fields with nothing marking them stale.
- The comment above the format string in `overlay.rs` states that a share
  stays within 100% because the two figures are smoothed the same way on the
  same thread. That reasoning holds only while every frame rebuilds, and the
  comment should go with the fix.

## Shape of the fix

Minimal: feed a zeroed `FramePhases` on the reuse and skip paths, so the EMAs
decay to 0 when nothing rebuilds. That is true - the phase really did cost
nothing this frame - it is two lines at the two early returns, and it makes the
overlay say "this app is not painting", which is the useful reading. The cost of
the rare rebuild stays visible in the slow-frame warning and in the `/stats`
window percentiles, which are computed from `frame_history` and already report
`frames: 0` honestly when nothing rebuilt.

Alternatives, if the rebuild cost should stay on screen:

- Divide the phases by the rebuild frame's own period rather than the live one.
  Keeps the share meaningful, but it is then a share of a frame that may be
  minutes old and nothing on screen says so.
- Carry an age on the snapshot and let readers mark the phase block stale.
  Correct for `/stats` consumers, more surface than the overlay needs.

The stale raw counters want the same decision: zero them alongside, or age them.

## Resolution (2026-08-27)

The minimal shape. `RenderInner::render` now calls `Stats::record_frame` with a
zeroed `FramePhases` on both early returns (the skip path after `driver.begin`
and the `Commit::Reused` path), so the phase EMAs are fed on every frame the JS
thread sees, on the same cadence as `frame_ms`. A static tree decays to
LAY/PNT/PST/HOV 0% within ~1 s (SMOOTH_TAU 0.15 s) and the overlay reads "this
app is not painting". The rebuild's cost stays visible in the slow-frame
warning and the `/stats` window percentiles, as before. The overlay comment now
states the cadence argument; `record_frame` and `FramePhases` document the zero
contract.

The `PaintStats` block (`nodesPainted`, the BND/SNP boundary and snapshot
counts) is zeroed on the same two paths: the overlay presents those as what the
current frame did, so on a reusing app the BND/SNP lines now hide (they are
gated on a nonzero sum) and `/stats` reports `nodesPainted 0`. The last
rebuild's count is still in `window.worst`.

The `LayoutCounters` set (`paraShapes`, `measureCalls`, `dirtiedNodes`,
`cacheHits/cacheGets`) and `node_count` stay latched on purpose. They are not
on the overlay, and the get_stats doc describes them as the last full rebuild's
figures, which is the useful reading for counts you reason about (zero for
every reusing app says nothing). If an age turns out to be needed for `/stats`
readers, that is the snapshot-age alternative above.

## Not part of this item

The 70.6 ms paint on a 14-node tree (and 85 ms for 8 nodes at startup) that
the stale figure happened to freeze on is a separate question, captured as a
line in `okf/ideas.md` (raster-thread RPCs behind the queue on a resize frame).
