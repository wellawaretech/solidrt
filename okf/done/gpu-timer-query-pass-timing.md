---
title: Report GPU-side pass duration, not raster-thread occupancy
description: get_stats reports gpuPassMs as the raster thread's time issuing passes, so a pass that is cheap to issue and expensive on the GPU reads as free; timer queries would make the number mean what its name says.
created: 2026-08-13
---

# Report GPU-side pass duration, not raster-thread occupancy

What it looks like when you hit it: a client is visibly dropping frames, the
GPU is clearly busy, and `gpuPassMs` stays small. The counter is honest about
what it measures - wall time the raster thread spent issuing passes - but that
is CPU-side, so a pass with a heavy fragment shader and a trivial issue cost is
invisible in it. An agent reading stats concludes the GPU is fine.

The fix is timer queries: GLES disjoint timer queries where available, whatever
Impeller can surface elsewhere, reported next to the existing counters so the
issue-side and GPU-side numbers can be compared rather than confused.

This is the engine-side half of what used to be one deferred item on
cross-platform GPU attribution. The counters that did land from it - passes,
presents, reuse/skip, fence timeouts - are already in `get_stats`; this is the
part that did not. The whole-system half is
[gpu-system-attribution](gpu-system-attribution.md), and the Linux probe that
motivated all of it is in
[measuring which process burns the GPU](../notes/gpu-burn-attribution-linux.md).

## Landed 2026-08-23: stage 1, pass execution time

`gpu::PassTimer` (alloy/src/gpu/timing.rs) wraps every pass - dirty flush,
`renderTarget`, `copyTexture`, node shaders - in a `TIME_ELAPSED` query and
harvests retired results non-blocking at the top of each raster command, so
the numbers lag a pass by a frame or two and never stall. Supported via
`GL_EXT_disjoint_timer_query` (GLES: Mesa, Android, ANGLE) or core timer
queries on desktop GL 3.3+; `GPU_DISJOINT_EXT` drops a harvested batch.

The existing counter was renamed so the pair says what each side measures:

- `gpuPassIssueMs` (was `gpuPassMs`): raster-thread time issuing passes.
- `gpuPassExecMs`: GPU-side execution time. Absent, not 0, when the context
  has no timer queries. Window rates `gpuPassIssueMsPerFrame` /
  `gpuPassExecMsPerFrame`; per-target `issueMs` / `execMs` in
  get_gpu_resources (was `passMs`).

Verified on Linux (Mesa) via the control API with gpu-particles: 606 passes,
issue 144 ms, exec 35 ms, per-target execMs attributed to the one target.

Open follow-ups, agreed when picking this up: stage 2 frame-level GPU time
(one query around Impeller's draw, `gpuFrameExecMs`), which is what explains
`fenceTimeouts` on apps with no passes; stage 3 the exec figure on the stats
overlay.

## Landed 2026-08-23: stage 2, frame execution time

The same timer wraps `draw_to_window` in `frame()` (Impeller's display list
plus the window-shader composite; not the pass flush before it, not the
present), attributed as `Timed::Frame` and reported as `gpuFrameExecMs`
(cumulative) and `gpuFrameExecMsPerFrame` in the window. Per frame it is
the figure to hold against `periodMs`: an app with no passes at all can be
GPU-bound, and this is the counter that says so where `fenceTimeouts` only
says that it already happened.

Verified on Linux (Mesa) with frame-animation: 300 frames in the window,
0.54 ms GPU per frame, pass counters at 0, both PerFrame rates present.

## Landed 2026-08-23: stage 3, the HUD line

The stats overlay shows `GPU n%`: window draw plus pass execution per frame
over the last one-second sample window (lattice `Stats::record_gpu`, diffed
at each sample), as a share of the smoothed frame period like the phase
lines above it. It is the one HUD figure that is not JS-thread work, so it
does not sum with them; near 100% the GPU is the bottleneck whatever the
phases say. Hidden when the context has no timer queries. Verified on Linux
with frame-animation under `srt run --stats`: `GPU 3%` (0.54 of 16.7 ms).
