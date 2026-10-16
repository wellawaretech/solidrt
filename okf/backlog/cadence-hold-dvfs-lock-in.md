---
title: The cadence hold locks in above 1 on Android because the step-down predicts from a CPU term the hold itself inflates
description: The tiles example's startup burst raises the hold (SM-T500 to 3, Pixel 7 to 4 then 3) and it stays there for minutes of flight the device could run at full rate (the Pixel at 30 fps with a 1.3 ms GPU frame); under the hold the controller's CPU term reads 9 to 25 ms against 3 to 5 at hold 1 because the span it measures (signal to swap call) holds the buffer-queue wait a held display causes, so the step-down prediction never fits; a step down tried on a timeout, reverted by the measured rule as today, would break the lock-in at one hitch per try.
created: 2026-10-16
---

# The cadence hold locks in above 1 on Android under DVFS

## Symptom

`packages/2d/examples/tiles.tsx`, default `Auto { max_ms: 50 }` policy,
Player.dev of 2026-10-16 with the re-bake fix:

- Pixel 7 (Mali-G710, 90 Hz): the startup bakes raise the hold,
  `cadence hold: 1 -> 4 refreshes (interval 4, work 25.3ms)` then `4 ->
  3 (interval 3, work 9.9ms)`, and the flight then runs at 30 fps (hold
  3) with a GPU frame of 1.3 to 1.7 ms, 11 to 13 missed presents per
  five seconds, for as long as it was watched. A relaunch that never
  tripped the rise ran the same flight at 90 fps, hold 1, zero misses
  for 75 s. The user saw both.
- SM-T500 (Adreno 610, 60 Hz), earlier the same day on the previous
  build: hold 3 (20 fps) for over three minutes after the burst; the
  day before it settled within a minute. On the fixed build the hold
  rose 150 s after launch (`1 -> 3 (interval 2, work 27.4ms)`), past
  the end of a two-minute reading that saw only hold 1, and stepped
  `3 -> 2` eight minutes later, where it stayed: a steady 30 fps.
- Left running side by side without a server, the Pixel then flapped
  (`3 -> 2`, ten seconds later `2 -> 4 (interval 5, work 34.5ms)`,
  `4 -> 3`, forty seconds later `3 -> 2`, and round again every half
  minute) while the tablet held 2 steadily, and the user read the
  tablet's steady 30 fps as the smoother of the two. So step-downs do
  come, after 40 s to 8 minutes, and a 35 to 39 ms frame at the lower
  hold (what it is was not measured; the Pixel at hold 1 ran 75 s with
  no miss) throws the hold back up: the cost of the lock-in is both the
  rate and the flapping.

The controller's diagnostics (`SOLIDRT_LOG=debug`, the 1/s `[alloy]
cadence:` line and the `recent (interval cpu+gpu @ms since previous
present)` tail of a hold-change line) say why it stays: at hold 4 on
the Pixel the CPU term reads 9.0, 9.6, 11.3, 13.2, 18.7, 25.3 ms with
the GPU at 1.0 to 1.9, where the same app at hold 1 reads 2 to 6 ms of
CPU. The CPU term spans the frame signal's emission to the swap call on
the raster thread, so it holds every wait the raster thread does before
and during the draw, and under a hold the display keeps each buffer for
three or four refreshes, so the buffer-queue throttle (dequeue inside
the first GL call of the draw) waits inside that span. The step down
predicts from `start_offset + cpu + gpu + DOWN_MARGIN_MS <= (hold - 1)
* period` over a whole DOWN_WINDOW_MS of presents, with two unfit
presents resetting the window; a CPU term swinging up to 25 ms never
gives a clean second, and nothing else moves the hold down: the measured
rule only raises it, and the idle reset needs a 500 ms gap no flying app
has. This is the mechanism okf/done/cadence-hold-sticky-android.md fixed
for the GPU term (the dequeue wait charged as GPU time), now in the CPU
term. The DVFS reading in this note's first draft (the tablet's GPU
frame 21.6 ms under the hold against 15.4 unheld) is probably the same
wait in that device's GPU term, not the clock; it was not separated.

## Leads

Two, and both may be right:

1. Take the wait out of the CPU term, as the sticky-cadence fix did for
   the GPU term: measure the frame's own CPU work (JS, layout, paint,
   submit, draw issue) and not the raster thread's fence or dequeue
   waits, which the hold itself causes (frame.rs records `wait_ms`,
   `draw_ms` and `present_ms` separately already; the dequeue throttle
   hides inside `draw_ms`, so it needs the frame timestamps or a fence
   around the first GL call to split out).
2. Try a step down on a timeout. The controller already treats a wrong
   step as cheap: the measured rule reverts it within two presents and
   each reverted step doubles the down window (up to
   DOWN_WINDOW_MAX_MS). A step tried without a fit prediction once the
   window has passed costs one hitch a few times a minute when wrong and
   ends the lock-in when right, since at the lower hold the waits
   shrink and the prediction fits. The Frame Pacing library's auto mode
   has the same shape.

## Done looks like

The tiles example on the Pixel 7 and the tablet at the Auto policy
settles to hold 1 within seconds of the startup burst and stays there
through a minute of flight; the TV box (50 Hz cap, saturated) does not
flap. The controller
test (alloy/src/tests/cadence.rs) covers the timed try and its revert.

## Involves

`alloy/src/cadence.rs` (the step-down rule), its test, and
`okf/design/frame-timing.md` (D9) for the decision.
