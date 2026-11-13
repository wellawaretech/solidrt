---
title: Presentation feedback - calibrate and validate the refresh count
description: Where the frame signal is released at swap return the refresh count is estimated from noisy instants against an anchor frozen after warm-up; every platform reports per frame the refresh it was shown on, too late to decide the count but in time to put the anchor on the true grid and to check the count against the compositor's record. Tier 1 of okf/design/frame-timing.md; the Android Choreographer half landed, the rest is built per platform when its refresh-count reading asks for it.
created: 2026-09-21
---

# Presentation feedback - calibrate and validate the refresh count

The app timeline advances by a refresh count
([frame-signal-refresh-count](../done/frame-signal-refresh-count.md)).
Where a vsync callback releases the frame signal, the count is taken from
that vsync's instant and sits on the grid: Android under VsyncLocked, on
the Choreographer frame time since 2026-09-21 (`okf/design/frame-timing.md`
D8, numbers in
[refresh-count-measurements](../notes/refresh-count-measurements.md)).
Everywhere else the signal is released at swap return and alloy estimates
the count from that instant (`alloy/src/present.rs`): exact while the
instant sits within 0.75 period of the refresh it follows, measured against
an anchor whose phase is the mean of the first eight references and is
frozen after that.

Every platform has an API that reports, per frame, which refresh the frame
went out on. Browsers (Chrome's viz presentation feedback), Android's
Frame Pacing library and every serious engine read it. This item is what
that feedback is worth to the count here, and what it is not.

## What feedback can and cannot do for the count

The refresh count rides the frame signal, and the frame signal for frame
k+1 is emitted at frame k's swap return, before the compositor has said
which refresh frame k went out on. `wp_presentation` feedback for frame k
arrives up to a refresh later (more under mailbox presentation), so it
cannot decide frame k+1's count at the moment that count is needed.

A vblank counter read at signal time does not get around this. The read
happens at the swap's return, which is the noisy instant: a return 0.6
period early or late lands on the other side of a vblank, and the delta
between two reads flaps 0, 1, 2 exactly as the per-sample rounding did
(design 3 in the design document's D1). The counter is useful for its
vblank time, not for its delta.

What feedback can do:

- **Calibrate the anchor.** A measured presentation time is a point on the
  true grid. With it the anchor's phase error is zero instead of the
  warm-up residual, the whole tolerance is left for the swap-return noise,
  and the period is the measured one instead of the nominal rate.
- **Validate the count.** The compositor's sequence numbers are its own
  record of what the ledger counts, in process, on every run, where today
  that check is an external census.
- **Report what the count cannot see:** discarded frames, and
  `missedPresents` from the compositor's sequence numbers.

Making the count exact at signal time would take a pacing mode that defers
the frame signal to the feedback itself, the way VsyncLocked defers it to
the Choreographer - a frame of latency, the trade the TV policy already
makes for fluency. That is a decision of its own and not this item.

## When to build

The measured need decides. The reading is the 1/s `refresh count` debug
line's `zero` and `multi` columns at full rate: both 0 means the estimator
is already exact there and a backend buys validation only. On the laptop
(Wayland/Hyprland, SwapPaced, 2026-09-21) both read 0 over minutes, with
one 0-then-2 pair in 361 frames near full rate. A platform whose reading
is not 0 at full rate is the one to build first.

## The seam

`RefreshCounter::on_signal` keeps taking a reference instant and nothing
downstream changes: the event shape, the clock, the ledger and the
contract are all in terms of the count.

Calibration needs a second entry on the counter, which does not exist yet:
a measured grid point (a presentation instant on the main loop's origin,
and the refresh period where the platform reports one). It moves the
anchor's phase onto that grid and never touches `counted`. Today the
anchor freezes after `WARMUP_SIGNALS`; with a backend it follows the
feedback instead, and the warm-up stays as the path for a platform without
one or a frame whose feedback has not arrived.

Validation is a ledger concern: a `SignalRecord` carries the compositor's
sequence number when the backend has it, and the 1/s line reports how many
counts disagreed with it.

The platform half is a fact ore reports per frame as plain values (instant,
period, sequence, discarded); alloy feeds them to the counter. Feedback
times are on `CLOCK_MONOTONIC` and become an `Instant` by subtracting
their age, as `frame_callback` in `ore/src/display_vsync.rs` does.

## Per platform

**Android, Choreographer frame time.** Landed and verified on the Pixel 7,
2026-09-21; recorded in D8 and the measurements note above. Nothing open.

**Android, `EGL_ANDROID_get_frame_timestamps`.** The plumbing exists:
`alloy/src/frame_timestamps.rs` enables the extension, asks the frame id
before each swap and polls a ring after each present, for the frame's GPU
term
([android-frame-gpu-time-from-egl-timestamps](../done/android-frame-gpu-time-from-egl-timestamps.md)).
It reads the rendering-complete and requested-present times; the second is
the queue time
[pacing-budget-samples-swap-throttle](pacing-budget-samples-swap-throttle.md)
needs. What is left here is one more name in the same poll, the display
present time, probed per surface like the others. It is the validation
source for both pacing modes and the calibration source for the TV, the
Android device that runs SwapPaced. The extension answers on the Adreno
610 tablet; whether the present time is supported there, on the Pixel 7
and on the TV's Mali r20p0 is what the probe says.

**Wayland, `wp_presentation`.** The compositor answers each committed
frame with the presentation time, the refresh duration and the sequence
counter, or `discarded`. This is the platform where the swap-return noise
was measured and where per-sample rounding failed. It belongs in ore,
behind the `window` feature. What it involves:

- The `wl_display` and `wl_surface` from SDL's window properties, and a
  Wayland client binding. Dependency policy applies: Smithay's
  `wayland-client` is the candidate, pinned exactly. Attaching it to SDL's
  connection is not in its safe API: it goes through
  `Backend::from_foreign_display`, an `unsafe fn` behind the
  `client_system` feature, so that is one `unsafe` call with its `SAFETY:`
  line (the display outlives the backend).
- The feedback request must be issued before the commit the swap performs,
  on the raster thread.
- The feedback events land on a queue of our own on SDL's connection. SDL's
  pump reads the socket; which thread dispatches our queue, and when, is
  to be settled before building.
- Hyprland's support and its behavior under mailbox presentation are to be
  measured first.

**Windows, ANGLE over D3D11.** ANGLE exposes `EGL_ANGLE_sync_control_rate`
and `EGL_CHROMIUM_sync_control` (`eglGetSyncValuesCHROMIUM`: last vblank
time, vblank counter, swap counter). The vblank time is the grid point; the
counters pair a swap with the vblank it was shown on, for validation. Check
the extension list first; DXGI `GetFrameStatistics` is the fallback if the
extension is missing. Note the D3D11 present-fence behavior recorded in
[angle-present-fence-pacing](angle-present-fence-pacing.md).

**macOS, ANGLE over Metal.** The display link already runs: it paces the
swap since 2026-09-25 (`ore/src/gl_surface/display_link.rs`), and only its
tick count is used. Its callback also carries the output time of the vsync
it prepares for, about one and a half periods ahead: a grid point with no
new dependency. Check ANGLE-Metal's extension list for the sync-control
pair as on D3D11.

## Done looks like

On a platform with a backend, the counter's anchor sits on the measured
grid, the ledger's counts are checked against the compositor's own record
(SurfaceFlinger's on Android, `wp_presentation` sequence numbers on
Wayland) and the disagreements are a reported number, 0 on a healthy
full-rate run alongside `zero` and `multi`. On a platform without one,
nothing changes. `okf/design/frame-timing.md` D2 and D8 are updated per
platform as each lands.

## Order

The Android present time first: the ring exists and it gives validation
something to stand on. Then the calibrate entry with Wayland (the noisy
platform), then macOS and Windows. Each is a bounded piece of work with
its own device verification; none blocks the others.
