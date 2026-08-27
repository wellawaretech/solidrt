---
title: GPU timer stats are unusable on tiled GPUs, and gpuFrameExecMs can return garbage
description: On Adreno the per-pass execMs and gpuFrameExecMs figures move with unrelated state, invert against ground truth, and on a frame with no passes report 401 ms of GPU time in a 17 ms frame, so anyone optimising from them is led the wrong way.
created: 2026-08-27
---

# GPU timer stats are unusable on tiled GPUs, and gpuFrameExecMs can return garbage

Two symptoms with one investigation behind them. The first is a measurement
model that does not survive a tiler; the second is a plain defect that shows
up regardless.

## Symptom 1: the numbers contradict ground truth

Measured on an Adreno 610 (Samsung SM-T500, Android 12, GLES 3.2) running
[packages/3d/demos/src/the-third-dimension.tsx](../../packages/3d/demos/src/the-third-dimension.tsx),
which draws six offscreen passes and composites three `d-texture` leaves.

`gpuFrameExecMs` reported 8.4, 11.5, 18.6 and 22.2 ms for the same composite
work, varying with state that cannot affect it:

- Switching shadow casters off moved it from 8.4 to 18.6 ms while pass exec
  fell and the frame got faster.
- Attaching a second client (a desktop window on the same server) moved it
  from 22.0 down to 8.5 ms.
- Adding `fullscreen` to `<window>` grew the composited area by 10 percent
  and the figure HALVED, 18.3 to 8.35 ms.

The per-pass `execMs` values drift the same way and can invert against the
frame time outright: at renderScale 0.35, turning the three casters off took
the app from 24.3 to 36.3 fps while the summed GPU total went UP, 17.0 to
21.7 ms.

Ground truth for the same app, obtained by subtraction (change one variable,
take the delta in wall-clock frame time):

| | timer says | actually |
|---|---|---|
| window composite, 3 textures + text at 2000x1200 | 8-22 ms | 1.9 ms |
| one render pass, 128x128, constant shader | ~1 ms | 2.15 ms |

## Symptom 2: 401 ms of GPU time in a 17 ms frame

With zero GPU passes running - a window drawing one full-window `d-rect`,
nothing else - `gpuFrameExecMs` advanced by about 401 ms per frame while the
frame took 17 ms and the display was capped at 60 Hz. The value was stable
across four different window contents, which rules out noise. Twenty-three
seconds of GPU time per wall-clock second is not a misattribution, it is a
bad read: a harvest against a query that never completed, a stale or
uninitialised accumulator, or a unit error on the no-pass path.

Reproduce with [packages/3d/demos/src/floor-probe.tsx](../../packages/3d/demos/src/floor-probe.tsx),
`mode` 0 through 3 with `passes` 0.

## Cause

For symptom 1, the measurement model does not hold on a tiled deferred
renderer. `PassTimer` brackets command submission, but a tiler defers the
actual tile execution for an offscreen target until its results are needed -
typically when the window draw samples it. That work then lands inside
whichever timer query happens to be open, so the split between
`pass_exec_micros` and `frame_exec_micros` is decided by draw order and by
what samples what, not by where the work belongs. Every anomaly above is
consistent with this: the figure tracks how much deferred tile work the
window draw pulled in, which is why it falls when there is less to pull and
rises when there is more.

Symptom 2 is separate and wants a straight read of the harvest path in
[alloy/src/raster/mod.rs](../../alloy/src/raster/mod.rs) around
`frame_exec_micros`, since a no-pass frame is exactly the case where a
query may be harvested without ever having been issued.

## Why it matters

These figures are not diagnostics of last resort. They are what `/stats`
serves, what the MCP tooling reports, and what the on-screen overlay shows,
so they are the first thing anyone reads when an app is slow. During this
investigation they pointed at the window composite as a 22 ms bottleneck; it
is 1.9 ms. Acting on that number would have meant rewriting the compositing
path to fix a cost that was never there.

## What done looks like

Either the numbers mean something defensible on a tiler, or they say they
cannot. Any of these closes it:

- Report the timers as unavailable where the split cannot be trusted, the
  way `timer_queries: false` already reports absence, rather than serving a
  number that reads as authoritative. A tiler is detectable from the GL
  renderer string, and Adreno/Mali/PowerVR is most of the Android fleet.
- Keep a single per-frame total, which is defensible because deferred tile
  work still lands somewhere inside the frame, and drop the per-pass split.
- Fix the attribution properly with `GL_TIMESTAMP` queries around a
  `glFlush` per pass, accepting that forcing the flush changes what is being
  measured, which may make this not worth doing.

Symptom 2 should be fixed regardless of which route symptom 1 takes: no
configuration should be able to report more GPU time than wall-clock time.
A cheap assertion that the harvested figure does not exceed the frame's
wall-clock duration would have caught it at the source.

## Also worth doing

`frameMs` is a smoothed EMA and disagrees with the frame counter under
bimodal frame times (48.8 ms EMA against 40.8 ms measured over the same
window). Anything comparing configurations should divide a frame-counter
delta by a `timeMs` delta instead. Worth a sentence in
[packages/cli/agents/debugging.md](../../packages/cli/agents/debugging.md),
which currently sends readers to `/stats` without saying which of its fields
survive a comparison.
