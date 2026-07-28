---
type: backlog-item
title: Idle tick runs away when the raster thread falls behind
description: The idle-tick gate read pending_presents == 0 as "GPU idle", but it was equally true when the raster thread was too far behind to have returned a frame, closing a positive feedback loop that diverged without bound; fixed and TV-verified, with the adjacent findings split into their own items.
status: done
timestamp: 2026-07-27T00:00:00Z
---

# Idle tick runs away when the raster thread falls behind

## Status 2026-07-27: candidates 1, 2, 4 implemented and verified on the TV

- Candidate 1: `RasterSender` (raster.rs) pairs the command channel with a
  shared queue-depth counter (increment on send, decrement as each command
  finishes executing); the idle-tick gate in app.rs now also requires depth 0,
  and resets its deadline on suppression so the main loop sleeps instead of
  spinning through a backlog.
- Candidate 2: `UpdateShaderParams` load-sheds per shader id within a batch,
  folding shed params lists into the surviving render (params can be partial);
  never sheds across a frame that draws.
- Candidate 4: `get_stats` reports `rasterQueue` and `idleTicks`, read live
  from the alloy Context at query time (not the frame-latched snapshot, which
  goes stale exactly when the raster thread wedges).

Verified on the same TV, runtime 0.0.38-20-g8348ad6-dirty, against the
original 233,600-vertex per-vsync-params app that produced everything below:

| | before | after |
|---|---|---|
| 233,600 vertices, params per vsync | unbounded doubling to 50 s/frame | **120 ms flat, 8.3 fps, 125 frames, max 140 ms** |
| JS ticks vs presents | 49.7 Hz vs 0.054 Hz (~900:1) | **7.7 Hz vs 8.3 Hz (~1:1)** |
| 60,000 vertices (the reduced config the bug forced) | 160 ms / 6.3 fps | **100 ms / 10.0 fps** |
| `get_stats` during the collapse | `fps: 0`, `frameMs: 22.6` | `fps: 8`, `frameMs: 125` (matches wall clock) |

`rasterQueue` reads 4 under load and `idleTicks` is identical across two
samples 52 s apart - the gate is suppressing exactly as designed. The
frame-time win is larger than the runaway fix alone: the params load-shed
also removed ~8 redundant renders per presented frame at the sustainable
counts, which is where the 160 -> 100 ms on the reduced config comes from,
and it is what lets full density run at 120 ms where a third of it used to
cost 160 ms.

Note the new floor: 100 ms is present pacing (five 50 Hz vsyncs), measured
identical for a 20k-vertex trivial scene, so workloads now sit against
candidate 3 / the present-fence finding rather than against this bug.

Candidate 3, observability half, landed 2026-07-27 (unverified):
`await_present_fence` now checks the `client_wait_sync` status; a
`TIMEOUT_EXPIRED` increments a counter surfaced live as `fenceTimeouts` in
`get_stats` and warns at 1/s ("GPU over budget, pacing lost this frame");
a wait failure warns with the raw status. The behavior half - allowing one
frame in flight by keeping a two-deep fence queue, to overlap draw with the
compositor's present latency - stays open pending measurement: desktop fence
waits measured 8-10 ms during the frame-pacing work, but the added
input-to-photon frame must be costed there before the depth change (or made
adaptive on observed wait times).

Measured 2026-07-27, same TV, flower app at full density (live `get_stats`
sampling over a timed 60 s window):

| | fenceTimeouts | interpretation |
|---|---|---|
| desktop (linux), idle + light use | 0, exactly | healthy GPU never hits the cap |
| TV, idle app | flat (1045 -> 1045 over 35 s) | no false positives from present pacing at idle |
| TV, 233k steady state (7-8 fps, frameMs ~140) | +549 over the window, ~8-9/s | **every presented frame times out** |

So at the TV's steady state the depth-1 fence provided no pacing at all - it
expired at the 100 ms cap on every single frame and we drew anyway - while
costing a 100 ms stall per frame: the ~140 ms frame period was ~100 ms
capped wait plus ~40 ms of everything else. `idleTicks` also stayed exactly
flat under full load - the idle-tick gate is completely quiet during
saturation.

Candidate 3, behavior half, landed and TV-verified 2026-07-27:
`PRESENT_FENCE_DEPTH = 2`, unconditional - `present_fences` is a two-deep
queue and the draw blocks only on the fence from two presents back, so the
next draw overlaps the compositor's present latency while ahead-of-glass
depth stays capped below the driver queue's 2-3. Decision: ship
unconditional; the desktop input-to-photon cost (one more frame ahead where
fences signal early) is accepted until actually observed - the adaptive
fallback is specced in adaptive-present-fence-depth.md.

Measured, same flower app, timed 60 s windows:

| | depth 1 | depth 2 |
|---|---|---|
| TV fenceTimeouts | ~8-9/s (every frame) | **0, exactly, under full load** |
| TV frameMs | ~140 | ~121-130, fps 8 |
| desktop (same app) | - | 60 fps locked, fenceTimeouts 0, rasterQueue 1 |

