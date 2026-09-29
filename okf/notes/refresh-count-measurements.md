---
title: Refresh counting, measured
description: What the refresh counter was measured against on the laptop (60 Hz, SwapPaced) and the Pixel 7 (90 Hz, VsyncLocked) - the old clock's GAIN ramp and 500 ms snap, the coalesced-signal and set_hz traps the counting hit, the banked-release reference on Android, and how much reference noise the 0.75-period tolerance absorbs.
created: 2026-09-21
---

# Refresh counting, measured

Cut from [frame-signal-refresh-count](../done/frame-signal-refresh-count.md)
at its close. The design and its reasoning are in
[frame-timing](../design/frame-timing.md), decision D1. The probe throughout is
`probes/slow-frame-tick-probe.tsx`: `onFrame` busy-waits a configurable number
of ms and the probe reports the JS-side tick deltas against `performance.now()`.

## The old clock, for the record

Laptop (60 Hz, SwapPaced, release client v0.0.60), 2026-09-21. Full rate: 181
deltas, every one 16.67 ms (min 16.55, max 17.57), tick and wall elapsed both
3017 ms. `busy 45` (~20 fps): deltas from 18.0 to 42 ms across every
quarter-period bin from 1.0 to 2.5, plus four deltas of 502.8 ms in 8 s; tick
elapsed 7580 ms against 8001 wall. `busy 30` (~30 fps): a steady 31 ms (157 of
199 in the 1.75-period bin), tick elapsed 6180 against 6005 wall, the ~167 ms
equilibrium lag paid back as a 3 percent speed-up. Exactly the D1 analysis:
the GAIN ramp, the 500 ms snap, and below the snap threshold a lag no app can
see.

## Traps the counting hit

- **Coalesced frame signals must carry their counts.** The first run of the
  counter was perfect at full rate (180 deltas, every one 16.67, tick 3000 vs
  wall 2999) and alloy's count right in every regime (60-62 refreshes per
  second on the debug line), but at 20 fps JS still saw 16.67 ms deltas with
  tick time at a third of wall. A JS-bound app leaves the main loop idle in
  its own terms (SwapPaced, nothing in flight), so idle Ticks keep firing at
  the refresh rate while JS is busy (66-68 signals/s, the presents landing on
  an already-counted refresh count 0), and the event loop's batch collapse
  kept only the newest signal - with only ITS count. Fixed by carrying the
  superseded signals' counts on the survivor
  (`runtime::coalesce_frame_signals`). The old clock never showed this
  because it advanced one period per delivered signal regardless, then leaned
  on GAIN.
- **An accounting rule is only as good as the paths that reset its memory.**
  The first desktop read of the ledger-based misses gave `missedPresents` 0 at
  22-28 fps where the raster-side accounting had read ~174 per 5 s window. The
  main loop feeds `set_hz` every iteration and the counting's `set_hz`
  dropped the interval in flight unconditionally, so every present read as
  undemanded. Fixed (a no-op unless the rate changed) with a unit test that
  feeds the same rate between two presents. Any rule whose input is "did the
  previous present demand a frame" needs a test per code path that can reset
  that memory.
- **`/stats` `timeMs` is not the app timeline.** It is the client's monotonic
  wall clock, so reading it against wall time proves nothing about the
  counter. The timeline evidence is the probe's JS-side tick sum against
  `performance.now()` on the same device.

## Reads after the fixes

- Laptop, same probe: full rate 181 deltas all 16.67 (tick 3017 vs wall
  3012); `busy 45` (~20 fps) deltas only 33.33 and 50 ms (52 and 125 of 177),
  tick 7983 vs wall 7999; `busy 30` deltas only 16.67 and 33.33, tick 6000 vs
  wall 5999; `busy 12` (still 60 fps) 359 of 361 exactly one period, with one
  0-then-2 pair - a reference landing at the tolerance edge, benign since the
  pair sums right, and the reading to watch when judging the tolerance per
  platform.
- Sponza on the laptop (GPU-bound at 25 fps, 42 ms frames, the phone's
  regime): 25 signals per second carrying 60-61 refreshes, every signal a 2 or
  a 3, zero stray counts over 72 s.
- Pixel 7 (90 Hz, VsyncLocked, Mali-G710), 2026-09-21. Full rate: 715 of 730
  deltas exactly one period, tick sum 8078 vs wall 8074 ms; 9 zero-then-2
  pairs (about 2 percent), the same rate with the wake-minus-delay reference
  and with the Choreographer frame time as the reference, so the flaps are not
  vsync-reference noise but the banked releases, whose reference was the
  swap's return (vsync plus a 3-19 ms libgui throttle wait) mixed into on-grid
  references. Fixed by referencing a banked release at its banked vsync.
  `busy 40` (~25 fps): tick sum 10533 vs wall 10536 ms; `missedPresents` 322
  per 5 s window against 124 presents (450 refreshes minus 124 is 326); the
  compositor's census of the app layer shows the presents 3 and 4 refreshes
  apart (23 and 39 of 62), the honest cadence. The JS-side deltas are more
  ragged than the census (1s and 6s alongside the 3s and 4s) because a
  JS-bound app under VsyncLocked got two frame signals per present
  ([vsync-locked-js-bound-double-signal](../done/vsync-locked-js-bound-double-signal.md));
  each delta is still the true count between the two JS frames that ran.
- Pixel 7 with banked releases referenced at their vsync: full rate 1363 of
  1371 deltas exactly one period and no 2s at all, tick sum 15144 vs wall
  15151 ms over 15 s; the 8 zeros left are genuine same-refresh signal pairs
  (no catch-up follows, the sum still tracks wall). `busy 40`: tick 10133 vs
  wall 10131; 324 misses against 125 presents per 5 s window.

## The tolerance

In the unit tests the jittered 1:1 guard holds at +-0.5 period uniform noise
over 20 seeds after the 8-signal warm-up; +-0.6 is not guaranteed flap-free,
because the anchor's residual error eats into the 0.75 tolerance. The real
reference noise on each platform is what decides whether the tolerance is
generous or tight; the 1/s `refresh count` debug line (zero / multi at full
rate) is the field reading for it.
