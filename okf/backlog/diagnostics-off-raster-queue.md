---
title: Diagnostics queue behind the thing they diagnose
description: get_gpu_resources queues behind the raster backlog it exists to explain, and get_stats/get_snapshot need a JS-thread slice, so they time out on a busy (healthy) app with a message that says "wedged"; serve inventory and stats off published state, and name the real timeout.
created: 2026-07-27
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

## The JS-thread flavor (2026-08-02)

From the wasm game-port demo feedback: get_stats and get_snapshot both
returned the same "JS thread busy or app wedged?" timeout while the app ran
perfectly - logs flowing, 35 fps steady on three clients. The JS thread was
simply saturated (cpuPct 90.9) and these queries need a slice of it. That is
the worst failure mode for exactly the class of app most likely to need
profiling: the message reads as "your app is wedged" when the truth is "your
app is busy", and the problem disappears the moment you profile something
cheap. get_logs stayed reliable throughout because it is pushed, not polled.

So the asymmetry above has two flavors - raster-thread and JS-thread - and
both halves of the fix generalize: serve get_stats from runtime-side
published state without touching the JS thread, and make the timeout message
distinguish busy from wedged; the runtime knows the difference, since it is
still presenting frames.

Status bumped deferred -> open (2026-08-02): the original rationale ("only
bites when a client is already wedged") no longer holds - a healthy,
CPU-saturated app is the common profiling case.
