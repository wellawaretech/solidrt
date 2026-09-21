---
title: Frame timing
description: The clocks and cadences of a SolidRT client in one place - how a frame signal is produced per platform and pacing mode, how display refreshes are counted, which timeline each consumer runs on (onFrame, timers, transitions, video, the dev clock, playback), the decisions behind each with the alternatives that were rejected, and the known limits with their open items. Read before touching anything that says clock, tick, vsync or pacing.
created: 2026-09-21
---

# Frame timing

Timing questions have come back in every quarter of this project: the paced
clock, the timer lag, the TV fluency hunt, the 51 fps cap on Android, the
idle-tick runaway, the video cadence, and now the tick that hops below the
refresh rate (Sponza feedback, item 28). Each was answered in its own done
record or backlog item, and each new session re-derived the picture from
those fragments. This document is that picture, kept current. The records
it gathers are linked where they carry the measurements; the decisions and
their reasoning are restated here so the fragments do not have to be read to
work safely in this area.

The code this describes: `alloy/src/app.rs` (the main loop),
`alloy/src/vsync.rs` (the vsync source and the release state machine),
`alloy/src/present.rs` (the refresh counter), `alloy/src/raster/frame.rs`
(present, fences, missed-present accounting), `lattice/src/paced_clock.rs`
(the app timelines), `lattice/src/runtime.rs` (the frame verb),
`flux/src/standards_plugins/time.rs` (virtual timers, the Timeline),
`packages/core/src/window.ts` (`onFrame`, `requestAnimationFrame`).

## Vocabulary

- **Refresh, period.** One display scan, and its duration (16.67 ms at 60 Hz,
  11.11 at 90, 20 at 50). The display shows every frame for a whole number
  of refreshes; there is no such thing as a frame shown for 1.4 of them.
- **Present.** A swap: the raster thread hands a finished frame to the
  compositor. The swap may block (queue full, or the previous frame's GPU
  work still running) and returns at an instant that is only loosely tied
  to the refresh the frame will be shown on.
- **Frame signal.** The event that starts the next frame's work on the UI
  thread: `FrameRendered` after a present, or an idle `Tick` when nothing
  was presented for a period. Both run the same per-frame JS work.
- **Reference instant.** The moment a frame signal is stamped with. It is
  the sample the refresh count is derived from, and its noise relative to
  the refresh it follows is the central difficulty of this whole area.
- **Refresh count.** How many display refreshes passed between one frame
  signal and the previous one. One at full rate; more when a frame took
  longer than a period. This is the fact the app timeline advances by.
- **Cadence.** The steady pattern of refresh counts an app settles into: 1
  at full rate, 2 at half, 4-4-4-5 for a 21 fps app on a 90 Hz panel.
- **Demand.** Rendering is demand-driven: a frame is built only when
  something asked for one (a property write, a standing `onFrame`, a
  playing video). No demand, no present, and the idle Tick keeps the
  per-frame logic alive at the refresh cadence instead.

## The chain, per platform and pacing mode

Three threads (alloy/CLAUDE.md): the main loop pumps SDL events and does
frame bookkeeping, `srt-ui` runs JS, layout and paint into display lists,
`srt-raster` owns the one GL context and presents. The main loop blocks on
the SDL queue plus a wake from the raster thread after each present and
from the vsync thread after each signal; it never polls.

**Present.** The raster thread draws, swaps with vsync on (interval 1), and
awaits a GL fence two presents back (`PRESENT_FENCE_DEPTH` 2) so the CPU
never runs more than one frame ahead of the GPU while draw still overlaps
the compositor's latency ([idle-tick-gpu-backlog-runaway], measured on the
TV where depth 1 timed out on every frame). It then sends
`FrameOutput::Presented` to the main loop and records the missed-present
interval ([stats-present-interval-jank]).

