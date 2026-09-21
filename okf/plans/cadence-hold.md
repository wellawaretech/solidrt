---
title: Cadence hold - a steady whole-refresh present interval below the refresh rate
description: An app that cannot make the refresh rate is shown for an alternating number of refreshes per frame (3 and 4 on the Pixel 7 at 25 fps), which the eye reads as judder even though the timeline is now honest; hold the frame interval at a whole number of refreshes chosen from the measured frame work time with hysteresis, the way Android's Frame Pacing library does, trading frames at the boundary for a metronomic cadence. Tier 3 of okf/design/frame-timing.md; alloy measures and enforces, lattice decides when.
created: 2026-09-21
---

# Cadence hold - a steady whole-refresh present interval below the refresh rate

The picture this fits into is [frame-timing](../design/frame-timing.md),
decision D2 (the three tiers). Tier 2, the honest refresh count, landed
with [frame-signal-refresh-count](frame-signal-refresh-count.md) and is
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
variable read by lattice at startup, `SRT_CADENCE_HOLD=off|auto|<k>`, dev
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
  the same line as the pacing choice, honors `SRT_CADENCE_HOLD`.
- `/stats` exposes `cadenceHold`; `packages/cli/agents/debugging.md` says
  what it means and how to pin it.

## Stage 4: verification

On each device the probe is run unheld (`SRT_CADENCE_HOLD=off`) and held,
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
   exists: yes; the same placeholder shape `SRT_LOG` uses.

## Still open before this plan is done

- The user's eye test of Sponza's tour on the Pixel and the desktop with
  the hold in force (the desktop with `SRT_CADENCE_HOLD=auto`).
- The TV box run (50 Hz, SwapPaced, the client class the hold is most
  for): a 90 s census before and after, when the box is connected.

## Findings

- Verification with the final build, 2026-09-21, `probes/slow-frame-tick-probe.tsx`
  (tick deltas are the honest count; `missedPresents` per 15 s window):

  | device | busy | hold | deltas | fps | misses |
  | --- | --- | --- | --- | --- | --- |
  | Pixel 7, 90 Hz, VsyncLocked (Auto by policy) | 0 | 1 | 901 of 906 at 1 | 90 | 0 |
  | | 20 | 4 | 333 of 342 at 4 | 23 | 0 |
  | | 30 | 4 | 327 of 337 at 4 | 22 | 3 |
  | | 40 | 1 (over the 50 ms maximum) | 246 of 268 at 5 | 18 | 718 |
  | | 0 again | 1 | 1086 of 1092 at 1 | 91 | 0 |
  | laptop, 60 Hz, SwapPaced (hold forced on) | 0 | 1 | 600 of 600 at 1 | 60 | 0 |
  | | 20 | 2 | 448 of 450 at 2 | 30 | 0 |
  | | 30 | 2 | 449 of 449 at 2 | 30 | 0 |
  | | 40 | 3 | 299 of 300 at 3 | 20 | 0 |
  | | 0 again | 1 after 3, 2 | 595 of 648 at 1 | 61 | 0 |

  On the Pixel a 20 ms frame is held at four, not three: with the 6.7 ms
  pacing delay its slot use is 30 ms mean and 33 max against a 33.3 ms
  three-slot, and an earlier build that held it at three showed 114
  four-refresh intervals among 300 threes. Four is the honest steady
  answer for that workload; starting a held frame earlier in its slot
  (the follow-on under Not in this plan) is what would buy the third
  slot back. The step down after a heavy stretch takes one window per
  step (the laptop's return to full rate shows 21 threes and 31 twos in
  its first two seconds), as designed.

- The UI thread's busy fact belongs to the JS executor, not to lattice's
  batch loop: the frame verb posts its JS work to the flux engine's exec
  queue and the batch returns before the frame runs, so a flag set around
  the batch was already clear while a 30 ms frame was building. The first
  Pixel run with the batch flag showed it: idle Ticks kept firing (491 JS
  frames for 323 presents at `busy 30`), and the frame work times read
  20 ms and 0.8 ms for a 30 ms frame, because each Tick moved the
  emission instant the work is measured from. The flag now lives in
  `FluxEngine::run` (`FluxEngineBuilder::busy_flag`), set around each
  exec closure and its microtask checkpoint, and reaches alloy through
  `AlloyCommand::SetUiBusyFlag` beside the frame-request latch.
