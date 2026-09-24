---
title: Pass counters are whole-frame, so no one target can be blamed
description: Landed 2026-09-23: the raster thread publishes every target's cumulative pass counters once per presented frame (a shared snapshot the frame records carry), and get_stats' window reports `targets` - per label, passes, issue and exec time and vertices per presented frame, node shaders under id 0 - so which of a scene, its views, a shadow atlas and a probe is the expensive one is one read.
created: 2026-09-08
completed: 2026-09-23
---

# Pass counters are whole-frame, so no one target can be blamed

## Symptom

`gpuPassesPerFrame: 20.8` is a true number that names nothing. A 3d app
draws its scene target, a shadow atlas (one tile pass per map), a
reflection probe (six faces plus its prefilter chain) and every `View3d`,
so a figure that climbed says only that something did - and the next step
is to comment targets out one at a time.

The pieces are already there and just are not joined:

- Every draw target takes a `label`, and `/gpu?label=` filters the
  resource inventory by it, so labels are the established identity that
  survives a reload.
- Passes, issue time and GPU-side exec time are all counted per pass on
  the raster thread; they are only summed before they are reported.

The reporter's framing: `get_stats` gives excellent whole-frame GPU
figures, and the labels exist, so surfacing per-label pass counts closes
the loop.

## Done looks like

Per-label pass attribution in `get_stats`: for each target that drew in
the window, its passes per frame and its share of issue and exec time,
alongside the existing whole-frame figures. The exec half inherits the
absence rule the frame figures already have (a client whose driver failed
the startup attribution self-test reports exec as absent, not zero).

Then the answer to "which target is expensive" is one read, and the
shadow atlas vs probe vs view question stops needing an ablation.

Involves: the raster-thread pass counters in alloy, the stats payload in
`lattice/src/go/connection.rs`, the `get_stats` description in the MCP
surface, and the stats section of `packages/cli/agents/debugging.md`.

Related: [gpu-timer-query-pass-timing](../done/gpu-timer-query-pass-timing.md)
made the exec figures trustworthy in the first place, and
[gpu-system-attribution](../backlog/gpu-system-attribution.md) is the other axis
(which PROCESS, not which target).

## Landed (2026-09-23)

As shaped. The per-target counters already lived on each `ShaderTexture`
(`pass_stats()`), readable only by the raster thread; `RasterState::
publish_target_counters` (alloy/src/raster/frame.rs) now snapshots them
once per presented frame into `RasterStats::targets` as an `Arc<Vec<
TargetCounters>>` - one row per target that has rendered, labelled like the
GPU inventory (a sub-target by its region's label), plus the node shader
passes under id 0, which render through scratch framebuffers and keep
their own row on the raster state. Each `FrameRecord` clones the Arc;
`FrameHistory::summarize` differences a window's first and last records
per target (a target created mid-window counts from zero) into
`TargetRates`, reported as `window.targets` with `passesPerFrame`,
`gpuPassIssueMsPerFrame`, `gpuPassExecMsPerFrame` (absent without timer
queries, the frame figures' rule) and `verticesPerFrame`
([gpu-vertex-fill-attribution](gpu-vertex-fill-attribution.md) landed
with it). `FrameRecord` stopped being `Copy` for the Arc. Unit-tested over
a synthetic scene/atlas/probe/view window; verified on the linux client
with a shader-target probe (`probe-waves`: 1 pass, 3 vertices, 0.01 ms per
frame).