**Frame signal, SwapPaced** (desktop, TV, any device without a touch
screen): the main loop emits `FrameRendered` the moment `Presented` arrives.
The blocking swap paces production: the buffer queue fills and the swap
returns once per refresh. Metronomic presentation, one to two frames more
input latency than the alternative. On Wayland with a mailbox compositor
the swap does not block until the queue is full, so the return instant
jitters within the period; this is the noise the refresh counter tolerates
(below).

**Frame signal, VsyncLocked** (touch devices, Android with a Choreographer):
the present's signal is deferred to the display vsync. `VsyncSource` arms
one Choreographer callback per request on its own looper thread; the
callback fires at the vsync, the thread sleeps the pacing delay and sends
the signal. `FrameRelease` (a pure state machine, unit-tested in
`alloy/src/tests/release.rs`) decides per present whether it emits now or
waits, arms the next request at emission time (not at present return,
which lost the re-arm race and halved the rate), keeps a fallback deadline
of request + period + delay + 4 ms for a lost signal, and banks a signal
that arrives while the frame it should release is still in the swap so the
present releases on return ([frame-production-capped-at-50hz],
[android-vsync-release-chain]). `PacingBudget` picks the delay from the
worst recent emission-to-present cost so the frame starts as late as it
can and samples the freshest input ([frame-pacing]). Frames phase-lock to
the clock Android batches touch on; touch is then resampled per frame from
a short history ([frame-pacing], stage 3; [frame-batched-pointer-input]).

**Why two modes.** VsyncLocked exists for finger-to-glass latency and is
right for a tablet. On the TV its release chain's jitter beat the queue's
slack about 1.4 percent of the time and dropped a latch; swap pacing on the
same device measured 0.00-0.04 percent, because a saturated queue absorbs
the jitter. Policy is chosen in lattice from the input-modality fact (touch
present or not), re-evaluated on hotplug ([frame-pacing-fluency]). The
fact itself needed fixing: SDL over-reports touch on TV boxes, so the touch
fact is gated on Android's touchscreen feature.

**Idle Tick.** When no frame signal fired for a period and nothing is in
flight, the main loop emits `Tick` so timers, the reactive flush and the
camera pump keep running. "Nothing in flight" means no present awaiting its
vsync signal, no frame between emission and present, and an empty raster
queue; the last condition is what stopped the runaway where ticks fed a
backlogged raster thread more work per period than it retired
([idle-tick-gpu-backlog-runaway]). On a SwapPaced desktop client the tick
runs at 2-3 Hz rather than the refresh rate when the picture does not
change; still open ([idle-onframe-tick-rate]).

**Collapsing.** Lattice drains the alloy event channel per batch and keeps
only the newest frame signal (`lattice/src/lib.rs`): a frame is a request
for the newest state, and two signals in one batch mean one frame was
missed, not that two are owed.

## The clocks

Six clocks exist, and most timing bugs were one consumer riding the wrong
one.

| clock | where | what runs on it |
| --- | --- | --- |
| wall | `performance.now()`, `Date.now()`, `Instant` | measuring work; calendar time; the timer reading's source |
| refresh count | alloy, per frame signal | the fact the app timeline advances by |
| animation timeline | `PacedClock::now_ms` (lattice) | `onFrame` tick, `requestAnimationFrame`, the render event, element and node transitions, silent video streams |
| timer timeline | `PacedClock::timer_now_ms` | `setTimeout`, `setInterval` in a GUI app |
| playback clock | frame / fps (lattice, `srt render`) | everything above, deterministically, when recording |
| audio sink | forge audio position | video streams with audio (master clock; the picture follows) |

Rules that follow, each learned the hard way:

- Animation reads the animation timeline, never the wall. A wall read inside
  a tick inherits the tick's execution jitter even when presents are
  metronomic; video frame selection on `Instant::now()` held and
  double-stepped 20 percent of frames on a clean 50 Hz cadence
  ([video-playback], frame scheduling).
- Timers read the timer timeline, never the animation one. Under slow
  frames the animation timeline used to lag by up to a second and every
  `setTimeout` fired that late ([timer-deadlines-lag-frame-timeline]).
  Timers stay frame-quantized (at least the delay, at most one frame late)
  and wall-accurate; a timer due during a background stretch fires on the
  resume frame, browser-style. The wake-at-earliest-deadline half is
  deliberately unbuilt: waking a saturated JS thread to run timer work is
  the backlog loop the tick gate exists to prevent.
