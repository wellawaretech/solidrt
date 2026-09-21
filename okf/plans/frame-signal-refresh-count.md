---
title: Frame signals carry a refresh count; the app timeline advances by it
description: Replace the one-period-per-present model with slow correction (which lags and then hops below about 24 fps) by counting the display refreshes each frame signal covers in alloy and advancing the animation timeline by exactly that in lattice; a cumulative drift estimator with hysteresis makes the count exact at full rate under swap jitter and honest below it. Tier 2 of okf/design/frame-timing.md.
created: 2026-09-21
---

# Frame signals carry a refresh count; the app timeline advances by it

The reasoning, the rejected alternatives and the place of this work in the
larger picture are in [frame-timing](../design/frame-timing.md), decision
D1. This file is the work.

## Problem

`alloy/src/present.rs` advances a modeled presentation time by
`round(gap / period).clamp(0, 1)` periods per frame signal and corrects
the remainder toward wall time at `GAIN` 0.05. When a frame covers more
than one refresh the timeline falls behind by `(interval - period) /
GAIN` in equilibrium; below ~24 fps at 60 Hz (~28 at 90) that exceeds
`STALL_MS` 500, the model snaps, and `PacedClock` either skips the snap as
a suspension or passes it through as a hop depending on which frame trips
it. Between snaps the app runs at 25-80 percent of real speed. Observed on
a Pixel 7 at 21 fps on 90 Hz (Sponza fly-through, feedback item 28) and
reproduced on the laptop by capping to 20 fps.
`lattice/src/tests/paced_clock.rs` asserts the lag today (`max_lag > 300`).

## Design

**Refresh counter** (alloy, owned by the main loop). Per frame signal, from
the signal's reference instant:

```
measured = (reference - anchor) / period        // refreshes since the anchor
after    = measured - counted - 1               // drift if this signal counts as one
n        = after >  TOLERANCE ? 1 + ceil(after - TOLERANCE)
         : after < -TOLERANCE ? 0
         : 1
counted += n
```

`TOLERANCE` = 0.75 refresh periods: a reference instant may sit up to that
far from the refresh it follows. It is the bound the missed-present
accounting already relies on (`JANK_JITTER_SLACK` 0.25 plus `round()`'s
0.5), now stated once.

Properties, each a unit test below: full-rate pacing with jitter up to the
tolerance counts exactly 1 every signal, with no correction term; a 2:1
cadence counts 2 from the second frame; 4.3:1 settles into 4-4-4-5 within
two frames; a duplicate or stray signal counts 0 within a frame or two;
`counted` stays within 1.75 periods of `measured` over any span, which is
the long-run wall truth the old GAIN provided, with a bound of ~30 ms
instead of 500. A refresh-rate change re-anchors so `measured` equals
`counted` at the last signal (under two periods of drift dropped, once per
mode change). A suspension is reported honestly (300 refreshes after a 5 s
background stretch); what that means is lattice's call.

**Reference instant, per signal** (design D8): the raster thread's
present-return instant, carried on `FrameOutput::Presented`, for a
SwapPaced emission and for a banked VsyncLocked release; the release
instant minus the armed pacing delay for a vsync-released signal; the tick
deadline for an idle Tick. The main loop's receive time is not used: it
adds channel and wake latency to the jitter, and using the raster thread's
instant puts the counter and `missedPresents` on the same samples.

**Event.** `FrameRendered { frame, fps, refreshes: u32 }` and
`Tick { frame, fps, refreshes: u32 }`. `time` goes (no consumer outside
alloy). Playback sends `refreshes: 1`.

**Clock** (lattice). `PacedClock` becomes an accumulator over the count:
at scale 1, `now += refreshes * period`, except that a count worth more
than `SUSPEND_MS` advances one period (D5; the only place this threshold
now exists, which closes the one-period window where the two layers
disagreed and a snap became a hop). A stepped frame advances exactly one
period, a paused one none, any other scale `period * scale`, as today. The
`offset` bookkeeping goes: it existed to follow a modeled presentation
timeline and there is none. `set_hz`, `period_ms`, the timer reading and
`timer_live_ms` are unchanged.

**Present ledger** (alloy). A fixed ring of the last N presents (frame
index, reference instant, refreshes counted, demanded at present time),
written by the main loop as it counts, lock-free to read. It is the
assertion surface the synthetic harness in
[frame-driver-pacing-contract](../backlog/frame-driver-pacing-contract.md)
asks for, the input tier 3 reads, and it makes a duplicate-content present
a readable fact instead of an inference. Stage 1 writes it; stage 2 reads
it.

**Contract** (`packages/core/src/window.ts`, `onFrame`): `tick` advances by
the display refreshes since the previous frame, exactly one period at full
rate and whole multiples when a frame takes longer, as a
`requestAnimationFrame` timestamp does; deltas are honest, so an app that
wants motion smoother than its frame rate steps at a fixed rate or averages
on its own; a gap over 500 ms (backgrounded) advances one period.

## Stage 1: the count and the clock

