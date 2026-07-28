---
type: backlog-item
title: Diagnostics queue behind the thing they diagnose
description: get_gpu_resources is a raster command, so it sits at the back of the same backlog it is meant to explain and times out exactly when the client is wedged; get_stats survived the same failure only because it is served elsewhere.
status: deferred
timestamp: 2026-07-27T00:00:00Z
---

# Diagnostics queue behind the thing they diagnose

Split out of idle-tick-gpu-backlog-runaway.md.

`get_gpu_resources` is dispatched as `RasterCmd::Resources` and answered by the
raster thread's command loop. So when the raster thread is saturated, the query
joins the back of the queue it exists to explain. During the 2026-07-27 session
it failed with

```
Query timed out: the client is connected but did not answer (JS thread busy or app wedged?)
```

on every attempt against the collapsed client - the single moment its output
(pipeline draw counts, target sizes, last-applied uniforms) would have been most
useful. `get_stats` kept answering throughout because it is served off a
different path, which is why the diagnosis leaned entirely on it plus
platform-level tools.

The error message is also misleading in this state: it suggests the JS thread,
which was in fact running fine at 49.7 Hz. Only the raster thread was wedged.

## Shape of the fix

Two independent halves, either useful alone:

1. Serve GPU inventory from state the raster thread publishes rather than from a
   round-trip through its command queue - a snapshot updated as resources are
   created/destroyed/resized, readable without the raster thread's cooperation.
   Texture *contents* (`get_texture`, `get_buffer`) genuinely need the GL
   context and cannot work this way; the inventory does not.
2. Failing that, distinguish the timeouts. A raster-queue timeout should say so,
   and report the queue depth, instead of blaming the JS thread. That alone
   would have pointed straight at the answer.

## Why deferred

Only bites when a client is already wedged, and the runaway that wedged clients
is fixed (see parent item). Kept because the diagnostic asymmetry is real and
will resurface with any future expensive-pass workload, and because half 2 is
nearly free.

Related: mcp-gpu-resource-inspection.md, production-diagnostics-surface.md,
gpu-pass-timing.md, reload-drain-raster-queue.md.
