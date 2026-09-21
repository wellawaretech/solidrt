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

**Android, Choreographer frame time.** Already in hand: `frame_callback`
in `alloy/src/vsync.rs` receives `frameTimeNanos` and discards it. Carry
it through the signal channel as `(generation, frame_time)`, convert to
the main loop's `Instant` by reading `CLOCK_MONOTONIC` on the vsync thread
(the same clock) and subtracting the difference, and correct by
`Display.getAppVsyncOffsetNanos()` as the video plane's sampler does
([choreographer-vsync-phase-offset](../done/choreographer-vsync-phase-offset.md)).
Applies to vsync-released signals only: a banked release fires at present
return and its refresh is the one after the present, not the vsync that
was banked.

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