alloy
- `src/present.rs`: `PresentClock` becomes `RefreshCounter { anchor_ms,
  last_ms, counted, hz }`, plain owned state, no atomics; `new`, `set_hz`,
  `period_ms`, `on_signal(reference_ms) -> u32`. The KNOWN ISSUE comment is
  rewritten: references are emission instants, the tolerance covers their
  jitter, presentation timing would make the count exact
  ([presentation-feedback](../backlog/presentation-feedback.md)).
- `src/raster/frame.rs`, `src/raster/mod.rs`: `FrameOutput::Presented { at:
  Instant, demanded: bool }`; the instant is the one `record_present_interval`
  already samples.
- `src/event.rs`: the two events above; doc comments state the contract.
- `src/app.rs`: one counter, `set_hz` where `watch` reports a change,
  `on_signal` at the four emission sites (present-return Emit, vsync
  Release, pacing-change flush, idle Tick) with the reference instants
  above; the ledger write beside it. A batch of deferred presents released
  together counts naturally: the first takes the refreshes, the rest fall
  in the same-refresh branch.
- `src/playback.rs`: `refreshes: 1`. `src/lib.rs`: export the counter.

lattice
- `src/paced_clock.rs`: no `PresentClock`, no `offset`; `tick(raw_ms,
  refreshes, mode)` where mode is paused, stepped or running at a scale;
  hz stored locally. Struct comment rewritten.
- `src/runtime.rs`, `src/lib.rs`: the frame verb receives the count from
  the signal and passes it through.

packages/core
- `src/window.ts`: the `onFrame` contract above. This discharges item 1 of
  the pacing-contract note's stage 3; items 2 and 3 there (expose the lag)
  become moot because the timeline no longer lags.

Tests
- `alloy/src/tests/present.rs`, rewritten for the counter on synthetic
  reference sequences: exact 1:1 grid; 1:1 with uniform jitter up to 0.6
  periods (the guard against the rejected `round()`); 2:1 and 4.3:1
  convergence with the drift bound held throughout; a same-instant
  duplicate and a stray signal count 0 and recover; a refresh-rate change
  re-anchors without a jump; a 5 s gap reports the whole count.
- `lattice/src/tests/paced_clock.rs`: the slow-frames test asserts the
  animation reading stays within two periods of wall (today it asserts a
  lag over 300 ms); suspension skip, pause/step/scale and the timer tests
  keep their meaning with the new signature.

Verification
1. `cargo test -p alloy --lib`, `cargo test -p lattice --lib`.
2. `make client` (release).
3. `probes/slow-frame-tick-probe.tsx`: `onFrame` busy-waits a configurable
   number of ms (debug command) and reports tick-delta min, max and a
   histogram. Against the current `dist/` binary at ~20 fps: the sawtooth
   and hops. Against the new one: deltas that are whole multiples of the
   period, none beyond the frame time, the sum tracking wall time; at full
   rate every delta exactly one period.
4. Sponza on the laptop with `SRT_HOME=<repo>`, its tour running, the same
   read; then the Pixel 7 with an APK from the repo runtime.
5. The Sponza integration: `src/index.tsx` `startTour` steps by `dt` (kept
   clamped at 0.1 s), the `step` average and the WORKAROUND comment go.

## Stage 2: one ledger for the count and the misses