- `performance.now()` is on neither timeline: real elapsed time, advancing
  through a paused dev clock. An app that times its animation off it under
  `srt render` plays at the wrong speed (Sponza feedback item 28 tried it).
- The dev clock control (`/clock?scale=`, `?step=`) gates frame delivery
  at scale 0, advances both timelines `period * scale` otherwise, and a
  step advances both exactly one period. Playback mode is the same idea
  taken to determinism: frame `k` is at `k / fps`, whatever the machine
  does.
- Transitions are stamped once per frame from the animation timeline. A
  target written before the first frame starts its track at clock 0 and
  fast-forwards the startup latency; open ([transition-clock-startup-anchor]).

## Decisions

### D1. The app timeline advances by counted display refreshes, not by wall samples

The tick an app animates from must be smooth at full rate and honest below
it. Four designs have been tried or considered, in this order:

1. **Raw wall time at present return.** Visible stutter with idle CPU and
   GPU: on Wayland/Mesa mailbox the swap return jitters by more than half a
   period, and an animation stepping by that jitter judders even though the
   display shows one frame per refresh.
2. **A JS-side moving average of wall time.** Masks the jitter without
   removing it; rejected.
3. **`round(dt / period)` whole-period quantization per frame.** Worse than
   raw: with noise over half a period a single sample cannot tell one late
   refresh from two early ones, so consecutive frames flipped between 0, 1
   and 2 periods.
4. **One period per present, slow-corrected toward wall time** (`GAIN`
   0.05, shipped 2026-06 to 2026-09). Metronomic at full rate, since the
   compositor shows one present per refresh and the present count is the
   steady signal even when the timestamp is not. It carried a hidden
   assumption: that the true cadence is one refresh per present. Below the
   refresh rate the timeline settled at a lag of `(interval - period) /
   GAIN`; below about 24 fps at 60 Hz that exceeds the 500 ms stall snap,
   so the model snapped and the app saw either a skipped half second or a
   hop, while running at a fraction of real speed in between. The timer
   split (D3) was the first casualty of this lag; the Sponza fly-through
   on a Pixel 7 at 21 fps was the second ([timer-deadlines-lag-frame-timeline],
   `~/solidrt/demoes/sponza/SOLIDRT-FEEDBACK.md` item 28).

The decision, 2026-09-21 ([frame-signal-refresh-count]): the platform
reports **how many refreshes passed since the previous frame signal**, and
the timeline advances by exactly that. This is the browser and Choreographer
model (a `requestAnimationFrame` timestamp is a vsync time; deltas below the
refresh rate are whole multiples of the period) and what Unity moved to in
2020.2, Godot ships as delta smoothing, and Croteam's frame-timing work
describes. Below the refresh rate the app sees the display's own cadence
(4-4-4-5 at 21 fps on 90 Hz); smoothing that further is the app's choice, as
it is in every browser, not the runtime's.

How the count is obtained without a presentation-timing source: a
cumulative drift estimator, `after = measured - counted - 1` where
`measured` is the reference instant's distance from an anchor in periods
and `counted` the refreshes reported so far; the signal counts as one
refresh unless `after` exceeds a tolerance either way, in which case it
counts as `1 + ceil(after - tolerance)` or as zero. The tolerance is 0.75
period, the same bound the missed-present accounting already assumes for
swap-return noise. Because the drift is measured against a cumulative
anchor, noise never accumulates, and because the band is wider than the
noise range, a full-rate app counts exactly 1 every signal with no
correction term, while a 2:1 cadence counts 2 every frame and a 4.3:1
cadence settles into 4-4-4-5 within two frames. Over any span the count
tracks wall time within 1.75 periods, which replaces the slow correction
entirely. The failure of design 3 does not recur: that design quantized one
sample; this one quantizes a cumulative measurement with hysteresis.

