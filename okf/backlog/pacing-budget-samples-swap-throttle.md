---
title: The pacing budget measures the swap's throttle wait as pipeline cost
description: On Android the vsync signal delay sits at its 8 ms floor for a 0.1 ms frame, because the emission-to-present sample the budget takes includes eglSwapBuffers blocking behind the previous frame's GPU work; the signal could fire ~6 ms later, which is that much input-to-glass latency given away.
created: 2026-09-12
---

# The pacing budget measures the swap's throttle wait as pipeline cost

`PacingBudget` (`alloy/src/vsync.rs`) picks how late after the vsync the
frame signal fires from the worst recent emission-to-present-return
duration: as late as the pipeline allows, so the frame samples the freshest
input and waits least in the queue. On Android the present-return is
`eglSwapBuffers` returning, and that swap blocks 3 to 19 ms in libgui's EGL
production throttle (see [[android-vsync-release-chain]]): the sample reads
12 to 19 ms for a frame whose build, draw and queue take under 4 ms, and
the delay clamps to the 8 ms floor instead of the ~14 ms the real cost
would allow.

## Done looks like

On the SM-T500 the 1/s `pacing: signal delay` line reads ~14 ms for the
cadence probe, with the census still all single intervals.

## What it involves

Sampling the cost up to the point the buffer is queued rather than the
swap's return - `EGL_ANDROID_get_frame_timestamps` reports the queue time
per frame, or the sample could close when the raster thread enters the swap
plus a measured constant. The banked-signal path must keep working: a
later signal narrows the present-return's margin against it.
