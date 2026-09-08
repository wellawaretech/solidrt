---
title: Pass counters are whole-frame, so no one target can be blamed
description: get_stats reports gpuPassesPerFrame and gpuPassExecMsPerFrame for the whole client, but an app drawing a scene, two views, a shadow atlas and a probe has five candidates and no way to tell which one is expensive; every target already carries a label.
created: 2026-09-08
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
[gpu-system-attribution](gpu-system-attribution.md) is the other axis
(which PROCESS, not which target).
