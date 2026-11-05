---
title: Cadence hold - a steady whole-refresh present interval below the refresh rate
description: An app that cannot make the refresh rate is shown for an alternating number of refreshes per frame (3 and 4 on the Pixel 7 at 25 fps), which the eye reads as judder even though the timeline is now honest; hold the frame interval at a whole number of refreshes chosen from the measured frame work time with hysteresis, the way Android's Frame Pacing library does, trading frames at the boundary for a metronomic cadence. Tier 3 of okf/design/frame-timing.md; alloy measures and enforces, lattice decides when.
created: 2026-09-21
completed: 2026-11-05
---

# Cadence hold - a steady whole-refresh present interval below the refresh rate

The picture this fits into is [frame-timing](../design/frame-timing.md),
decision D2 (the three tiers). Tier 2, the honest refresh count, landed
with [frame-signal-refresh-count](../done/frame-signal-refresh-count.md) and is
what makes this tier measurable: the ledger now records how many refreshes
every present was shown for. This file is the work.

## Problem

Measured 2026-09-21 on the Pixel 7 (90 Hz, VsyncLocked) with
`probes/slow-frame-tick-probe.tsx` at `busy 40`: SurfaceFlinger shows the
app's frames for 3 refreshes 23 times and 4 refreshes 39 times per 10 s.
The refresh count reports exactly that, so the tour in Sponza moves the
right distance per frame, and the pace matches the desktop (confirmed by
eye the same day). What remains is the rhythm: consecutive frames cover
33 ms and 44 ms of motion in a pattern that never settles, and a 3/4
alternation reads as judder where a steady 4 (22.5 fps) reads as fluent.
The same shape on the laptop (60 Hz, SwapPaced, Sponza at about 22 fps):
2, 3 and 4 refreshes per present in one second.

This is a known trade in every frame-pacing stack. Android's Frame Pacing
library (Swappy) holds a swap interval that is a whole number of refreshes
and moves it only when the measured frame time has clearly crossed a
boundary; console engines lock to a divisor of the refresh; browsers do
not do it at all, because a page cannot be told to accept a lower rate.
SolidRT runs one known application per device, so the runtime can offer
the trade and the app can decline it.

## Design

