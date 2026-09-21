---
title: Measure the frame's GPU time on Android from EGL frame timestamps
description: On the SM-T500 the TIME_ELAPSED query around the window draw read the frame interval (32 ms at a two-refresh cadence, 46-52 at three) for a frame whose GPU work is 15 ms, so the cadence hold's step-down could never pass and any hold above 1 was permanent; done means the frame's GPU term comes from the compositor stack's per-frame timestamps on Android, and the tablet runs the reporting app's slide at hold 1.
created: 2026-09-21
completed: 2026-09-21
---

# Measure the frame's GPU time on Android from EGL frame timestamps

## Symptom

An app whose slide frames cost 7 ms of CPU (srt-ui 1, srt-raster 5, swap
1.3, no throttle wait) and about 15 ms of GPU ran at 20 fps on the SM-T500
(Adreno 610, VsyncLocked, hold Auto), with 43 ms of nothing between
frames. `cadenceHold` read 3 and never stepped down. The controller's log
at startup: `cadence hold: 1 -> 3 refreshes (interval 4, work 45.7ms)`,
with the recent presents' `cpu+gpu` at `12.5+32.3`, `0.0+45.7`,
`0.0+45.7`. Seen on the [[quartz-heron]] app after its 4f fix; the same
frame on the desktop client (hold policy Off) was fine.

Measured: SurfaceFlinger's `--latency` frameReady minus the app's
`queueBuffer` (clocks aligned on the VSYNC-sf grid) gave 15.5 ms median
GPU per slide frame, 56 ms on the add frame.

## Cause

`FrameOutput::Presented.gpu_micros` was the `TIME_ELAPSED` query issued
around `draw_to_window` (`alloy/src/raster/frame.rs`, harvested in
`resources.rs::harvest_pass_timings`). On this driver the span reads the
frame interval, not the work: 32.3 ms at a two-refresh cadence, 45.7 and
52 at three (the plan for [cadence-hold] met the same class of reading on
the 50 Hz TV: 20 ms for a 7 ms frame, and tolerated it by admitting the
pipelined estimate only near a measured long interval). The step down in
`alloy/src/cadence.rs` predicts from `offset + cpu + gpu + 2 ms` fitting
`hold - 1` slots; a GPU term equal to the current interval never fits one
slot fewer, so a hold reached once (the mount frames took it to 3) was
permanent. The attribution self-test (`okf/done/gpu-timer-attribution.md`)
does not catch this: its probe renders offscreen, and this span is the
window surface, where the driver's buffer handling sits inside the timed
commands. On this device the query path also retired exactly one frame
query per session and no more, which is why `gpuFrameExecMs` sat at a
constant 52.

## Done

`alloy/src/frame_timestamps.rs`: on Android the frame's GPU term is the
compositor stack's own measurement, `EGL_ANDROID_get_frame_timestamps`'
`EGL_RENDERING_COMPLETE_TIME_ANDROID` (the buffer's GPU fence,
SurfaceFlinger's frameReady). The term charged to a frame is

    gpu = rendering_complete - max(frame_begin, previous_rendering_complete)

with `frame_begin` a CLOCK_MONOTONIC stamp taken at the top of the raster
frame verb, ahead of the offscreen pass flush (on a tiler the passes
execute in the same submission as the window draw), and the floor at the
previous frame's completion the Frame Pacing library's model: a frame
queued behind the previous frame's GPU work is charged for its own
execution only.

- The three entry points the safe khronos-egl API lacks
  (`eglGetNextFrameIdANDROID`, `eglGetFrameTimestampSupportedANDROID`,
  `eglGetFrameTimestampsANDROID`) are resolved through `get_proc_address`
  into signatures pinned to the extension's ABI, in that one module; the
  display and draw surface come from khronos-egl as in
  `current_surface_samples`. Tokens are from `EGL/eglext.h`:
  rendering-complete is 0x3435 (the compositor-timing names 0x3431-0x3433
  sit between the enable and the frame timestamps).
- Enabled lazily on the first frame after a window-surface bind
  (`EGL_TIMESTAMPS_ANDROID`), forgotten on `rebind_window_surface`; per
  frame the id is asked before the swap and queued on success; the ring
  is polled after each present and the latest completed span becomes
  `last_frame_gpu_micros` (the `Presented` notification already carries it
  a frame or two behind) and the cumulative `frame_exec_micros`.
- Armed, the frame's `Timed::Frame` query is not issued; `PassTimer::hold`
  keeps the frame span so the offscreen rasters inside it stay untimed as
  they were under the query (timed, their readings on this driver were
  junk and inflated `pass_exec_micros` by seconds), and retired pass
  micros are not folded into the frame figure (the span covers them).
- A stack that never answers (8 frames gone or pushed out without a
  completion, or a failing next-frame-id call) is given up on with a log
  line naming the EGL error, and the timer query takes the frame back.
  Not Android, extension absent or support false: the query path,
  unchanged.
- Pure parts (`sweep`, `span_micros`) tested in
  alloy/src/tests/frame_timestamps.rs.

Verified on the SM-T500 with the reporting app: per-frame spans of 14-17
ms in a 60 fps stream (the fence census said 15.5), 43-52 ms for an
isolated frame from a cold GPU; the hold sits at 1, and a slide presents
31 of 33 intervals at one refresh with 0 missed presents, against 20 fps
before. Startup on that device still rises to 3 on the mount frames and
steps back to 1 within about two seconds of presents. The vsync-cadence
probe's table was not rerun; the desktop path is the unchanged query path
(alloy tests green).

Follow-on, same plumbing: `EGL_DISPLAY_PRESENT_TIME_ANDROID` is the
measured presentation timestamp the frame-timing design lists as modeled
today ([presentation-feedback]).

## Related

- `okf/design/frame-timing.md` D9 (this record is listed there).
- okf/plans/cadence-hold.md, the controller and the TV-box reading of the
  same artifact.
- okf/done/gpu-timer-attribution.md, the offscreen self-test this span
  escapes.
- okf/backlog/android-main-loop-rarely-sleeps.md, the second finding of
  the same measurement (SDLThread at 98% while idle), still open.

[cadence-hold]: ../plans/cadence-hold.md
[presentation-feedback]: ../backlog/presentation-feedback.md
