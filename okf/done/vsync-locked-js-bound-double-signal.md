---
title: A JS-bound app under VsyncLocked gets two frame signals per present
description: When the JS thread builds a frame for longer than the fallback deadline (~1.6 periods), FrameRelease gives the in-flight window up, idle Ticks resume while JS is still busy, and the eventual present adds its own vsync-released signal - so a 25 fps app on the Pixel 7 runs two JS frames per present, with honest but ragged tick deltas (1 and 6 periods alongside the compositor's 3 and 4).
created: 2026-09-21
---

# A JS-bound app under VsyncLocked gets two frame signals per present

Seen 2026-09-21 on the Pixel 7 (90 Hz, VsyncLocked) with
`probes/slow-frame-tick-probe.tsx` at `busy 40` (a 40 ms busy-wait per
`onFrame`), reading the refresh-counted ticks against the compositor's own
census of the app layer:

| | per 10 s |
| --- | --- |
| SurfaceFlinger present intervals | 23 of 3 refreshes, 39 of 4 |
| JS frames (tick deltas) | 262: 51 of 1 period, 28 of 2, 21 of 3, 82 of 4, 30 of 5, 50 of 6 |

The tick sum matches wall time (10533 vs 10536 ms) and every delta is an
honest count of the refreshes between two JS frames; the count is not the
problem. The problem is that the JS thread ran about 260 frames for 62
presents' worth of display time: two frame signals per present.

## Mechanism

`FrameRelease` (`alloy/src/vsync.rs`) treats a frame as in flight from its
signal's emission to its present, and the idle-tick gate holds while it is.
The in-flight window is bounded by the fallback deadline (request + period
+ delay + slack, about 1.6 periods): if neither the present nor the next
signal has arrived by then, `on_wake` gives the window up. A JS frame that
takes 40 ms exceeds that on a 90 Hz panel, so mid-build the loop reads as
idle, Ticks resume at the refresh cadence (coalesced by lattice into one
signal, delivered as soon as JS is free), JS runs a frame for that Tick,
and then the present's own vsync-released signal arrives one or two
refreshes later and JS runs another. The extra frame costs a full build
and a present that a browser would have skipped.

## Done looks like

The probe at `busy 40` shows one JS frame per compositor present: tick
deltas of 3 and 4 periods in the census's proportions, nothing else. The
same on the tablet (60 Hz).

## What it involves

The in-flight window should end at the present, not at a deadline, while
the raster queue or the UI thread is demonstrably busy: the idle gate
already consults the raster queue depth; the UI side's "building a frame"
state is the missing fact (lattice knows when a frame verb is running).
Alternatively the deadline stays for the lost-signal case but the Tick
that follows it is suppressed while a frame request is latched (a JS
frame in progress will present). Either way this is a frame-driver
property and belongs with [frame-driver-pacing-contract](frame-driver-pacing-contract.md);
the honest count from [frame-signal-refresh-count](../plans/frame-signal-refresh-count.md)
is what made it measurable.