- Under VsyncLocked a held frame does not start at its slot's vsync but
  at the vsync plus the pacing delay (6.7 ms at 90 Hz, the input-arrival
  floor), so the controller counts that offset as slot use
  (`FrameRelease::slot_offset_ms`): 25 ms of work fits three slots from
  the slot start and needs four from 6.7 ms in.
- Prediction is not sound evidence for stepping up. With the first
  controller (need = ceil((offset + work + 0.25 period) / period) in both
  directions) the Pixel held an empty full-rate frame at 2 (45 fps, no
  misses): 6.7 ms of pacing delay plus 2.8 ms of margin leave 1.6 ms of a
  90 Hz period for work, and a frame that measurably fits one refresh
  (interval 1, no misses, for minutes) was still predicted not to. The
  same first run got the rest right (busy 30 held at 4 with 323 of 340
  deltas exactly four refreshes and zero misses; busy 20 at 3; busy 40 at
  43.9 ms over the 50 ms maximum, unheld). Hence the split: up on the
  measured interval, down on prediction with the budget's own 2 ms
  margin, and a backoff for reverted steps. The fit boundary under
  VsyncLocked is device-specific (SurfaceFlinger's latch point against
  the app's vsync phase), which is exactly why the prediction only ever
  proposes and the display disposes.
- The demand sample at present time was wrong for every animating app,
  and the stray Ticks had hidden it. The draw gate consumes the
  frame-request latch after the frame's JS has re-registered its
  `onFrame` callback, so at present time the latch read false for a
  standing animation and `demand_at_present` called every interval idle:
  with the busy flag in place (no Ticks re-setting the latch between the
  gate and the present) the Pixel reported zero misses at busy 30 with no
  hold, and the controller, which reads only demanded intervals, never
  saw one. Stage 2's miss counts (324 per 5 s at busy 40) had come out
  right only because the Tick-driven second frame re-set the latch. The
  fix: standing demand is declared by its sources and re-requested past
  the gate, the way running transitions already were. The web
  `requestAnimationFrame` queue is visible to the plugin; core's `onFrame`
  keeps its callback map in JS and requests through the `flux:rendertree`
  export, whose documented meaning was already "a pending callback is a
  standing request for the next frame", so that export now declares
  standing demand (`PlatformContext::declare_standing_demand`) and the
  gate consumes the declaration after the request. One-shot writes never
  come through it, so an idle gap still reads as idle.
- Under swap pacing the unheld chain pipelines and the interval hides the
  work: the laptop (60 Hz, hold forced on) showed a 21 ms frame as
  intervals of 1,1,2 (48 fps, 127 misses per 15 s, a stutter every third
  frame) and never two long intervals in a row, so the measured rule
  could not rise. The controller therefore also takes the pipelined
  estimate, `max(cpu, gpu)` plus the margin, as evidence for a rise when
  the chain is pipelined (`FrameRelease::pipelined`, false under
  VsyncLocked, which runs one frame at a time). Same run: busy 30 held at
  2 with 450 of 450 deltas exactly two refreshes and zero misses, and
  busy 40 fell back to unheld because the maximum, 50 ms at a 16.667 ms
  period, floored to two slots instead of three; the slot count now
  carries a hundredth of a slot of slack.
- The SurfaceFlinger census (`--latency <layer>`) returns only the period
  line on this Pixel's Android 17 build, for the SurfaceView layer, its
  BLAST child and the full `--list` line alike, so the phone evidence in
  this plan is the probe's tick deltas (the honest count, which the census
  agreed with earlier in the day) and `missedPresents`.
- Tablet (Samsung SM-T500, 60 Hz, VsyncLocked, Auto by policy), the
  probe with the same build as the Pixel table above: full rate 607 of
  607 deltas at 1; busy 20 held at 2 (308 of 431 at 2, with the rise
  going through 3 first while the work settled); busy 30 held at 3 (265
  of 292 at 3); busy 40 held at the 3-slot maximum with 212 of 223 at 4,
  its misses counted (the 42 ms frame plus the 8 ms delay does not fit
  50 ms); back at full rate it stepped 3, 2, 1 in two seconds. The Pixel
  in the same run stepped 4, 3, 2 after its heavy stretch and stuck at 2
  (46 fps) because the empty frame spikes to 8 ms now and then and the
  down window demanded that every present in a second predict a fit;
  the window now tolerates an isolated non-fit, as the up rule tolerates
  an isolated long interval, and closes on two in a row. Rerun with that
  build: busy 30 to full rate steps the Pixel 4, 3, 2, 1 within 3.3 s
  (969 of 1131 deltas at 1 over the 15 s window that starts with the
  steps) and the tablet 3, 2, 1 in 2 s.
