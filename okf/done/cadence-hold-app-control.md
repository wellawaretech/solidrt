---
title: Cadence hold without stutters - raised only by sustained need, lowered only by proof or by the app
description: The automatic hold locks in on Android (a startup burst sets hold 3 on the SM-T500, 20 fps for the whole session, reloads included) because two slow frames raise it and its step-down predicts from work the hold itself inflates; raise it only when a share of the last presents needed it, lower it only on a measured fit (which reaches hold 1 only when forced, since the hold lowers the capacity it measures), never by a blind try, and give the app the say - set the policy, force a reset at a phase start and read the hold in force - at runtime, not through an environment variable.
created: 2026-11-05
---

# Cadence hold without stutters

The picture this fits into is [frame-timing](../design/frame-timing.md),
decision D9. The mechanism (the hold enforced by FrameRelease, the honest
refresh count, the policy command) is [cadence-hold](../done/cadence-hold.md) and
stays; this plan replaces its controller rules and its policy source.

## Problem

Reported from the tiles game on 0.0.85
(`demo/tiles/okf/upstream/android-cadence-hold-stuck.md`, the same lock-in
as [cadence-hold-dvfs-lock-in](cadence-hold-dvfs-lock-in.md)):

- On the Galaxy Tab A7 (SM-T500, 60 Hz) a fresh launch reads
  `cadenceHold: 3` on the idle menu within 3 s (a 453 ms first frame, then
  4 slow frames), and it is still 3 after 27 s with a JS p95 of 2.8 ms. With
  `SOLIDRT_CADENCE_HOLD=off` the same menu makes every refresh, 0 missed.
  Reloads do not clear it. A later screen mounting about 70 text rows trips
  it again.
- The Pixel 7 runs the tiles example's flight at hold 3 (30 fps at 90 Hz)
  with a 1.3 ms GPU frame, where a run that never tripped the rise ran at
  hold 1 with no misses.
- The app cannot choose. The policy follows the input modality (Auto on
  touch and remote clients, Off with a mouse), and the only override is the
  environment variable, read once at client start.

Why it stays:

- **The rise is too easy.** Two consecutive long intervals raise the hold,
  to the worst interval of the run. A startup burst or one heavy mount is
  enough.
- **The step-down cannot pass.** It predicts `offset + cpu + gpu + 2 ms`
  against the shorter slot. Under the hold the CPU term (signal to swap
  call) holds the buffer-queue wait the held display causes (9 to 25 ms on
  the Pixel against 2 to 6 at hold 1), the GPU term grows (29.2 ms at
  hold 3 against 14.1 unheld on the A7: a lower clock, see Findings), and
  the two terms overlap in the pipeline but are summed (21.5 ms of "work"
  on the A7 while it makes every 16.7 ms refresh).
- **Nothing else ends it** while frames keep coming: the idle reset needs a
  500 ms gap, and the controller outlives a reload.

## Decisions

- **No stutter the developer did not choose.** A hold the app runs under is
  never changed on a guess: no blind try at the shorter interval, not after
  a timeout, not after a minute. A failed try is a visible stutter; one
  every 30 s through an hour of rendered animation is still terrible.
- **The fix is at the rise.** A hold that only rises on sustained need has
  nothing to undo after a burst.
- **Down only on proof, unless forced.** The controller steps down only on a
  measured fit. Where the proof stays pessimistic under the hold, that is
  safe: it may keep a hold longer than needed, but it never steps down into
  a stutter. Forcing is the app's: `resetCadenceHold()` or a policy change.