`missedPresents` (`record_present_interval`, raster thread) and the
counter measure the same fact with the same tolerance philosophy on the
same instants. After stage 1 is verified, the misses are read from the
ledger: over demanded presents, `refreshes - 1` per signal, so one
mechanism produces both numbers and `/stats` can also report the cadence
(the last N counts) directly. `RasterStats::missed_presents` and the
raster-side run accounting go; `frame_history` and the connection's stats
read the ledger. Verified against the SurfaceFlinger census on the tablet
(counts sum to the census's doubled intervals) and against the probe on
the laptop.

## Not in this plan

- Presentation timestamps (tier 1): [presentation-feedback](../backlog/presentation-feedback.md).
- Cadence hold below the refresh rate (tier 3): [frame-driver-pacing-contract](../backlog/frame-driver-pacing-contract.md), stage 2.
- The idle `onFrame` rate on a SwapPaced desktop: [idle-onframe-tick-rate](../backlog/idle-onframe-tick-rate.md). The counter measures an idle Tick's refreshes honestly, so a 2-3 Hz tick now advances the timeline by the refreshes it covers instead of one period, which makes that bug visible in the tick instead of hidden by it.

## Findings

- Baseline, old clock, laptop (60 Hz, SwapPaced, release client v0.0.60),
  `probes/slow-frame-tick-probe.tsx`, 2026-09-21. Full rate: 181 deltas,
  every one 16.67 ms (min 16.55, max 17.57), tick and wall elapsed both
  3017 ms. `busy 45` (~20 fps): deltas from 18.0 to 42 ms across every
  quarter-period bin from 1.0 to 2.5, plus four deltas of 502.8 ms in 8 s;
  tick elapsed 7580 ms against 8001 wall. `busy 30` (~30 fps): a steady
  31 ms (157 of 199 in the 1.75-period bin), tick elapsed 6180 against
  6005 wall, the ~167 ms equilibrium lag paid back as a 3 percent
  speed-up. Exactly the D1 analysis: the GAIN ramp, the 500 ms snap, and
  below the snap threshold a lag no app can see.
- First run of the new client: full rate perfect (180 deltas, every one
  16.67, tick 3000 vs wall 2999) and alloy's count right in every regime
  (60-62 refreshes per second on the debug line), but at 20 fps JS still
  saw 16.67 ms deltas with tick time at a third of wall. Mechanism: a
  JS-bound app leaves the main loop idle in its own terms (SwapPaced,
  nothing in flight), so idle Ticks keep firing at the refresh rate while
  JS is busy (66-68 signals/s, the presents landing on an already-counted
  refresh count 0), and the event loop's batch collapse kept only the
  newest signal - with only ITS count. Fixed by carrying the superseded
  signals' counts on the survivor (`runtime::coalesce_frame_signals`). The
  old clock never showed this because it advanced one period per delivered
  signal regardless, then leaned on GAIN.
- Rebuilt with the carry, same probe, same laptop: full rate 181 deltas
  all 16.67 (tick 3017 vs wall 3012); `busy 45` (~20 fps) deltas only
  33.33 and 50 ms (52 and 125 of 177), tick 7983 vs wall 7999; `busy 30`
  deltas only 16.67 and 33.33, tick 6000 vs wall 5999; `busy 12` (still
  60 fps) 359 of 361 exactly one period, with one 0-then-2 pair - a
  reference landing at the tolerance edge, benign since the pair sums
  right, and the reading to watch when judging the tolerance per platform.
- Sponza on the laptop (GPU-bound at 25 fps, 42 ms frames, the phone's
  regime): 25 signals per second carrying 60-61 refreshes, every signal a 2
  or a 3, zero stray counts over 72 s. (A first version of this entry
  quoted `/stats` `timeMs` against wall time; `timeMs` is the client's
  monotonic wall clock, not the app timeline, so that read proved nothing.
  The timeline evidence is the probe's JS-side tick sum against
  `performance.now()` on the same device.) The workaround came out of
  `src/index.tsx` (`startTour` steps by `dt`).
- Stage 2, first desktop read: `missedPresents` 0 at 22-28 fps where the
  raster-side accounting had read ~174 per 5 s window. The main loop feeds
  `set_hz` every iteration and the counting's `set_hz` dropped the
  interval in flight unconditionally, so every present read as undemanded.
  Fixed (a no-op unless the rate changed) with a unit test that feeds the
  same rate between two presents. Lesson kept: an accounting rule whose
  input is "did the previous present demand a frame" is only as good as
  the code paths that can reset that memory; each one needs a test.
- Pixel 7 (90 Hz, VsyncLocked, Mali-G710), the same probe on the device,
  2026-09-21. Full rate: 715 of 730 deltas exactly one period, tick sum
  8078 vs wall 8074 ms; 9 zero-then-2 pairs (about 2 percent), the same
  rate with the wake-minus-delay reference and with the Choreographer
  frame time as the reference, so the flaps are not vsync-reference noise
  but the banked releases, whose reference was the swap's return (vsync
  plus a 3-19 ms libgui throttle wait) mixed into on-grid references;
  fixed by referencing a banked release at its banked vsync (third APK,
  measured below). `busy 40` (~25 fps): tick sum 10533 vs wall 10536 ms;
  `missedPresents` 322 per 5 s window against 124 presents (450 refreshes
  minus 124 is 326); the compositor's census of the app layer shows the
  presents 3 and 4 refreshes apart (23 and 39 of 62), the honest cadence.
  The JS-side deltas are more ragged than the census (1s and 6s alongside
  the 3s and 4s) because a JS-bound app under VsyncLocked gets two frame
  signals per present; each delta is still the true count between the two
  JS frames that ran. Filed as
  [vsync-locked-js-bound-double-signal](../backlog/vsync-locked-js-bound-double-signal.md);
  not a counting matter.
- Third APK (banked releases referenced at their vsync), Pixel 7, same
  probe: full rate 1363 of 1371 deltas exactly one period and no 2s at
  all, tick sum 15144 vs wall 15151 ms over 15 s; the 8 zeros left are
  genuine same-refresh signal pairs (no catch-up follows, the sum still
  tracks wall). `busy 40`: tick 10133 vs wall 10131; 324 misses against
  125 presents per 5 s window. Stage 1, stage 2 and the Android half of
  tier 1 are verified on the device.
- Unit tests: the jittered 1:1 guard holds at +-0.5 period uniform noise
  over 20 seeds after the 8-signal warm-up; +-0.6 is not guaranteed
  flap-free, because the anchor's residual error eats into the 0.75
  tolerance. The real reference noise on each platform is what decides
  whether the tolerance is generous or tight; the 1/s `refresh count`
  debug line (zero / multi at full rate) is the field reading for it.