One expectation corrected by the measurement: the depth-1 "100 ms capped
wait + 40 ms rest" decomposition did NOT mean 40 ms was reachable - the
capped wait overlapped real GPU execution, and the Mali's actual throughput
at 233k density is ~120-130 ms/frame, which is where depth 2 now sits. What
depth 2 bought is the ~10-15% serialization bubble, timeouts eliminated
(pacing is honest again: every fence now retires within its window), and
the structural fix: the previous "hard ~10 fps ceiling regardless of
content" from present latency should be gone for lighter scenes - worth a
one-off check with a small-vertex config on the TV, which would also close
the "present-fence pacing caps throughput" adjacent finding below. This
closes the candidates; the remaining adjacent findings stay open.

### Light-scene check 2026-07-27: the ceiling is NOT gone, and it was never the fence

Ran the flagged one-off on runtime 0.0.38-22-g4c7e487-dirty. Light configs on
the TV are unchanged from their pre-fence-work numbers:

| TV scene | before any fence work | depth 2 |
|---|---|---|
| 20k vertices, trivial vertex shader, 1 pass | 80-100 ms | **80 ms median, 12.4 fps** |
| 34,800-vertex flower | 100 ms / 10.0 fps | **100 ms / 10.0 fps** |
| 233,600-vertex flower | - | 120 ms / 8.3 fps |

`fenceTimeouts` is 0 in every one of these, so the fence work is doing
exactly what it claims - it just was not what capped light scenes. The stall
**moved rather than disappeared**, which the phase log shows outright for the
20k scene:

| | fence wait | draw | present |
|---|---|---|---|
| before fence work | 78 ms | 2 ms | 1 ms |
| depth 2 | **0.0 ms** | 6 ms | **80-85 ms** |

Same total, different owner: `eglSwapBuffers` itself blocks for 4-5 vsyncs
(80/100 ms alternating on a 20 ms panel). The depth-1 fence was absorbing the
compositor's back-pressure *ahead* of the swap; with depth 2 the swap absorbs
it directly. So there was no throughput to recover on light scenes - the fence
was downstream of the real constraint.

It is TV-specific, not general: identical binary and identical 20k scene, via
live `get_stats`.

| | fps | frameMs | rasterQueue | fenceTimeouts |
|---|---|---|---|---|
| desktop linux | 61 | 19.94 | 1 | 0 |
| Android TV | 12 | 89.97 | 3 | 0 |

The persistent `rasterQueue` 3 vs 1 is the same story from the other side: the
TV's raster thread sits ~3 commands deep on a trivial scene purely because
each swap takes four refresh periods to return.

So the "present-fence pacing caps throughput" adjacent finding below **stays
open and needs re-aiming**: it is not fence pacing, it is that the Android
SurfaceView's swap blocks ~4 vsyncs on this compositor. Next levers to look
at are EGL swap interval and the surface's buffer count / queue depth, none of
which alloy currently sets explicitly. Worth confirming on a second, newer
Android device before treating it as an engine problem rather than a
MediaTek-TV compositor one.

Source: Android TV debugging session 2026-07-27. Philips TPM171E (MediaTek
MT5891, ARM Mali-T860, GLES 3.2 driver r20p0, Android 8.0, armeabi-v7a,
1920x1080 at **50 Hz**), runtime 0.0.38-18-g4a72e81-dirty release. App was
`examples`-style: two `createPipeline` point-cloud passes (233,600 + 6,532
vertices) with one `params` write per pipeline per `onFrame`.

Symptom: the app starts at a steady ~290 ms/frame, holds for about five
seconds, then frame time **doubles every frame without bound** - measured
1320 -> 2380 -> 4200 -> 7480 -> 13120 ms on a freshly launched process, and
observed out to 50 s/frame if left alone. It never reaches a steady state and
it never recovers. `load`/`reload` does not clear it; only restarting the
client process does.

## What actually happens

The idle tick in `alloy/src/app.rs` fires on this condition:

```rust
if pending_presents == 0 && last_frame_signal.elapsed() >= tick_period {
  event_tx.send(AlloyEvent::Tick { frame, fps }).ok();
```

with the comment "an idle Tick keeps its per-frame logic running while the
GPU stays idle". `AlloyEvent::Tick` drives JS's per-frame work identically to
`FrameRendered` (`lattice/src/lib.rs`, both arms of the frame-signal match).

`pending_presents == 0` is true when the GPU is idle. It is *also* true when
the raster thread is so far behind that it has not handed back a frame yet -
the opposite condition, and indistinguishable at this gate. So while the
raster thread grinds through a queue of pipeline renders it sends no
`Presented`, `pending_presents` stays 0, and the main loop emits an idle Tick
every refresh period. Each Tick runs the app's `onFrame`, which writes
`params`, which becomes an `UpdateShaderParams` on the raster channel, which
is another full point-cloud render.

Backlog -> no presents -> uninterrupted 50 Hz ticks -> more backlog. The loop
gain is `(tick rate) x (per-pass GPU cost)`; above 1 it diverges, and the
measured ~1.9x per frame matches a ~38 ms pass against a 20 ms tick period.

Measured directly on the collapsed app, reading the app's own `onFrame`
counter through `call_debug` while sampling SurfaceFlinger present times:

