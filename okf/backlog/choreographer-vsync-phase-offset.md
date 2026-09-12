---
title: Correct the Choreographer vsync phase by the app vsync offset
description: Our Android vsync grid sits one millisecond after the true hardware vsyncs, because a Choreographer frame time is the app's target wake-up time and not the vsync it is waking for. Everything that snaps to the grid is off by that much.
created: 2026-09-12
---

# Correct the Choreographer vsync phase by the app vsync offset

`Choreographer.FrameCallback.doFrame(frameTimeNanos)` does not report a vsync.
It reports the moment the platform intended to wake the app for a vsync, and
Android wakes an app a configured offset ahead of the vsync it is drawing for.
On Android 12 that is `appWorkDuration + sfWorkDuration` before the target;
with the one-millisecond phase offsets a Samsung SM-T500 reports, it works out
to 32.333 ms, which is 1.94 refresh periods. Taken modulo the period, the grid
we derive from those samples sits 1.0 ms AFTER the true hardware vsyncs.

Measured, not inferred: the video plane snaps each frame's release time to
"closest grid point minus 80 percent of a period", and the SurfaceFlinger
latency dump puts every request 0.74 of a period before its present rather
than the 0.80 the arithmetic asks for. The gap is exactly the offset. An
independent systrace read of the same device puts `VSYNC-app` 1.85 ms after
the hardware vblank and the Choreographer `doFrame` slice 2.50 ms after it,
over 653 samples.

## Done looks like

The phase a snapped release time is computed against is a true vsync, and the
measured request phase matches the constant the code asks for. On the tablet
that means the 0.74 becomes 0.80.

## What it involves

`Display.getAppVsyncOffsetNanos()` is the missing term. The correction is

    trueVsync = frameTimeNanos + (period - appVsyncOffsetNanos)

taken modulo the period, which is all the grid uses. The period and the offset
are both available where the sample is taken, so this is one addition at the
source rather than a correction spread over the consumers.

Today the only sampler is the video plane's, in `VideoPlaneView` (the
Choreographer callback re-posted while the view is attached) reporting into
`alloy::video_plane::set_vsync_ns`, consumed by `forge::video::transport`'s
`VsyncGrid`. Anything else that later wants a vsync phase on Android should
take it from the same corrected source rather than sampling Choreographer
again and inheriting the same mistake, so the correction belongs in the
sampler and not in the video code.

Worth knowing before picking this up: it is a real error and it is NOT the
cause of the Android 12 cadence trouble recorded in
[[android-video-punch-through]]. A 1 ms shift does not cross SurfaceFlinger's
readiness comparison at any offset that was measured, and ExoPlayer's
`VSyncSampler` makes the same assumption while producing a clean cadence. Fix
it because the grid should be the grid, not because it will move a census.

## The same value is also being discarded, which is the bigger prize

There are exactly two consumers of a Choreographer frame time in the tree.
One is the video plane's sampler above, which misreads it - that is this
item. The other is the frame pacing backend's own callback, `frame_callback`
in `alloy/src/vsync.rs`, which takes it as `_frame_time_nanos` and ignores it
on purpose: the comment says the timestamp is unused because the paced clock
models time as frame counts rather than wall-clock samples.

That second one matters more than the first. Because the backend never looks
at the timestamp, the release state machine cannot know which vsync it was
woken for, so it cannot tell a wake that is on cadence from one that has
already slipped a period, and it cannot correct. An animating app on a 60 Hz
Android panel is stuck at about 51 frames a second for want of exactly that
feedback, bisected to the commit that introduced that state machine - see
[[frame-production-capped-at-50hz]].

So the work here is one change with two payoffs: sample the frame time once,
correct it by the app vsync offset as described above, and give both the
video grid its true phase and the pacing state machine the cadence feedback
it currently lacks. Pick the two items up together.

## Verifying it

The plane's own path is the cheapest check: play 25 fps content on a 60 Hz
Android 12 device, take `dumpsys SurfaceFlinger --latency` on the video layer,
and compare `(actualPresentTime - desiredPresentTime)` modulo the refresh
period against `100 - VSYNC_OFFSET_PERCENT`. The recipe and the traps are in
[[android-video-punch-through]]; note that an overlay repainting a few times a
second wrecks the census on its own, so hide it first.
