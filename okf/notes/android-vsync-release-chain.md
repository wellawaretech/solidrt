---
title: The Android vsync release chain, traced against the compositor
description: What a 5 s atrace of an animating app on a 60 Hz Android 12 tablet shows about the vsync-locked frame chain - the Choreographer callbacks are on cadence, the swap blocks up to a period behind the previous frame's GPU work so the present-return races the next vsync signal, and the GPU cost behind that was the rig path taken because GL denies FBO 0's multisampling on Adreno while EGL reports it - with the numbers, the census recipe and the traps.
created: 2026-09-12
---

# The Android vsync release chain, traced against the compositor

Measured 2026-09-12 on a Samsung SM-T500 (Android 12, 60 Hz, Adreno 610)
with `probes/vsync-cadence-probe.tsx`: one marker on the position lane in a
fullscreen window, retargeted every leg, 0.1 ms build. Instruments: the
per-layer present census (`dumpsys SurfaceFlinger --latency` on the
`SurfaceView ... (BLAST)#0` layer; the plain SurfaceView layer shows no
presents), the client's `/stats` window, and `atrace -t 5 -b 65536 gfx view
sched`, which puts SurfaceFlinger's VSYNC-app ticks, the app's threads and
the raster thread's swap in one clock.

## The chain, per frame

Thread names in the client process: `SDLThread` is the alloy main loop,
`srt-ui` the UI side (two threads), `srt-raster` the raster thread,
`srt-vsync` the Choreographer backend. Per frame the vsync thread runs three
times: woken by SurfaceFlinger's `app` thread (the callback), woken by its
own sleep end (the signal, after the pacing delay), and woken by SDLThread
(the next request, sent as the frame signal is emitted).

Numbers over one 5 s trace, animation running:

| | count |
|---|---|
| VSYNC-app ticks | 300 |
| Choreographer callbacks delivered | 294 |
| vsync signals sent | 290 |
| swaps (eglSwapBuffers) | 248 |
| callbacks that slipped a period | 7 |

- Callback delivery is 0.4 ms after the tick (p50, max 2.6). The request
  that arms it is posted 7.7 ms before the tick (p50, min 3.1). The vsync
  thread is not where frames go missing, and neither the re-arm margin nor
  "post the callback before the sleep" can move the count.
- The pacing delay sits at its 8 ms floor, not the ~14 ms the 0.1 ms build
  would allow, because the budget samples emission-to-present and that
  includes the swap wait below.

## The swap blocks behind the previous frame

`eglSwapBuffers` does not return when the buffer is queued. Inside it,
libgui's `BufferQueueProducer::queueBuffer` for the EGL API waits on the
PREVIOUS queued buffer's acquire fence ("Throttling EGL Production" in
AOSP: two full buffers may be queued, not a third). That fence is the
previous frame's GPU completion, and the GPU work for a frame cannot start
until the buffer it renders into has been released by the compositor, so
the wait is bound to SurfaceFlinger's cycle, not to the frame's cost. In
the trace the wait (`waitForever` nested in `queueBuffer` nested in
`eglSwapBuffers`, ending with a "GPU completion fence" mark) runs 3 to 19
ms; the swap's own queueBuffer happens 2 ms in.

`FrameOutput::Presented` is sent when the swap returns, so the present
reaches the main loop 3 to 19 ms after the swap started, while the next
vsync signal arrives about 15 ms after it:

| | on-cadence frames (197) | two-period gaps (50) |
|---|---|---|
| wait end minus swap start, p50 | 8.3 ms | 16.7 ms |
| present-return after the next signal | 0 | 43 |

The remaining 7 gaps are the 7 slipped callbacks. That is the whole
1,1,1,1,2 census: one frame in six, the present-return loses the race, and
until 2026-09-12 `FrameRelease::on_wake` treated a signal with nothing
pending as "demand stopped", ended the chain, and the late present
re-armed from scratch, one period later. The regression bisected to the
FrameRelease commit because before it a stale signal could still release.
The same window also let idle Ticks fire mid-animation (86 in 31 s): the
idle gate was "nothing pending" and a frame in the swap is nothing pending.

The fix (`banked` in `alloy/src/vsync.rs`): a signal that beats the
in-flight present is kept and releases the present on return; the
in-flight window closes the idle gate and bounds the wait at the armed
deadline; a second signal with nothing pending still ends the chain.

That alone did not move the rate (255 frames per 5 s, same census). It
changed the regime: recovered frames emit at present-return, and the
swap's wait then runs ~16 ms on nearly every frame, so the loop is paced
by the swap chain at the same ~51 fps. SurfaceFlinger's own frame
timestamps (`--latency`, third column = the buffer's GPU fence signaling)
showed why: consecutive frames became GPU-ready 18 to 20 ms apart whatever
the queue timing, on this build and on 0.0.58, while 0.0.55 delivered them
14 to 16 ms apart. The GPU cost per frame was the cap, and that came from
the second finding below.

## GL denies the multisampled backbuffer; EGL does not

The window fast path (draw straight into FBO 0, the driver resolves 4x MSAA
in-tile at swap) is taken when FBO 0 reports multisampling. On the SM-T500
`glGetIntegerv(GL_SAMPLE_BUFFERS)` on FBO 0 answers 0 while EGL says the
current draw surface is the real window surface on config id 35 (RGBA
8888, depth 24, stencil 8, 4 samples; the device offers 4-sample window
configs for every RGBA/RGBX/565 depth-stencil combination). The 0.0.55
client's log read "4x multisampled" through the same GL query and the
2026-07-28 note ([[android-surface-swap-latency]]) recorded the device as
"denied" a multisampled config - both were GL's word, and it flip-flops
across builds for reasons not found (same SDL 3.4.10, same attributes,
same driver V@0502). With GL's answer this build took the rig path: an
offscreen render plus a full-frame resolve per frame, ~19 ms of GPU
pipeline instead of ~15. `window_samples` now asks the surface's EGL
config when GL denies multisampling, and the count is forgotten on every
window-surface rebind rather than latched for the process.

Result on the tablet with both fixes: 300 frames per 5 s window, 0 missed
presents, census 126 single intervals of 126, frames GPU-ready 16 ms apart.
Pixel 7 (Mali-G710, Android 17): 90 fps at 90 Hz and 60 at a forced 60 Hz
before and after, GL and EGL agreeing on 4 samples. The TV (SwapPaced,
GL already reporting 4x) is untouched by construction.

Not the cause, checked: frame cost (0.11 ms p50), refresh rate (16.67 ms
reported), a hardcoded period (none), `PacingBudget::MARGIN_MS`,
`PRESENT_FENCE_DEPTH` (the raster thread's own fence wait is before the
draw and never blocked here), and the `glFlush` after the post-swap fence
(inside the blocking swap's shadow, not the blocker). Linux (Wayland, no
vsync backend, present-return pacing) runs the same probe at 300 frames per
5 s window with 0 missed presents.

## Choreographer frame times are wake-up times

`frameTimeNanos` is the app's scheduled wake-up, one
`Display.getAppVsyncOffsetNanos()` (1 ms on this tablet) after a true
hardware vsync; a grid derived from raw frame times sits that much late.
The video plane's sampler subtracts the offset at the source
(`VideoPlaneView.java`); the pacing backend never reads the timestamp and
does not need to (see above).

## Recipe and traps

- Serve the probe from the repo root (`bunx srt run
  probes/vsync-cadence-probe.tsx --project --port <N> --lan`), install the
  matching dev APK from `dist/android/<abi>/solidrt-go.apk` first, then
  `bun packages/cli/bin/srt android --port <N>`. A client from another
  release runs the bundle at 1 fps and reads like a cap.
- Census: `dumpsys SurfaceFlinger --latency-clear`, wait, `--latency
  '<layer>'`; round present deltas to refresh periods. Use `adb shell ...
  < /dev/null` inside any `while read` loop, or adb eats the loop's input.
- Trace: `atrace -t 5 -b 65536 gfx view sched -o /data/local/tmp/t.txt`,
  `adb pull`. Classify the vsync thread's runs by waker (SurfaceFlinger's
  `app` thread = callback, SDLThread = request, anything else = sleep end;
  a timer wake is attributed to whatever task the CPU was running).
- SDLThread slept only 15 times in those 5 s (259 switch-outs, mostly
  preemptions) and `/stats` read 135% CPU, against 7% for the same probe on
  Linux; the Pixel 7 reads 136% too. SDL's Android event wait does block on
  a timed semaphore, so the cause is not obvious; not investigated
  ([[android-main-loop-rarely-sleeps]]).
- A display mode change is not observed by the client: the Pixel 7 forced
  to 60 Hz kept reporting `periodMs` 11.11, so `missedPresents` counted
  against 90 Hz ([[android-refresh-rate-change-unobserved]]).