What the app timeline is therefore not: it is not a smoothing layer, it
carries no lag, and its long-run rate is wall rate by construction.

### D2. The three tiers, and where we stand

The industry-grade stack has three tiers. Tier 2 is the decision above;
this document is updated as the other two land.

1. **True presentation timestamps.** `wp_presentation` on Wayland, DXGI
   frame statistics or the sync-control extensions on ANGLE, `CVDisplayLink`
   on macOS, Choreographer frame times and
   `EGL_ANDROID_get_frame_timestamps` on Android. They turn the count into a
   measurement: the tolerance is never exercised and the estimator is
   only a fallback. Behind the same seam; per platform; open
   ([presentation-feedback]).
2. **An honest clock that advances by whole refreshes.** D1.
3. **Stable cadence selection** when the app cannot make the refresh rate:
   Android's Frame Pacing library (Swappy) is the reference. It holds a
   swap interval that is a whole number of refreshes, so a 21 fps app runs
   a metronomic 22.5 (every fourth refresh) instead of a 4/5 wobble. Policy,
   fed by the honest count; costs frame rate for regularity; open
   ([frame-driver-pacing-contract], stage 2).

### D3. Timers are wall-anchored; animation is not

See the clocks table. The two readings share the pause, step and scale
policy and differ in anchoring. Do not unify them: the animation reading
skips a suspension (one period across a background stretch) and the timer
reading lives through it. `okf/done/timer-deadlines-lag-frame-timeline.md`
has the measurements and the two regressions the split introduced and fixed
(cold-start early fire, stale deadline bases).

### D4. Pacing policy follows input modality

VsyncLocked for touch, SwapPaced for everything else, chosen in lattice from
alloy's `InputDevices` fact ([frame-pacing-fluency]). Facts in alloy,
policy in lattice; alloy never keys behavior on a device. The policy is
implicit today; making it enumerable and app-overridable is
[runtime-policy-registry].

### D5. Suspension: skip for animation, live through for timers

A gap over `SUSPEND_MS` (500 ms) between frame signals is a suspension (app
backgrounded, system stall), not a slow frame. The animation timeline
advances one period across it, so nothing replays time the app never lived
through; timers fire what came due. After D1 this threshold exists only in
lattice: alloy reports the honest count and lattice decides what a count
worth more than 500 ms means.

### D6. Playback is deterministic

`srt render` drives frame `k` at `k / fps`, delivers exactly one frame
signal per captured frame, bypasses pacing, and puts both timelines on the
same clock. Anything timing-related must keep a playback path that does not
touch the wall.

### D7. The present fence is two deep, unconditionally

One frame in flight overlaps draw with the compositor's present latency;
the driver queue's two to three ahead-of-glass frames are never reached.
The desktop input-to-photon cost of the second frame is accepted until
observed; an adaptive fallback is specified in
[adaptive-present-fence-depth]. On ANGLE/D3D11 the wait never blocks
([angle-present-fence-pacing]).

### D8. What the reference instant is, per signal

Until tier 1 lands, the reference instant is: the raster thread's
present-return instant for a SwapPaced or banked release; the vsync
release instant minus the armed pacing delay for a VsyncLocked release
(the delay is known, so the reference lands on the vsync rather than
delay-plus-wake later); the tick deadline for an idle Tick. The
Choreographer's `frameTimeNanos` is the app's wake-up time, one
`Display.getAppVsyncOffsetNanos()` after the true vsync
([choreographer-vsync-phase-offset]); when it becomes a reference it is
corrected at the source, as the video plane's sampler already does.

## Known limits and open items

- Presentation timestamps are modeled, not measured: [presentation-feedback].
- No cadence hold below the refresh rate: [frame-driver-pacing-contract].
- Idle `onFrame` on a SwapPaced desktop ticks at 2-3 Hz: [idle-onframe-tick-rate].
- A display mode change is not observed on Android: [android-refresh-rate-change-unobserved].
- The pacing budget samples the swap's throttle wait as pipeline cost, so
  the vsync delay sits at its floor: [pacing-budget-samples-swap-throttle].
