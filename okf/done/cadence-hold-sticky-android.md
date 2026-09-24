---
title: The cadence hold never steps down on Android and reads the held interval as GPU time
description: Fixed 2026-09-23: the frame's GPU term from the EGL frame timestamps started at the frame's first GPU command, so under a hold the buffer dequeue's wait was charged as GPU time (43-51 ms against 25-32 ms of work) and the step-down prediction never fit; it now starts at the instant the swap queued the buffer, the census's own queue column, and the tablet reads 23.7 ms against a census span of 24.3 ms. The idle reset landed 2026-09-22.
created: 2026-09-22
completed: 2026-09-23
---

# The cadence hold never steps down on Android and reads the held interval as GPU time

## Symptom

demo/notes on the Galaxy Tab A7: once an add/remove animation has pushed
the hold to 3, `cadenceHold` stays 3 - through seconds of idle, and into
the next animation - and get_stats' `gpuFrameExecMsPerFrame` reads 43-51
ms for every animated window. After a reload (which resets the
controller) the same scene animates with the hold at 1 when its frames
fit; measured from the compositor, the frames that read as 43-51 ms of
GPU time were 25-32 ms of frameReady-minus-queue, and in one
configuration 21 ms (one refresh would have needed 33).

Seen 2026-09-21/22 across ~15 configurations of the demo: in every one
where the hold had reached 3, the GPU figure sat 15-20 ms above the
compositor's span and at or just below the held interval.

## Why it matters

- The hold cannot step down: `on_present` predicts a fit from
  `start_offset + cpu + gpu + DOWN_MARGIN` against `(hold - 1) * period`,
  and with `gpu` inflated to the interval the prediction never fits, so
  the app is stuck at 20 fps until a reload or restart.
- The figure is also what agents and humans read to size GPU work; here
  it was useless for attribution (every experiment in
  rounded-clip-cost-android.md had to fall back to `dumpsys
  SurfaceFlinger --latency`).

## Suspected cause

frame_timestamps.rs charges `complete - max(begin, previous complete)`,
with `begin` stamped at the frame's first GPU command. Under a held swap
interval the buffer dequeue blocks inside that span (the raster thread
begins the frame, then waits for a buffer that the hold releases a slot
later), so the wait lands in the GPU term. The doc comment says the
timer-query path had exactly this problem and the EGL timestamps were
adopted to fix it; on this device the timestamps path shows it too.

## What done looks like

- The GPU term excludes the dequeue wait: stamp `begin` after the buffer
  is acquired, or use the compositor's own queue-to-ready span (which is
  what SurfaceFlinger reports and what matched the work here).
- Done 2026-09-22: the hold returns to 1 on the first present after an
  idle gap of IDLE_RESET_MS (500 ms, cadence.rs), so a new interaction
  starts unheld and the measured rule alone decides; tested in
  tests/cadence.rs (an_idle_gap_resets_the_hold). The inflated GPU term
  itself, which keeps a raised hold from stepping down within one
  interaction, stays open.
- get_stats reports the compositor's queue-to-ready span beside the
  runtime's own figure on Android, so the two can be compared in place.

## Fixed (2026-09-23)

The suspected cause was the cause. `alloy/src/frame_timestamps.rs` asks
the stack for the frame's queue instant beside its rendering-complete
time (`EGL_REQUESTED_PRESENT_TIME_ANDROID`, the queue time when no
presentation time is requested) and charges `complete - max(queued,
previous complete)`; the first command stays the floor only on a surface
that does not report the queue instant (the probe logs which floor is in
use). The third bullet above is met by construction rather than by a
second field: the runtime's figure now is the compositor's queue-to-ready
span, measured by `srt android --census` (the census tool landed in the
same batch).

Measured on the SM-T500 with a sliding-panes probe (ten rounded panes,
five paragraphs, held at 3 because its work genuinely needs three slots,
44 ms mean): `gpuFrameExecMsPerFrame` 23.7 ms beside the census's
frameReady-minus-queue p50 24.3 ms over the same slide, where the old
floor read 43-51 ms. With an honest GPU term the step-down prediction
sees the real slot use; a workload that crosses back below a boundary
mid-interaction was not in the probe, so that step itself was not
observed on device (the controller's rule is covered by
alloy/src/tests/cadence.rs).
