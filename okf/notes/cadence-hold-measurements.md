---
title: The cadence hold, measured
description: What the first cadence-hold controller was measured against on the Pixel 7, the SM-T500 tablet, the 50 Hz TV box and the laptop - the probe tables per busy level, why the up rule is measured and the step down predicted, the pacing-delay offset, the busy flag and standing-demand fixes, the pipelined estimate and its settle and miss-memory rules, Sponza on each device, and the measuring traps (census layer names, debug-command bodies, thermal drift).
created: 2026-11-05
---

# The cadence hold, measured

Cut from the findings of [cadence-hold](../done/cadence-hold.md) (2026-09-21).
The controller rules these readings shaped are replaced by
[cadence-hold-app-control](../done/cadence-hold-app-control.md); the
readings stand.

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
