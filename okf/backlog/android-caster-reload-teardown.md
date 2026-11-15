---
title: A reload while casting can leave the Android engine stuck in teardown
description: Once on the Pixel 7, a dev reload of an app that was casting its window live left the old engine stuck in its teardown, the swap never building a frame until a force-stop; the same scene headless on the laptop tears down cleanly, so the device-side part (the MediaCodec encoder's flush, the window tap, the camera) is not pinned down.
created: 2026-11-14
---

# A reload while casting can leave the Android engine stuck in teardown

## Symptom

Seen once on the Pixel 7, 2026-10-16, with `examples/screencast` casting
the window live over TCP: a reload from the dev server left the old
engine stuck in its teardown. The new engine never built a frame, the
window stayed on the old picture, and only a force-stop recovered the
app.

The same scene headless on the laptop (a TCP loopback cast left open at
the test's end) tears down cleanly, so the cause is on the device side.
Engine teardown stops every open recording and closes its window tap;
the host awaits the recorder's join, bounded, before the next engine
(flux/src/alloy_plugins/record.rs). So a recorder thread alone should
not hold the swap.

## Suspects

- **The MediaCodec encoder's flush or stop** blocking while the codec
  still holds input buffers (libvpx is synchronous, so the laptop never
  takes this path).
- **The window tap** being closed while its readback is in flight on
  the raster thread, during the swap.
- **The camera**, if the cast also had it open.

## Done looks like

- A reproduction: a reload while casting, repeated (say 20 times) on the
  Pixel, with a thread dump (`debuggerd -b <pid>`) when it sticks.
- The stuck wait found and bounded, or the reload order fixed; then 20
  reloads in a row with no stuck teardown.