- Transitions written before the first frame anchor at clock 0: [transition-clock-startup-anchor].
- The Android main loop rarely sleeps: [android-main-loop-rarely-sleeps].
- Frames carry no deadline, so an overrunning critical path wobbles between
  one and two refreshes instead of degrading to a stable cadence: the same
  [frame-driver-pacing-contract].

## Measuring

- On Android the only trustworthy present meter is `dumpsys SurfaceFlinger
  --latency` on the app's own layer, over 90 s or longer because drops
  cluster; `screenrecord` and the engine's fps stat both lie on the TV
  ([frame-pacing-fluency], [android-surface-swap-latency]).
- `atrace gfx view sched` puts the vsync ticks, the app's threads and the
  swap in one clock; the recipe is in [android-vsync-release-chain].
- In the client, `/stats` `missedPresents` over the window is the jank
  figure; `slowFrames` judges the JS critical path only; averages hide
  single frames ([stats-present-interval-jank]).
- A GPU counter that looks plausible can be wrong; measure saturation
  before building on a number ([tv-gpu-measurement-postmortem]).
- Probes that exist for this area: `probes/tick-reload-probe.tsx`,
  `probes/timer-deadline-probe.tsx`, `probes/onframe-probe.tsx`,
  `probes/vsync-cadence-probe.tsx`, `examples/spin/src/pacing.tsx`.

## Records this gathers

Done: [frame-pacing], [frame-pacing-fluency], [timer-deadlines-lag-frame-timeline],
[onframe-tick-reset-on-reload], [idle-tick-gpu-backlog-runaway],
[frame-production-capped-at-50hz], [choreographer-vsync-phase-offset],
[android-surface-swap-latency], [frame-batched-pointer-input].
Notes: [android-vsync-release-chain], [tv-gpu-measurement-postmortem].
Open: listed above.

[frame-pacing]: ../done/frame-pacing.md
[frame-pacing-fluency]: ../done/frame-pacing-fluency.md
[timer-deadlines-lag-frame-timeline]: ../done/timer-deadlines-lag-frame-timeline.md
[onframe-tick-reset-on-reload]: ../done/onframe-tick-reset-on-reload.md
[idle-tick-gpu-backlog-runaway]: ../done/idle-tick-gpu-backlog-runaway.md
[frame-production-capped-at-50hz]: ../done/frame-production-capped-at-50hz.md
[choreographer-vsync-phase-offset]: ../done/choreographer-vsync-phase-offset.md
[android-surface-swap-latency]: ../done/android-surface-swap-latency.md
[frame-batched-pointer-input]: ../done/frame-batched-pointer-input.md
[android-vsync-release-chain]: ../notes/android-vsync-release-chain.md
[tv-gpu-measurement-postmortem]: ../notes/tv-gpu-measurement-postmortem.md
[stats-present-interval-jank]: ../backlog/stats-present-interval-jank.md
[frame-driver-pacing-contract]: ../backlog/frame-driver-pacing-contract.md
[presentation-feedback]: ../backlog/presentation-feedback.md
[idle-onframe-tick-rate]: ../backlog/idle-onframe-tick-rate.md
[android-refresh-rate-change-unobserved]: ../backlog/android-refresh-rate-change-unobserved.md
[pacing-budget-samples-swap-throttle]: ../backlog/pacing-budget-samples-swap-throttle.md
[transition-clock-startup-anchor]: ../backlog/transition-clock-startup-anchor.md
[android-main-loop-rarely-sleeps]: ../backlog/android-main-loop-rarely-sleeps.md
[adaptive-present-fence-depth]: ../backlog/adaptive-present-fence-depth.md
[angle-present-fence-pacing]: ../backlog/angle-present-fence-pacing.md
[runtime-policy-registry]: ../backlog/runtime-policy-registry.md
[video-playback]: ../backlog/video-playback.md
[frame-signal-refresh-count]: ../plans/frame-signal-refresh-count.md
