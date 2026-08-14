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
