---
type: backlog-item
title: Time the GPU pass work
description: Shader and pipeline passes execute in the raster command loop where nothing is timed, so a client can be tens of seconds per frame while the engine reports a healthy 40ms draw; per-pass duration is the one counter the 2026-07-27 GPU investigation still lacked.
status: open
timestamp: 2026-07-27T00:00:00Z
---

# Time the GPU pass work

Split out of idle-tick-gpu-backlog-runaway.md, where this was the costliest
diagnostic gap: it is what turned a one-look diagnosis into an afternoon.

`RasterState::frame` (alloy/src/raster.rs) times its three phases - fence wait,
draw, present - and those feed `[alloy] slow frame` and `get_stats`' `frameMs`.
Everything else the raster thread does runs in the command loop and is timed
nowhere. `UpdateShaderParams` / `UpdateShaderTextures` re-render a whole
pipeline target; `RasterizeDlInto`, readbacks and texture uploads all cost real
GPU time. None of it appears in any counter.

The concrete failure this produced: an app at **50 seconds per presented
frame** logged

```
[alloy] slow frame: fence wait 0.0ms, draw 40.3ms, present 34.1ms
```

and `get_stats` reported `fps: 0, frameMs: 22.6`. Both were truthful about
`frame()` and blind to the 49.9 s of queued pipeline renders next door. The
only way to see reality was reading present timestamps out of
`dumpsys SurfaceFlinger --latency` on the device.

## What to add

Per-pass wall time on the raster thread, aggregated into the live stats the
same way `rasterQueue` / `idleTicks` / `fenceTimeouts` already are (read from
the alloy Context at query time, not the frame-latched snapshot - it goes stale
exactly when the raster thread wedges). Minimum useful shape:

- total ms spent in pass execution since the last query, or a rolling mean
- a count of passes executed, so redundant renders are visible as
  passes-per-presented-frame > 1
- ideally per-target attribution, since an app with several pipelines wants to
  know which one is expensive

Note that a pass count alone would have caught the original bug: it was
~900 pipeline renders per presented frame.

## Why it is worth doing now

Each counter added during that session converted a hunt into a glance -
`rasterQueue` showed the backlog, `idleTicks` proved the gate was quiet,
`fenceTimeouts` showed the depth-1 fence expiring on every frame. Pass timing
is the last one missing, and there are now two Android devices with an ~8x
spread on identical work (Mali-T860 TV at 120 ms/frame vs Adreno 610 tablet
vsync-locked at 16.7 ms) to validate the numbers against, plus desktop.

Related: gpu-usage-attribution.md (the "is the client burning GPU and on what"
question this partly answers), production-diagnostics-surface.md,
diagnostics-off-raster-queue.md.
