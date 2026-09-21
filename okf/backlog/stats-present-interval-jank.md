---
title: The stats window has no present-interval jank counter, so a repeated frame can pass every figure clean
description: missedPresents (raster-side, demand-gated, run-based counting) is implemented and is the figure probes quote; remaining are maxPresentGapMs and per-platform validation of the present timestamps (ANGLE/D3D11, macOS, Android).
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

## Why the other fields miss it

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

## Implemented: missedPresents (stage 1)

Since 2026-09-21 the count comes from the refresh count on every frame
signal ([frame-signal-refresh-count](../plans/frame-signal-refresh-count.md),
stage 2): the main loop counts the display refreshes each signal covers
(`alloy/src/present.rs`), and for a present that follows a demanded one,
the refreshes of the interval beyond the first are the misses
(`RefreshCounting::count`; idle Ticks between the two presents fold into
the interval). The raster thread now only samples the demand latch at
present time and sends it with the present's instant
(`FrameOutput::Presented`). One mechanism produces the app timeline's
advance and the jank count, on the same instants. The paragraphs below
describe the raster-side run accounting this replaced; the two design
points still hold, the second now being the counter's own tolerance.

As first implemented, measured at the present, where the truth lives:
`record_present_interval` in alloy/src/raster/mod.rs ran as each
interactive present returned from the swap, accumulated into
`RasterStats::missed_presents`, and the count rides `RasterCounters`
into `/stats` - cumulative next to `fenceTimeouts`, and as a count over
the window (frame_history `RasterRates`) next to the per-frame rates.
The get_stats tool description and packages/cli/agents/debugging.md say
to quote the window's `missedPresents` as the primary jank figure;
`slowFrames`, `gpuFrameExecMsPerFrame`, `fenceTimeouts`,
`rasterCmdMsPerSec` attribute the cause.

Two design points that settled the shape:

- THE DEMAND GATE: rendering is demand-driven, so presents legitimately
  stop when nothing changes. An interval only counts while a next frame
  was demanded when the previous one presented; a gap with no demand is
  idle, not jank. The raster thread samples (never consumes) the
  frame-request latch at present time - forwarded once at startup via
  `RasterCmd::SetDemandLatch` from the platform loop's
  `SetFrameRequestLatch` handler. The timing did NOT work out by itself
  (found 2026-09-21 while building the cadence hold): the draw gate
  consumes the latch after the frame's JS has re-registered its `onFrame`
  callback, so at present time it read false for exactly the apps that
  animate, and the counts had only come out right because a stray idle
  Tick re-set it between the gate and the present. Standing demand is now
  declared by its sources (core's `requestFrame` export, the rAF queue,
  running transitions) and re-latched past the gate, so the latch is set
  at present time mid-animation and clear after a one-shot frame by
  construction (okf/plans/cadence-hold.md, Findings).
- RUN-BASED COUNTING, not a per-interval threshold: swap-return
  timestamps jitter by more than half a refresh period under
  mailbox/triple-buffered compositors - the documented reason the
  animation clock paces by present count, not timestamps - so judging
  each interval against a 1.5x-period threshold (the originally proposed
  shape) would latch phantom misses on a healthy client. Instead each
  contiguous demanded run is judged as a whole: misses = whole periods
  spanned (minus `JANK_JITTER_SLACK` = 0.25) minus presents delivered,
  reported against a per-run high-water mark so a jittery reading never
  counts twice. Summing the span before dividing cancels the per-swap
  jitter; it survives only at run boundaries, where the slack absorbs
  it. A run restarts on a refresh-rate change (mixed periods) and on a
  skipped swap (minimized window).

Answers to the questions that were open:

- The paced frame clock advances by the display refreshes alloy counts per
  frame signal (since [frame-signal-refresh-count](../plans/frame-signal-refresh-count.md);
  before that it accumulated one period per signal to hide swap jitter, and
  a JS-side tick-gap counter was structurally blind). A tick delta of two
  periods is now a readable miss from JS; the ledger that count is recorded
  in is where `missedPresents` moves in that plan's second stage.
- The skip half of 4-4-6 needs no count of its own: a missed present
  implies the content skip on a paced timeline. Revisit only if a report
  ever shows content skipping without missed presents (a pacing bug, not
  a stats gap).

Known sampling caveat, shared with every windowed raster rate: the window
count is the counter differenced between the first and last frame records
inside the window, so misses that land before the first in-window record
(e.g. during a reload's teardown/reseed, when no frames are recorded) show
only in the cumulative `missedPresents`, not the window count. Observed on
the tiles example: a reload's seed re-bake froze the demanded animation
for a few hundred ms (+9 cumulative, hitch logger agreed) while the
following window read 0.

## Remaining (stage 2)

- `maxPresentGapMs` with its `ageMs`: the worst single gap, for
  severity. Timestamp-based and so jitter-noisy, but a max is
  diagnostic, not a count.
- Platform validation: the counting must hold against how each platform
  actually paces presents (ANGLE/D3D11 present-fence pacing, macOS,
  Android). Run the present-fence probes
  (alloy/examples/present_fence_probe.rs) per platform and confirm a
  healthy client reads 0 before trusting the figure there. Linux is
  validated at stage 1.
