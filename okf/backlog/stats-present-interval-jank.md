---
title: The stats window has no present-interval jank counter, so a repeated frame can pass every figure clean
description: Jank is a presented frame whose content does not advance one step - a repeat then a skip (4 4 6) - and none of the window fields count it directly; slowFrames sees only the JS critical path, fenceTimeouts only fires past 100ms, fps and the per-frame averages are blind to a single miss. Count missed presents on the raster thread and make that the figure probes quote.
created: 2026-08-31
---

# The stats window has no present-interval jank counter

## The definition

Jank is a presented frame whose CONTENT does not advance one step along
the animation timeline. The signature of a missed deadline on a paced
clock is a repeat followed by a skip: where the screen should show
`1 2 3 4 5 6`, it shows `1 2 3 4 4 6` - the display repeats image 4 for
one refresh, and because the animation clock keeps real time (correctly:
pacing never slows time down), the next rendered frame samples one step
ahead and content 5 is never seen. The total frame count over a window is
unchanged, so `fps` reads exactly 60 and every per-second average is
blind. Jank is a per-frame worst-case phenomenon; only per-interval
counting sees it.

## Why the current fields miss it

- `window.slowFrames` (lattice/src/frame_history.rs: `total_ms >
  period_ms`) judges the JS-thread critical path only. A frame that
  misses its slot on the raster thread, in GPU execution, or at present
  shows `slowFrames: 0`.
- `fenceTimeouts` fires only when a present-fence wait expires at
  `PRESENT_FENCE_TIMEOUT_NS` = 100 ms (alloy/src/raster/mod.rs). A 33 ms
  GPU frame - two refresh periods, a plainly visible repeat - counts
  nowhere.
- `gpuFrameExecMsPerFrame` and the other window rates are averages; one
  spiked frame hides inside them.

A probe run can therefore quote every current figure clean while the
screen showed 4 4 6. Today the only direct measurement is an app-side
onFrame tick-gap logger (the ad-hoc `[hitch]` line some examples carry),
which should not be each example's job.

## Shape

Measure at the present, where the truth lives: the raster thread already
brackets the present-fence wait per frame (raster/mod.rs measures
`wait_ms`), so it knows when each frame actually presented.

- Count an interval between consecutive presents that exceeds a
  JANK_INTERVAL_FACTOR (~1.5) times the refresh period as one missed
  present. Carry `missedPresents` and `maxPresentGapMs` (with its
  `ageMs`) through `RasterCounters` into the `/stats` window next to
  `slowFrames`, as counts over the window like the other rates.
- THE DEMAND GATE WRINKLE, the one design point that needs care:
  rendering is demand-driven, so presents legitimately stop when nothing
  changes. An interval only counts as missed while a next frame was
  actually demanded (latched) when the previous one presented; a gap
  with no demand is idle, not jank. The latch state at present time
  decides, not the interval alone.
- Attribution stays with the existing fields: `missedPresents` says THAT
  it janked; `slowFrames`, `gpuFrameExecMsPerFrame`, `fenceTimeouts`,
  `rasterCmdMsPerSec` say why. The stats tool description and
  packages/cli/agents/debugging.md get one line each: probes quote
  `missedPresents` as the primary jank figure, the rest as cause.

## Open before implementing

- Whether the paced frame clock's tick timestamps derive from actual
  presents or are scheduled ahead of them. If they derive from presents,
  a JS-side tick-gap counter is equivalent and nearly free - but the
  raster-side count is authoritative either way, and one implementation
  is better than two.
- Platform present semantics differ (ANGLE/D3D11 present fence pacing,
  macOS, Android): the interval must be measured against the same clock
  that paces the platform, or a healthy client shows phantom misses.
  The present-fence probes (alloy/examples/present_fence_probe.rs) are
  the place to validate per platform.
- Whether the skip half of 4-4-6 needs its own count. A missed present
  implies the content skip on a paced timeline, so counting misses
  should subsume it; revisit only if a report shows content skipping
  without missed presents (that would be a pacing bug, not a stats gap).