**What is held.** The interval between frame signals, not the swap. The
frame that follows a present starts at the beginning of its slot (the
previous frame's start plus `hold` refresh periods), is built and presented
as soon as it is done, and the display shows it at the slot's end. Holding
the signal rather than the swap keeps the input the frame samples as fresh
as the slot allows; holding at the swap, which is where a swap wrapper like
Swappy must do it, builds early and waits with a stale frame. `hold = 1`
is today's behavior.

**How it is held, per pacing mode** (both in `FrameRelease`,
`alloy/src/vsync.rs`, which already owns the release decision):

- VsyncLocked: a present's signal releases on the first vsync whose instant
  is at least `hold` periods, less half a period, after the frame's start.
  Earlier vsync signals are skipped and the next callback re-armed; each
  skip costs one Choreographer callback, and the fallback deadline keeps
  its per-callback form, so a lost signal is still caught within a period.
- SwapPaced: a present whose slot has not ended is `Deferred` to a timer
  deadline instead of emitting at return; `wait_deadline` already puts the
  main loop to sleep until the deadline, and `on_wake` releases it. The
  blocking swap still paces the case where the frame outruns its slot.

The refresh count is unaffected: a held release is referenced at its
release instant, on the grid the hold defines, and counts `hold` refreshes
per frame. The app's `onFrame` sees a constant delta.

**The fact: frame work time** (alloy). Per present, the time the frame
actually needed, independent of any hold:

- `cpu_ms`: frame signal emission (main loop) to the swap call (raster
  thread). The emission instant is kept per in-flight frame in
  `FrameRelease` for both modes (today only VsyncLocked keeps it, for the
  pacing budget); the raster thread stamps the swap call and sends it with
  `FrameOutput::Presented`.
- `gpu_ms`: the GPU execution time of the most recently retired window
  draw from the timer queries the raster thread already runs (a frame or
  two behind the present it travels with, which a windowed controller
  does not mind). Without timer queries it is zero and the frame's cost
  is its CPU side alone, which already contains any present-fence wait;
  a lower bound, so the hold is then never worse than today.
- `work_ms = cpu_ms + gpu_ms`. Under a hold the chain is sequential (one
  frame in flight), so the sum is the frame's cost. The swap's own
  blocking time is deliberately not used: under a FIFO swap it contains
  the wait for the vsync the hold itself chose, and a controller fed its
  own hold runs away to the maximum.

`SignalRecord` gains `work_ms` and `hold`, so the ledger says for every
present what it cost and what interval was in force. The 1/s
`refresh count` debug line gains `hold k, work mean/max ms`, and `/stats`
carries `cadenceHold`, `frameWorkMeanMs` and `frameWorkMaxMs`.

**The controller** (alloy, new `cadence.rs`, a pure state machine tested
in `src/tests/cadence.rs`, Swappy's shape with one change of evidence, see
Findings):

- up, on measurement: the refresh interval the display actually showed
  the previous frame for (the honest count, reported by
  `RefreshCounting::count` for demanded intervals) exceeds the hold on
  `UP_FRAMES` (2) consecutive presents; the hold rises to the worst of
  the run. Not on one: a single dropped frame must not cost a step. A
  hold that is too short shows itself exactly this way, so no margin and
  no prediction is involved;
- down, on prediction: a hold that is too long shows nothing (every
  interval equals it), so the step down is predicted from the frame's
  slot use, `start_offset + work + DOWN_MARGIN_MS` fitting `hold - 1`
  periods, where the offset is the pacing delay after the vsync under
  VsyncLocked (zero under SwapPaced) and the margin is the pacing
  budget's own 2 ms headroom. One step at a time, after a whole
  `DOWN_WINDOW_MS` (1000 ms) of presents that all predict the fit, so a
  boundary workload settles on the higher side;
- a wrong step down is reverted by the measured rule as soon as two long
  intervals coincide, and each reverted step doubles the down window (to
  `DOWN_WINDOW_MAX_MS`, 32 s), so a workload the prediction misjudges
  costs a hitch a few times a minute at worst; a step that holds for its
  window restores the base window;
- the first `SETTLE_PRESENTS` (3) after any hold change are not evidence
  for a rise: the pipeline re-forms across a change (a held chain drains
  the swap queue, an unheld one refills it) and the first intervals show
  the transition;
- under a pipelined chain (swap pacing, unheld) the interval can hide the
  work behind a 1,1,2 pattern, so there the pipelined estimate
  `max(cpu, gpu)` also counts as evidence for a rise, but only within
  `MISS_MEMORY_PRESENTS` (30) of a measured long interval: without one
  there is nothing to fix, and a pipelined chain's CPU and GPU readings
  are stretched by its own fence and buffer waits (see Findings, the TV);
- bounded by the policy's maximum: a workload that needs more is held at
  the maximum and its longer intervals are misses (Swappy's rule).
  Dropping to unheld beyond the maximum was tried first and flapped on
  Sponza on the laptop, which sits on the 50 ms maximum and shows
  intervals on both sides of it;
- every change is logged with the interval or the predicted count that
  caused it, and the ledger records the hold per signal, so a census can
  say why the cadence changed.

**Presents that miss.** `RefreshCounting::count` today reports
`interval - 1` as missed presents for a demanded interval. With a hold in
force the expected interval is `hold`, so the rule becomes
`interval - hold` with the hold that was in force when the interval
opened. A held interval is by definition not a miss; a held app reads
`missedPresents` 0.

**The pacing budget.** `PacingBudget::record` discards samples over 1.5
periods as slipped frames. Under a hold of `k` a legitimate sample is up to
`k` periods, so the filter takes the slot length (`period * hold`); the
delay then sits at its floor for any frame longer than a period, and the
held frame starts at the first vsync of its slot. Starting it later in the
slot, as late as the budget allows so it samples fresher input (the second
half of what Swappy does), is a follow-on: it needs the release to pick a
vsync inside the slot from the budget, not only the slot's first.

**Policy** (lattice). A new command, `AlloyCommand::SetCadenceHold`, with

```
enum CadenceHold { Off, Auto { max_ms: u32 }, Fixed(u32) }
```

`Auto` is the controller with a maximum slot length (`MAX_HOLD_MS` 50: no
hold slower than 20 fps, Swappy's default too); `Fixed(k)` is a locked
interval for an app that knows its target (the "cap at 30 fps" a game
offers, honest for once) and for the probe. Lattice sends it beside
`SetFramePacing` from the `InputDevices` fact, re-evaluated on hotplug:

| client | fact | pacing | cadence hold (proposed default) |
| --- | --- | --- | --- |
| phone, tablet | touch | VsyncLocked | Auto |
| TV, set-top | no touch, no mouse | SwapPaced | Auto |
| desktop, laptop | mouse | SwapPaced | Off |

The reasoning for the desktop exception: under SwapPaced with the
two-deep present fence a desktop client pipelines (the next frame is
built while the previous one is on the GPU), so its natural throughput is
`1 / max(cpu, gpu)`, and a hold, which serializes the chain, costs it
`1 / (cpu + gpu)` on top of the boundary rounding. A 55 fps desktop app
held to 30 is a bad trade that no browser makes; a pointer-driven client is
also the one where cursor motion at the higher uneven rate beats the lower
even one. Touch clients already run one frame in flight (VsyncLocked), so
the hold costs them only the boundary rounding, and the TV's own fluency
hunt established that a saturated queue's regularity beats its frame rate
([frame-pacing-fluency](../done/frame-pacing-fluency.md)). The default is
one match arm in lattice, and it is the first thing the policy registry
([runtime-policy-registry](../backlog/runtime-policy-registry.md)) makes
app-overridable. This decision is the user's; see Open questions.

Until the registry exists, the developer override is an environment
variable read by lattice at startup, `SOLIDRT_CADENCE_HOLD=off|auto|<k>`, dev
only, so the probe and a census can pin the policy on any device.

The current hold is exposed in `/stats` as `cadenceHold` next to
`missedPresents`, read from a new atomic in `RasterStats` the main loop
writes.

## Stage 0: one frame signal per present (the double-signal fix)

[vsync-locked-js-bound-double-signal](../done/vsync-locked-js-bound-double-signal.md)
first, because the hold's controller reads frame work times and the hold's
mechanism assumes one signal per present; a JS-bound client under
VsyncLocked today gets two, and every cadence measurement on the Pixel is
distorted by it.

The missing fact is whether the UI thread is executing. Lattice knows: it
runs one batch at a time. It sets a flag around each batch, next to the
raster queue depth the idle gate already consults (`RasterStats`, shared
atomics), and the main loop's idle Tick is gated on both: no Tick while
the raster queue is non-empty or the UI thread is busy. The fallback
deadline that gives up the in-flight window stays for the lost-signal
case; the Tick that followed it while JS was still building is what goes.

Done: the probe at `busy 40` on the Pixel shows tick deltas of 3 and 4
periods only, in the census's proportions; one JS frame per compositor
present.

## Stage 1: the fact

- `FrameOutput::Presented` carries the swap-call instant (`ready_at`)
  beside `at` and `demanded`.
- `FrameRelease` keeps the emission instant for every in-flight frame in
  both pacing modes and hands `on_present` callers the `cpu_ms` of the
  present (None for a Tick-triggered present, which has no emission).
- `RasterStats` gains a per-frame GPU time the main loop can pair with
  the present; the fallback without timer queries is the fence wait.
- `SignalRecord` gains `work_ms: Option<f32>` and `hold: u32` (1 for now);
  the debug line gains the work columns.

Nothing user-visible changes. Verified by the debug line showing work
times that match the probe's `busy` setting plus raster cost, on the laptop
and the Pixel.

## Stage 2: the controller and the mechanism

- `alloy/src/cadence.rs`: `CadenceController` with `on_present(work_ms,
  period) -> Option<u32>` and the constants above; tests: steady work
  below a boundary holds; a two-frame excursion steps up and a window of
  fitting frames steps down one step; a one-frame stall does not step; a
  workload on a boundary settles high and does not flap over 1000 seeded
  jitter runs; the maximum is respected.
- `FrameRelease` gains `hold: u32` and the two release rules; tests in
  `release.rs`: VsyncLocked skips `hold - 1` signals and releases on the
  vsync at or after the slot end; SwapPaced defers to the slot deadline
  and releases on wake; a frame that outruns its slot releases at return
  and re-anchors the grid; a policy change flushes as `set_pacing` does.
- `RefreshCounting::count` takes the hold in force and reports
  `interval - hold`; tests updated (a held 3:1 cadence reports 0 missed, a
  4 under a hold of 3 reports 1).
- `PacingBudget` takes the slot length.
- `AlloyCommand::SetCadenceHold`, applied in the main loop; the ledger
  records hold changes; `RasterStats::cadence_hold`.

## Stage 3: the policy

- Lattice selects the default from `InputDevices` as tabled, logs it on
  the same line as the pacing choice, honors `SOLIDRT_CADENCE_HOLD`.
- `/stats` exposes `cadenceHold`; `packages/cli/agents/debugging.md` says
  what it means and how to pin it.

## Stage 4: verification

On each device the probe is run unheld (`SOLIDRT_CADENCE_HOLD=off`) and held,
at `busy` settings that sit on and between boundaries, and Sponza's tour
is judged by eye by the user, held against unheld:

- Pixel 7 (90 Hz, VsyncLocked): `busy 30` (a boundary case, 2.7 periods
  plus raster) and `busy 40`. Done looks like: one tick delta value, one
  interval in the SurfaceFlinger census, `missedPresents` 0, and the
  frame-rate cost recorded in Findings.
- Laptop (60 Hz, SwapPaced, Wayland mailbox): the same with the hold
  forced on, since the desktop default is Off; this is where
  `HOLD_MARGIN` is calibrated against the compositor's lead.
- TV box (50 Hz, SwapPaced) if it can be connected: the client class the
  hold is most for; a 90 s census before and after.

Each run's numbers go into Findings as they are taken.

## Not in this plan

- A pipelined hold (`max(cpu, gpu)` with two frames in flight, Swappy's
  auto-pipelining), which is what would make the hold a good default on
  desktop clients. Filed as a follow-on once the sequential hold has
  numbers.
- The app-facing policy surface: [runtime-policy-registry](../backlog/runtime-policy-registry.md).
  The env override is the placeholder.
- Presentation feedback per platform: [presentation-feedback](../backlog/presentation-feedback.md).
  The hold's grid and the count's grid are the estimator's; feedback would
  calibrate both.
- The idle desktop tick rate: [idle-onframe-tick-rate](../backlog/idle-onframe-tick-rate.md).

## Decisions taken (2026-09-21)

1. The default per client class: Auto for touch clients and for clients
   with neither touch nor mouse, Off where a mouse is present, for the
   reasons above. Auto everywhere is one line if the pipelined follow-on
   makes it the better answer later.
2. Stage 0 first: yes; every measurement on the Pixel depended on it.
3. The dev override as an environment variable until the policy registry
   exists: yes; the same placeholder shape `SOLIDRT_LOG` uses.

## Outcome

Built 2026-09-21 as staged. The user's eye test of Sponza's tour on the
Pixel with the hold in force passed (2026-11-05). The controller's rules
and the policy source are replaced by
[cadence-hold-app-control](../done/cadence-hold-app-control.md), after the
hold locked in on Android in the field; that plan also carries the two
checks left open here, the desktop eye test and the 50 Hz TV box census.

## Findings

Cut into [cadence-hold-measurements](../notes/cadence-hold-measurements.md).