- **The capacity the hold lowers is accepted.** Stage 1 (Findings) showed
  the held readings are honest work at a capacity the hold itself lowered:
  the A7's governor clocks the GPU down, the Pixel's scheduler moves the
  frame's threads to the little cores. No reading taken under the hold
  shows the capacity the lower hold would get, and nothing in reach
  recovers it (a remembered hold-1 reading goes stale with the scene;
  performance hints are not on the A7's Android 12). So the proof steps
  down as far as it reaches, which on both devices measured is hold 2,
  and hold 1 comes back by an idle gap, a reload, `resetCadenceHold()` or
  a policy change. The rise fix is what keeps the hold at 1 in the first
  place.
- **Reset at a phase start is the documented practice**, after the phase
  has loaded, not before its mount (a mount burst no longer raises the hold,
  so the reset's job is ending a hold from an earlier, heavier phase). The
  router does not reset on navigation.
- **The app's policy wins over the runtime default.** The environment
  variable stays for measurements only and wins over both.
- **No package.json key.** A `setCadenceHold` call at the app's top level
  runs before the first frame.

## Design

### 1. The controller (`alloy/src/cadence.rs`)

Up, on sustained need:

- The hold rises only when at least a share of the last N presents showed
  an interval longer than the hold (named constants, starting at about 30
  presents and a third).
- It rises to the typical interval of those long presents, not the worst,
  so a 453 ms first frame does not set the maximum. Never past the policy's
  maximum (50 ms).
- Under an unheld swap-paced chain the pipelined estimate `max(cpu, gpu)`
  still counts as evidence, under its existing miss memory.
- The settle rule stays: the presents right after a change are not
  evidence.

Down, on proof only:

- The predicate models the chain as it runs. The sum `offset + cpu + gpu`
  assumes frame N+1 starts after frame N's GPU work completes; alloy's
  chain does not wait for that: the swap returns at the queue, the GPU
  runs while the next frame's signal and UI work proceed, and the
  compositor latches the frame at the first vsync after its GPU work
  completes. A constant interval then needs throughput (each stage fits
  the slot) and a steady latency (the end-to-end latency rounds to the
  same vsync each frame). The A7 fits this: a steady 23.8 ms latency lands
  on the second vsync every time, a constant interval of 1 (96% of its
  frames complete after the next one has begun, see Findings). The hold
  steps down one refresh when every present of a whole window fits
  `max(cpu, gpu) + DOWN_MARGIN_MS <= (hold - 1) * period`. This is the
  model the rise's pipelined estimate already uses, so up and down judge
  from one model. The pacing delay a vsync-locked frame starts after is
  not in it: every frame starts at the same phase, so the delay is
  latency, not slot use (stage 5 on the Pixel: counting it held an empty
  frame at 45 fps that runs every refresh at 90). Latency jitter needs no check of its own: the
  measured rule catches it. The cadence.rs comment that "a held chain is
  sequential either way" is corrected.
- The terms stay as they are: stage 1 found no hold-caused wait in them
  (the fence wait is 0, the swap call is outside the CPU term, and the GPU
  term is already floored at the queue instant). Their growth under the
  hold is the lowered capacity, accepted above.
- A wrong step down stutters once. Inside its revert span (the settle
  plus one rise window), a step down is reverted by two consecutive long
  intervals, not the share: the step was the controller's own
  prediction, and a stream that misses 30% of its frames at the lower
  hold sits under the share for seconds. A reverted step raises a floor
  the step down never goes below again, until an idle gap, a reset or a
  policy change. This replaces the doubling down window, its maximum
  (`DOWN_WINDOW_MAX_MS`) and the backoff state, which retried a misjudged
  step a couple of times a minute.

A hold also ends on an idle gap (exists), a reload, a policy change and
`resetCadenceHold()`.

### 2. The app API (`@solidrt/core`, beside `setQuality`)

```ts
setCadenceHold("off" | "auto" | n | "default")  // applies at once, starts at hold 1
resetCadenceHold()                               // forced: hold 1 at once, policy unchanged
cadenceHold(): { policy: "off" | "auto" | number; hold: number }  // reactive
```

- `"default"` hands the choice back to the runtime (the input-modality
  default). `n` is a whole number of refreshes, at least 1. A bad value
  throws (dev).
- `cadenceHold()` reports the policy in force (the app's, the runtime's
  default, or the environment override) and the hold in refreshes (1 = no
  hold). It is a sticky event turned into a signal, like `pointerLocked()`,
  so a late reader sees the current state.
- Write path: `sol:app` exports (lattice's own module, beside `exit` and
  `background`) forward to the policy layers in lattice.
- Lattice keeps the three layers (`lattice/src/cadence.rs`,
  `CadencePolicy`): the runtime default (re-set on an input-device change,
  which reaches alloy only when the policy in force changes), the app's
  choice (which always reaches alloy, so a call starts unheld) and the
  environment override (set once at start); the effective policy is the
  override, else the app's choice, else the default. Alloy is told the
  policy in force (`SetCadenceHold`) and carries it out; it holds no
  layers. A device change no longer overwrites the app's choice, nor
  resets a learned hold.
- Read path: alloy emits an event when the effective policy or the hold
  changes; flux delivers it to core.

### 3. A reload starts clean

When lattice builds a new engine it clears the app's choice and resets the
controller, so the next app starts at hold 1 under the default.

### 4. `SOLIDRT_CADENCE_HOLD`

Kept for probes and censuses (`packages/cli/src/android/census.ts`). When
set it wins over the default and the app's choice, and an app call under it
logs that the override is in force.

### 5. App-developer docs

A section in `packages/core/agents/performance.md`, with its trigger line
in the core AGENTS.md index:

- What the hold is, that touch and remote clients get Auto by default, and
  what it trades (frames at the boundary for a steady cadence).
- Reset at a phase start, after loading.
- Choose the policy when the app knows itself: `"off"` for interactive
  gameplay that makes the refresh rate (tiles: off when gameplay starts,
  `"default"` back on the menu), `"auto"` or a fixed `n` for continuous
  motion (a cutscene, video).
- Read `cadenceHold()` for a debug HUD or to lower a phase's work when the
  hold rises.

## Stages

1. Measure (done, see Findings): the tiles flight on the A7 and the Pixel
   at `SOLIDRT_CADENCE_HOLD=1`, `2` and `3`, with the raster line's prep,
   GPU span, latency and overlap and the cadence line's CPU and GPU means
   (kept as debug diagnostics). It settled the predicate (overlap, so
   `max`) and found no hold-caused wait to take out, so the planned stage
   for that is dropped.
2. The controller rules and their tests (`alloy/src/tests/cadence.rs`): a
   burst does not raise the hold; a sustained need does, to its typical
   interval; no step down without proof; a step down when the `max`
   predicate fits where the sum did not; a reset, a policy change and an
   idle gap end the hold.
3. The policy layers in lattice, the commands, the event, the reload reset,
   the environment override; tests that the app's choice beats the default
   and survives an input-device change (`lattice/src/tests/cadence.rs`).
4. The core API, its `sol:app` exports and types, the docs.
5. Verification and the records.

## Done looks like

- On the SM-T500 and the Pixel 7, a fresh launch of
  `packages/2d/examples/tiles.tsx` and of the tiles game never leaves hold
  1 at the default policy, through a minute of flight and the text-heavy
  screen.
- Sponza's tour on the Pixel at a quality whose frame fits the maximum
  (low) settles at its hold and keeps it, and reads steady by eye; at
  medium its heavy views are past the maximum, where misses are counted
  and no hold can be steady.
- The step down by proof and the forced reset work, under a load a debug
  command (`registerDebug`) switches on and off, on the Pixel, the A7 and
  the TV box at the default policy: a CPU load (a JS busy loop of about
  40 ms a frame for 2 s) raises the hold by sustained need; once it is
  off, the hold steps down by proof as far as the proof reaches (hold 2
  on both phones, per Findings) within a few seconds and does not flap;
  `resetCadenceHold()` then returns it to 1, where it stays for a minute.
- Carried over from [cadence-hold](../done/cadence-hold.md): the user's eye
  test of Sponza on the desktop with the hold in force
  (`SOLIDRT_CADENCE_HOLD=auto`), and the TV box run, a 90 s census at 50 Hz
  under SwapPaced before and after, with no flapping.
- `setCadenceHold`, `resetCadenceHold` and `cadenceHold()` work on a
  running app (read back through `cadenceHold()` and `get_stats`).
- D9 in `okf/design/frame-timing.md` says "down only on proof, unless
  forced" and the app's say; the cadence entry in
  [runtime-policy-registry](../backlog/runtime-policy-registry.md) is
  updated; [cadence-hold-dvfs-lock-in](cadence-hold-dvfs-lock-in.md)
  moves to `done/`.

Closed 2026-11-05 with everything above verified (see the note) except the
desktop eye test and the tiles game's text-heavy screen, which were not run;
the desktop showed Sponza at medium past the maximum and at low unheld, so
it held no cadence to judge.

## Findings

Cut into [cadence-hold-capacity](../notes/cadence-hold-capacity.md).
