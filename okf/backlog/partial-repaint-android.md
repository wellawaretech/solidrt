---
title: Partial repaint on Android's multisampled fast path
description: On the MediaTek TV a 20 px animation costs ~40 ms GPU per frame - 88% of a full-window change - because the multisampled-FBO0 fast path draws full frames and partial repaint steps aside; decide and build the patch-confined alternative (MSRTT rig + patch copy vs in-tile full frame), measured per device.
created: 2026-09-02
---

# Partial repaint on Android's multisampled fast path

## Symptom

Measured 2026-09-02 on the Philips TPM171E (MediaTek armv7, 1080p50),
release client, two probes sharing an 800-rect static field and a 10 Hz
animation (`probes/damage-probe.tsx` / `damage-probe-full.tsx`):

- 20x20 mover: gpuFrameExecMsPerFrame 39.4-40.4 ms
- window-covering animated rect: 44.4-45.4 ms

Fill cost is damage-size-independent: every animated frame pays ~40 ms
of GPU whatever changed. Desktop (Mesa/Intel) does not have this - stage
2 of [partial-repaint](../done/partial-repaint.md) confines those frames
to the damage rect there.

## Why it does not engage

Android's window backbuffer is multisampled (4x on this TV), so frames
take the in-tile fast path: draw straight into FBO 0, driver resolves at
swap - the cheapest possible FULL frame. Impeller clears every wrapped
target, so a partial frame cannot be drawn into the preserved back
buffer directly; `repaint_patch` (raster) deliberately answers full when
`gl::window_fast_path` is true.

## What done looks like

The mover probe's gpuFrameExecMsPerFrame drops by an order of magnitude
on the TV while `partialPresents` climbs, with no visual artifacts - or
a measurement showing the rig detour costs more than it saves on the
target tilers, recorded as a documented limitation.

## Rough shape

Route Android partial frames through the MSRTT rig like other partial
frames (the machinery exists: DL root clip + scissored resolve copy,
`run_pass` scissor): give up the multisampled window config (or leave
FBO 0 single-sample when partial is viable) and pay rig + patch-copy
per frame instead of full in-tile. Whether that wins depends on the
tiler; decide per measurement, not in general. First check
EGL_EXT_buffer_age on the app's EGL context there - SurfaceFlinger
lists EGL_KHR_partial_update (which requires buffer age), so it very
likely exists. `eglSetDamageRegionKHR` may then also confine the
driver's tile writeback on the fast path itself, which would be the
cheapest possible answer - worth probing before any rig rerouting.
