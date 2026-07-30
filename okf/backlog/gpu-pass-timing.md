---
type: backlog-item
title: Time the GPU pass work
description: Shader and pipeline passes execute in the raster command loop where nothing is timed, so a client can be tens of seconds per frame while the engine reports a healthy 40ms draw; per-pass duration is the one counter the 2026-07-27 GPU investigation still lacked.
status: done
timestamp: 2026-07-27T00:00:00Z
---

# Time the GPU pass work

Stage 1 landed 2026-07-30: raster counters consolidated into one shared
`RasterStats` (alloy/src/raster.rs), each `flush_dirty` target render counted
and wall-timed, surfaced live in get_stats as `gpuPasses` / `gpuPassMs`
(cumulative; diff two queries for a rate). The ms figure is raster-thread
occupancy issuing the passes, not GPU-side duration (GL is async; true GPU
time would need EXT_disjoint_timer_query).

Stage 2 landed 2026-07-30: per-target attribution. Each ShaderTexture keeps
cumulative pass count + micros (Cell fields, raster-thread only, survive
resize, die with the target), reported through the Resources RPC as
`passes` / `passMs` on every get_gpu_resources pipeline entry. Caveat: the
RPC queues behind a wedged raster thread, so attribution is for
normal-operation "which target is expensive" - the stage-1 aggregates are the
wedge-proof signal.

Stage 3 landed 2026-07-30: `rasterCmdMs` in get_stats, cumulative wall time
executing non-Frame commands (uploads, readbacks, offscreen rasterizations,
compiles, param writes, and the pass flushes those commands trigger). Frame
commands are deliberately excluded: their phases are already timed (frameMs),
and their present blocks on vsync by design, so including them would read as
busy on a perfectly healthy app.

All three counters are runtime-unverified as of 2026-07-30; the validation
pass this note suggested (Mali TV vs Adreno tablet vs desktop) is still worth
doing when a shader-target app is next on a device.

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
