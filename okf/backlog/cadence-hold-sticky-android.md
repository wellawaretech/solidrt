---
title: The cadence hold never steps down on Android and reads the held interval as GPU time
description: On the SM-T500 get_stats' gpuFrameExecMsPerFrame tracked the held interval (43-51 ms at a hold of 3) while SurfaceFlinger's frameReady-minus-queue spans were 25-32 ms, so the step-down prediction (offset + cpu + gpu + margin must fit the shorter slot) never passes, the hold stays at 3 through idle, and every later animation starts at 20 fps even when its frames would fit one refresh. Reload resets it.
created: 2026-09-22
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