| | |
|---|---|
| JS `onFrame` ticks | 497 -> 3036 over 51.0 s = **49.7 Hz** |
| screen presents, same window | one per **18.5 s** |

**~900 JS frame callbacks per presented frame.** There is no effective
backpressure from present to frame production.

Second, independent amplifier: `RasterCmd::UpdateShaderParams` in
`alloy/src/raster.rs` calls `shader.render()` for *every* command in a batch.
Thirty lines earlier the same loop load-sheds `RasterCmd::Frame` to the last
one in the batch via `rposition`. Params updates get no such treatment, so a
batch that accumulated N of them for one shader id runs the pass N times and
discards N-1 results. That is not the root cause - the loop above opens with
or without it - but it is what converts the open gate into ~900 wasted
233k-vertex renders per presented frame.

## Reproduction

Any `createPipeline` whose single pass costs more than one refresh period,
driven by a per-`onFrame` params write. On the Mali-T860 at 1080p the
threshold sits between 100k and 233k point-topology vertices:

| total vertices | frame time | |
|---|---|---|
| 34,800 | 100 ms | stable |
| 60,000 | 160 ms | stable |
| 100,000 | 380 ms | stable |
| 233,600 | - | **runaway, ~1.9x/frame, unbounded** |

Confirming the mechanism rather than the workload - same 233,600 vertices,
changing only the params write rate to every other vsync (halving the loop
gain to ~0.95):

| 233,600 vertices | result |
|---|---|
| params per vsync | runaway |
| params every 2nd vsync | **480 ms flat, 104 frames, no drift** |

Two controls worth recording, because they rule out the obvious suspects:
rendering the same 233,600 vertices into a **quarter-size target** (960x540)
collapses on an identical curve, and `gl_PointSize = 3.0` (9x the fill) costs
nothing measurable. The cost is per primitive; neither fill nor target size
is involved.

## Fix candidates, in preference order

1. **Gate the idle tick on real in-flight work.** Note that a frames-only
   counter is not enough: the backlog here is `UpdateShaderParams`, not
   `Frame`, so incrementing at `Context::submit` and decrementing on
   `Presented` would still read "idle" while the raster thread is saturated
   with pipeline work. The gate wants raster *queue depth* - an
   `AtomicUsize` bumped on every `Context::send` and decremented as each
   command is consumed - so the condition becomes "nothing queued and nothing
   in flight". Idle should mean idle.
2. **Coalesce `UpdateShaderParams` per shader id within a batch**, mirroring
   the `Frame` load-shed already sitting in that loop. Damage control rather
   than root fix, but it removes the redundant renders and would have kept
   this failure inside one order of magnitude instead of unbounded.
3. **Make `await_present_fence` honest.** It is the only backpressure in the
   pipeline and it ignores the `client_wait_sync` result, so a
   `PRESENT_FENCE_TIMEOUT_NS` (100 ms) timeout - which is exactly the "GPU is
   over budget" signal - is indistinguishable from a clean wait. At minimum
   it should be observable; treating a timeout as a reason to skip production
   would give a second line of defence.
4. **Expose queue depth / in-flight commands in `get_stats`.** See the
   diagnostics note below; this is the counter whose absence turned a
   one-look diagnosis into a day.

## Not established

Which link is broken is inferred, not proven. What is measured is 49.7 Hz of
JS frame callbacks against 0.054 Hz of presents; the idle-tick gate is the
only place in `app.rs` that can produce that, since both `FrameRendered`
arms are strictly one-per-`Presented`. Confirming it is a one-line log at
that branch, or a ticks-vs-FrameRendered counter in `get_stats`. Do that
before building the fix.

## Adjacent findings from the same session

All split out into their own items 2026-07-27; kept here as an index because
they were all found by the same investigation and each carries a slice of its
evidence.

- gpu-pass-timing.md [open] - shader/pipeline passes run in the raster command
  loop where nothing is timed, so this bug reported `draw 40.3ms` at 50 s per
  frame. The costliest diagnostic gap of the session and the last one still
  missing.
- device-perf-model-docs.md [open] - the scaffold's "GPU work is nearly free"
  guidance holds on desktop and mid-range mobile and is wrong by ~8x on
  TV-class hardware; the docs currently steer people toward exactly the app
  shape that tripped this item.
- android-surface-swap-latency.md [deferred] - the re-aimed
  "present-fence pacing caps throughput" finding: a real 4-5 vsync swap on the
  MediaTek TV, shown device-specific by a second Android device running the
  same binary vsync-locked at 60 fps.
- reload-drain-raster-queue.md [deferred] - a backed-up raster channel survives
  `load`/`reload`, so a wedged client cannot be recovered from the dev loop.
  Contaminated several measurements here before it was understood.
- diagnostics-off-raster-queue.md [deferred] - `get_gpu_resources` queues behind
  the backlog it would explain and times out precisely when wanted, blaming the
  JS thread which was running fine.
- android-dev-server-persistence.md [deferred] - the dev-server address only
  arrives as a launch-intent extra, so any relaunch outside the CLI starts into
  `apps/default` with no way back without adb.
