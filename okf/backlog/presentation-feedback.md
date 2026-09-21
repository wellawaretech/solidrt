---
title: Presentation feedback - measure the refresh a frame was shown on
description: The refresh count behind the app timeline is estimated from swap-return and vsync-release instants with a 0.75-period tolerance; every platform has an API that reports the actual presentation time or vblank count per frame, which would make the count a measurement and the estimator a fallback. Tier 1 of okf/design/frame-timing.md, one platform at a time, Android first.
created: 2026-09-21
---

# Presentation feedback - measure the refresh a frame was shown on

After [frame-signal-refresh-count](../plans/frame-signal-refresh-count.md)
the app timeline advances by a refresh count that alloy estimates from
reference instants: the swap's return, the vsync release, the tick
deadline. The estimate is exact whenever those instants sit within 0.75
period of the refresh they follow, and the industry answer to making that
unconditional is to stop sampling and start asking the display pipeline
which refresh each frame went out on. This is what browsers (Chrome's viz
presentation feedback), Android's Frame Pacing library and every serious
engine do; `present.rs` has called it "the correct fix" since the first
paced clock.

## The seam

`RefreshCounter::on_signal` takes a reference instant. A feedback backend
replaces that instant with the measured presentation time of the frame the
signal follows, or replaces the whole count with a vblank-counter delta.
Nothing downstream changes: the event shape, the clock, the ledger and the
contract are all in terms of the count. Where a backend is absent or a
frame's feedback has not arrived by signal time, the estimator stands.

## Per platform

**Android, Choreographer frame time.** DONE 2026-09-21: `frame_callback`
in `alloy/src/vsync.rs` converts `frameTimeNanos` to an `Instant` on the
vsync thread (age on `CLOCK_MONOTONIC`, the same clock, subtracted from
`Instant::now()`), the signal channel carries `(generation, vsync)`, and
the main loop counts a vsync-released signal from that instant. A banked
release is counted from its banked vsync too (`Release::Emit { vsync }`),
which keeps every VsyncLocked reference on the grid; counting it at the
swap's return (vsync plus libgui's 3-19 ms throttle wait) produced a
2 percent rate of zero-then-2 pairs at full rate on the Pixel 7. The
`Display.getAppVsyncOffsetNanos()` correction is deliberately not applied:
a constant phase offset does not change a refresh count; a consumer that
needs the absolute phase takes it from the video plane's sampler
([choreographer-vsync-phase-offset](../done/choreographer-vsync-phase-offset.md)).

**Android, `EGL_ANDROID_get_frame_timestamps`.** Per frame: requested
present, latch, actual present, display retire, GPU composition done. The
actual present time is the truth for every emission path, and the queue
time is what [pacing-budget-samples-swap-throttle](pacing-budget-samples-swap-throttle.md)
needs. Driver support is the question: likely absent on the TV's Mali
r20p0, present on the Adreno 610 tablet and the Pixel 7 to be confirmed by
listing the client context's extensions first. Access through SDL's EGL
handles (`SDL_EGL_GetCurrentDisplay`, `SDL_EGL_GetWindowSurface`) in
`sdl_utils.rs`.

**Wayland, `wp_presentation`.** The compositor answers each committed
frame with the presentation time, the refresh duration and the sequence
counter, or `discarded`. This is the platform where the swap-return noise
was measured and where design 3 (per-sample rounding) failed, so it is the
one that turns the estimator into a fallback where it matters most. Needs
the `wl_display` and `wl_surface` from SDL's window properties and a
Wayland client binding; the request must be issued before the commit the
swap performs, on the raster thread. Dependency policy applies: a
battle-tested binding used through its safe API (Smithay's
`wayland-client` is the candidate), pinned exactly. Hyprland's support and
its behavior under mailbox presentation are to be measured before
building.

**Windows and macOS, ANGLE.** ANGLE exposes `EGL_ANGLE_sync_control_rate`
and `EGL_CHROMIUM_sync_control` (`eglGetSyncValuesCHROMIUM`: last vblank
time, vblank counter, swap counter) on D3D11; the counter delta between
signals is the refresh count directly. Check the extension list on both
ANGLE backends; DXGI `GetFrameStatistics` and `CVDisplayLink` are the
fallbacks if the extension is missing. Note the D3D11 present-fence
behavior recorded in [angle-present-fence-pacing](angle-present-fence-pacing.md).

## What feedback can and cannot do for the count (2026-09-21)

Read against the code before starting the Wayland backend: the refresh
count rides the frame signal, and the frame signal for frame k+1 is
emitted at frame k's swap return, before the compositor has said which
refresh frame k went out on. `wp_presentation` feedback for frame k
arrives up to a refresh later (more under mailbox presentation), so it
cannot decide frame k+1's count at the moment that count is needed. What
it can do: calibrate the counter's anchor to the true grid (the phase
error becomes zero instead of the warm-up residual), report discarded
frames, and give `missedPresents` the compositor's own sequence numbers.
Making the count exact at signal time would take a pacing mode that defers
the frame signal to the feedback itself, the way VsyncLocked defers it to
the Choreographer - a frame of latency, the trade the TV policy already
makes for fluency, and a decision of its own.

The Android Choreographer path has no such gap: the vsync instant is known
when the signal is released (implemented 2026-09-21; device verification
pending). The same holds for any platform whose frame signal is released
by a vsync callback.

So the Wayland and ANGLE items below are calibration and validation
sources first. The measured need decides when they are built: the 1/s
`refresh count` debug line's `zero` and `multi` columns at full rate are
the reading (this laptop, Wayland/Hyprland, SwapPaced, 2026-09-21: both 0
over minutes, one 0-then-2 pair in 361 frames near full rate).

## Done looks like

On a platform with a backend, the ledger's counts agree with the
compositor's own record (SurfaceFlinger census on Android, `wp_presentation`
sequence numbers on Wayland) frame for frame, and the estimator's tolerance
branches are never taken on a healthy run; on a platform without one,
nothing changes. `okf/design/frame-timing.md` D2 and D8 are updated per
platform as each lands.

## Order

Android Choreographer first (no new dependency, closes the "value is being
discarded" thread), then Wayland (the noisy platform), then ANGLE. Each is
a bounded piece of work with its own device verification; none blocks the
others.
