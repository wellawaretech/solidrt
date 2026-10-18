---
title: The sort thread lands on a slow core, so a splat re-sort lags more than it needs to
description: On the Pixel 7 the order worker sorts 100k keys in about 30 ms where the UI thread did it in 15, and 300k takes the same 30 ms; the thread runs at normal priority beside two display-priority frame threads and is scheduled onto a little core at a low clock, so the order lags the camera by two to four times what a big core would give.
created: 2026-10-17
completed: 2026-10-18
---

# The sort thread lands on a slow core, so a splat re-sort lags more than it needs to

## Symptom

okf/done/splat-sort-spike.md: with the re-sort moved to its own thread
the frame no longer stalls, but the sort itself got slower at small
sizes (Pixel 7, 100k keys: ~30 ms on the thread, 15 ms when it ran in
the flush) and stops scaling (300k: the same ~30 ms). The frame threads
take display priority (`sdl_utils::frame_thread_priority`); the sort
thread is an ordinary thread, which the scheduler parks on a little
core, likely at the governor's floor with the cadence hold off. The
sort's duration is the order's lag behind the camera (53 ms median at
1M), so the placement is a latency cost, not a frame cost.

## Shape

- Measure with the thread at SDL's high priority (`SDL_SetThreadPriority`,
  which on Android raises the nice value) and, if that does not move
  it, with an explicit big-core affinity through the SDL wrapper layer
  (`alloy/src/sdl_utils.rs`, where SDL has no API) - the frame threads
  must keep their precedence either way.
- Or shrink the work: the key pass projects three floats per record
  and the scatter touches 4 MB of permutation twice per pass; a 16-bit
  key over the cloud's bounds halves the passes at a precision the
  field ships (see the done record's option list, declined for quality
  until measured by a frame diff).

## Measured 2026-10-18

Pixel 7 (4 little cores at capacity 158, 2 mid at 763, 2 big at 1024),
release, cadence hold off, the 1M train cloud on
probes/splat-mesh-probe.tsx, orbit at 0.3 rad/s; `orderSortMs` from
/stats sampled every 200 ms over 15 s per rung (median / max of the
landed jobs), the UI thread's p95 from the 10 s window. The variants came
from one build with a temporary env switch, since removed.

| placement of the sort thread | 100k | 300k | 1M |
|---|---|---|---|
| none (as spawned: nice -4 inherited from the UI thread, cores 0-7) | 28.1 / 39.5 | 17.8 / 29.0 | 41.8 / 59.3 |
| nice -2 alone | 26.8 / 40.8 | 17.9 / 28.8 | 42.1 / 61.4 |
| nice -2 + pin to cores 4-7, run 1 | 14.8 / 16.2 | 16.9 / 31.5 | 41.6 / 58.8 |
| nice -2 + pin to cores 4-7, run 2 | 14.8 / 16.1 | 18.9 / 29.8 | 42.8 / 60.3 |
| nice -2 + pin to cores 4-7, run 3 | 14.6 / 16.5 | 18.2 / 29.3 | 43.2 / 59.5 |
| nice -2 + pin to cores 6-7 (the two big) | 9.7 / 13.2 | 20.8 / 29.2 | 44.6 / 48.4 |

The UI thread's p95 is 2.8-3.3 ms in every run: no placement costs a
frame. (An alarm was dismissed on the phone during the nice-alone 100k
rung; its read sits beside the unplaced one, so it was not rerun.)

- SDL's own `SDL_THREAD_PRIORITY_HIGH` was not tried: on Android and
  Linux it is nice -10 (`SDL_threadprio.c`), above the raster thread's
  -8, so it would invert the frame threads' precedence.
- Nice moves nothing. The thread inherits the UI thread's -4 at spawn
  anyway, so a nice between the frame tier and the default is a step
  down, kept only for the precedence it states.
- A utilization floor (`sched_setattr` with a util_min clamp, the hint an
  energy-aware scheduler reads to put a low-utilization burst on a big
  core at its top clock) is refused on Android by policy: EPERM for an
  unprivileged caller, with and without the keep-policy flags, verified
  with a C probe on the phone. Android's kernel carries the downstream
  commit "sched: Make uclamp changes depend on CAP_SYS_NICE", which no
  app has. Desktop Linux (Arch, 7.1) accepts the same call.
- The pin is what moves the sort: 100k is back at its in-flush 15 ms
  (1.9x), 300k scales above it. 300k and 1M are memory-bound and the same
  under every placement; 1M reads 42 ms on every variant where yesterday's
  record said 53, a day-to-day (thermal) difference, not the placement.
- The two-big-core pin wins 5 ms at 100k, loses at 300k and 1M medians,
  and shares the two cores the frame threads prefer: declined. The pin to
  every core above the smallest capacity is the shipped shape.
- A pin does not survive a cpuset move: HOME puts the app in the
  background group (cores 0-3, the mask rewritten to it), and the return
  to top-app rewrites it to 0-7, unpinned for good. So the worker
  re-asserts the pin before every job (one syscall before a sort of
  milliseconds); while the app sits in a group without the pinned cores
  the re-pin fails and the next job in the top-app group restores it.
  Verified on the phone with the final build: pinned 4-7 at start,
  0-3 in the background group after HOME, 0-7 right after the return,
  4-7 again once the next sort ran; the ladder on that build reads
  15.2 / 16.0, 17.2 / 39.4 and 42.6 / 52.5 ms with the UI p95 at 2.9-3.0.

## Outcome

- `alloy/src/sdl_utils.rs`: `latency_worker_thread_priority()`, the
  placement of a thread whose duration is lag but never a frame's
  deadline: `thread_nice(-2)`, then `thread_util_min(1024)`, and where the
  floor is refused a pin to `performance_cores()` (the cores above the
  smallest `cpu_capacity`) through `pin_thread_to`. It returns a
  `LatencyPlacement` whose `reassert` re-applies the pin. macOS (QoS
  already places a default thread on the performance cores) and Windows
  (no hint) are no-ops. `frame_thread_priority` goes through
  `thread_nice`.
- `alloy/src/gpu/order_worker.rs`: the `sol-order` thread takes the
  placement at start and re-asserts it before each job.
- Not done on purpose: the 16-bit key (a cheaper sort, the option list in
  okf/done/splat-sort-spike.md); the sort at 300k and up is memory-bound
  under every placement, so the next gain is in the work, not the core.