- TV box (Philips TPM171E, Android 8, Mali-T860, 50 Hz, SwapPaced, Auto by
  policy; armeabi-v7a build), the probe: the first build held the empty
  frame at 2 (25 fps) and never came back, and the user saw it as fluent
  ("25 fps but it looks like 50"), which is the hold doing its job on the
  wrong workload. The controller's diagnostics line (debug level, once a
  second, plus the last eight presents' raw facts on every change) showed
  the down window completing, the step to 1, and a revert 80 ms later:
  at hold 1 the TV runs the frame at 50 fps with every interval 1, but the
  CPU side reads 19 ms (the present-fence wait of the pipelined chain) and
  the GPU timer 20 ms (the GPU waiting for a buffer inside the measured
  span) for a 7 ms frame, so the pipelined estimate said two slots and
  won. Two rules came out of it: a settle of three presents after any
  change (the two presents right after a change showed the transition,
  not the workload), and the pipelined estimate weighing in only within
  30 presents of a measured long interval. Final build, all three
  devices, the probe (deltas at the held interval / total; misses per
  15 s window):

  | device | busy 0 | busy 20 | busy 30 | busy 40 | back to 0 |
  | --- | --- | --- | --- | --- | --- |
  | TV, 50 Hz, SwapPaced | hold 1, 600/600 at 1, 50 fps | hold 2, 373/377, 2 misses | hold 2, 375/375 | hold 2 (max), 275 at 2 + 66 at 3, 68 misses | hold 1 within 1 s, 698/726 at 1 |
  | tablet, 60 Hz, VsyncLocked | hold 1, 608/608 | hold 2, 256/440 (rose through 3 while settling) | hold 3, 266/292 | (earlier run) hold 3 = max, 212/223 at 4 | hold 1 in 2 s, 774/828 |
  | Pixel 7, 90 Hz, VsyncLocked | hold 1, 915/920 | hold 3, 356/430, 24 misses (boundary) | hold 4, 313/335 | (earlier run) hold 4 = max, 251/269 at 5 | hold 1 in 3 s, 920/1106 |

  Sponza with the same build, hold on everywhere: desktop (60 Hz, forced
  Auto) holds at 3 for a GPU-bound 40 ms frame, a steady 20 fps with one
  miss per 15 s; the cold Pixel holds at 3 at 30 fps with two misses per
  15 s. On the tablet and the TV Sponza is not a cadence test at all: the
  scene pass takes about 1.1 s per frame on the Adreno 610 and 3 to 5 s
  on the Mali-T860, so both sit at their maximum hold with every frame a
  miss, as they should; the probe is the evidence for those two.
- Sponza (a 3d app) exposed two more things the probe could not. The
  frame timer measures the window draw only; a 3d scene renders as a pass
  before it, so `gpu_ms` read 9 ms for a frame whose GPU work was about
  40. The per-frame GPU time now sums the passes retired ahead of the
  window draw's query with the draw itself (one GL queue, issue order).
  And Sponza on the laptop sits on the 50 ms maximum (intervals of 3 and
  4 at 60 Hz), where "beyond the maximum runs unheld" flapped between
  unheld and three every few seconds; the hold now stays at the maximum,
  as in Swappy, and the longer intervals are misses.
- Measuring: the probe's debug commands take a bare JSON value as the
  body (`-d 30`), not an array (`[30]` reached the handler as an array
  and the busy loop ended at once, which read as a perfect full-rate run).
  On this Pixel (Android 17) the SurfaceFlinger census answers only for
  the buffer layer, named `SurfaceView[<package>/<activity>](BLAST)#<n>`
  in `--list`; the parent `SurfaceView[...]#<n>` returns no rows.
- A thermal note for anyone comparing Pixel frame rates across a session:
  after 25 minutes of Sponza the phone reported thermal status 1 and the
  GPU sat at about 350 MHz, and the same scene ran at 20 fps that had
  run at 50 to 70 when cold. Compare cold runs, or the same run's halves.
