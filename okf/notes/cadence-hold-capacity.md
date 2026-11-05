---
title: The cadence hold under a lowered capacity, measured
description: What the second cadence-hold controller was measured against on the SM-T500, the Pixel 7, the 50 Hz TV box and the desktop - the held readings are honest work at a capacity the hold itself lowers (the A7's GPU clock, the Pixel's little cores), the chain overlaps under VsyncLocked, the pacing delay is latency and not slot use, the startup lock-in and its end, and Sponza past the 50 ms maximum at medium quality.
created: 2026-11-05
---

# The cadence hold under a lowered capacity, measured

The measurements behind
[cadence-hold-app-control](../done/cadence-hold-app-control.md): its
stage 1 (what the hold does to the terms the controller reads) and its
stage 5 (the controller and the app API on each device). The first
controller's measurements are [cadence-hold-measurements](cadence-hold-measurements.md).

### Stage 1 on the SM-T500 (2026-11-05)

`packages/2d/examples/tiles.tsx` (the flight), Player.dev from this
checkout, cold start per run, 20 s settle, 30 s capture, battery 23 C.
The raster 1/s line gained prep, the frame timestamps' GPU span, latency
(first command to rendering complete) and overlap (the next frame's first
command before this frame's completion); the cadence 1/s line gained the
CPU and GPU means; `/sys/class/kgsl/kgsl-3d0/gpubusy` (readable without
root, unlike the clock) gave the GPU busy share, sampled 1/s.

| `SOLIDRT_CADENCE_HOLD` | 1 | 2 | 3 |
|---|---|---|---|
| signals/s | 60.1 | 30.2 | 20.2 |
| CPU term (ms) | 6.5 | 9.1 | 9.9 |
| raster prep + wait + draw (ms) | 2.7 + 0.0 + 0.8 | 2.1 + 0.0 + 1.2 | 2.2 + 0.0 + 1.2 |
| swap call (ms) | 6.2 | 5.5 | 5.4 |
| GPU span, frame timestamps (ms) | 14.9 | 23.6 | 28.2 |
| GPU busy per frame, kgsl (ms) | 13.1 | 23.8 | 29.4 |
| GPU busy share | 79% | 72% | 59% |
| frames overlapping the next | 96%, by 7.3 ms | 24%, by 1.8 | 0% |
| latency (ms) | 23.8 | 30.8 | 35.4 |

- **The GPU term is the clock, not a wait.** At holds 2 and 3 the span
  matches the busy time per frame (23.6 against 23.8, 28.2 against 29.4),
  so it is execution; the same frame takes 13.1 ms busy at hold 1 and
  29.4 at hold 3, with the busy share falling from 79% to 59%: the
  governor lowers the clock as the hold lowers the load. On this device
  the proof from hold 2 to 1 cannot pass (`23.6 + 2 > 16.7`), as the
  decision "a clock the hold lowers is accepted" foresaw. From 3 to 2 it
  passes under the new predicate (`max(9.9 + offset, 28.2) + 2 <= 33.3`)
  where the sum failed, so a hold at 3 settles at 2.
- **The chain overlaps.** At hold 1, 96% of frames complete 7.3 ms after
  the next frame has begun on the raster thread, with a 23.8 ms latency
  against a 16.7 ms period, and the device makes every refresh (3
  multi-refresh signals in 30 s). The sum (21.4 ms) is the wrong model;
  `max` is the right one.
- **No hold-caused wait in this device's CPU term.** The raster thread's
  parts are flat across holds (fence wait 0, draw 0.8 to 1.2; the swap
  call, outside the term, 5.4 to 6.2). The term grows 3.4 ms on the UI
  side (6.5 - 3.5 against 9.9 - 3.4), likely the CPU clock as well; it is
  honest work at that clock. The dequeue wait the Pixel showed is a
  Pixel finding until the Pixel run separates it.
- **The span overstates execution unheld.** At hold 1 it reads 14.9 ms
  against 13.1 busy: a frame queued behind the previous one is charged
  from the previous completion, so the span is the completion spacing.
  Under a hold no frame is queued behind (0% overlap at hold 3), so the
  step down reads execution.
- **The lock-in reproduces.** At the Auto policy from a cold start the 4th
  present raised the hold to 3 (`interval 3, work 64.3ms`), and it held 3
  through the 60 s capture. The fixed-hold-1 run shows the device makes
  every refresh of the same flight unheld.

### Stage 1 on the Pixel 7 (2026-11-05)

The same runs at 90 Hz, skin 27 C. The Mali clock (`cur_freq`) and the
cpufreq policies are readable without root; utilization is not. A second
pass sampled the UI and raster threads' current core (`ps -T -o PSR`, 40
samples per hold).

| `SOLIDRT_CADENCE_HOLD` | 1 | 2 | 3 |
|---|---|---|---|
| signals/s | 89.9 | 44.9 | 29.8 |
| CPU term (ms) | 6.0 | 9.5 | 12.8 |
| raster prep + wait + draw (ms) | 1.5 + 0.0 + 1.3 | 2.2 + 0.0 + 2.1 | 2.7 + 0.0 + 2.7 |
| UI side: CPU term less raster (ms) | 3.2 | 5.2 | 7.4 |
| GPU span (ms) | 2.8 | 1.8 | 1.9 |
| GPU clock (MHz) | 204 | 204 | 204 |
| frames overlapping the next | 1% | 1% | 0% |

- **The CPU term is the cores, not a wait.** The fence wait is 0 at every
  hold and the growth is spread over every stage (UI 3.2 to 7.4 ms, prep
  1.5 to 2.7, draw 1.3 to 2.7), the signature of slower cores rather than
  a block. The thread samples show it: at hold 1 the raster thread ran on
  the mid or big cores in 26 of 40 samples and the UI thread in 26 of 40;
  at hold 3 the raster thread ran on the little cores in 33 of 40 and the
  UI thread in 31 of 40. The scheduler places a lighter load on the
  little cores, as the A7's governor lowers its GPU clock. The 9 to 25 ms
  the backlog item read at hold 4 came from a startup bake and did not
  reproduce in the flight.
- **The proof cannot pass the last step here either.** From 2 to 1 it
  needs `max(offset + 9.5, 1.8) + 2 <= 11.1`, and fails, where the device
  runs the flight at 90 fps with a 6.0 ms CPU term. From 3 to 2 it passes
  (`14.4 + offset <= 22.2`). A rise on this device would settle at 45 fps.
- **The GPU is idle at its floor clock**, a 2 ms frame, so nothing on the
  GPU side moves with the hold.
- **The Auto run did not lock in this time**: hold 1 for 60 s from a cold
  start. The rise depends on how the startup burst lands.

### What stage 1 means for the design

On both devices the held readings are honest work at a capacity the hold
itself lowered: the A7's GPU clock, the Pixel's core placement. Neither
showed a hold-caused wait in the terms the controller is fed (the fence
wait is 0, the swap call is outside the CPU term). So stage 2 has no
target in this workload. And no proof read under the hold can show the
capacity the lower hold would get: on both devices the last step down, to
hold 1, cannot pass, though both run the flight unheld at full rate. "Down
only on proof" then means "down to 2 at best, and to 1 only when forced".
That is open for a decision before stage 3.

### Stage 5 on the SM-T500 (2026-11-05)

Player.dev with stages 2 to 4, the default policy (Auto, touch).

- **The lock-in is gone.** Three cold launches of
  `packages/2d/examples/tiles.tsx`, 68 s each: no hold change in any (the
  policy line only), 3 to 9 multi-refresh signals per run. Before the
  change the same launch rose to hold 3 at the 4th present and stayed.
- **The load round trip** (`probes/cadence-hold-probe.tsx`, a marker per
  frame, a 40 ms JS busy loop per frame through its `load` command): the
  hold rose `1 -> 3` on the sustained need, and with the load off stepped
  `3 -> 2 -> 1` by proof, one window per step, no revert. The probe's
  frame is light, so the lowered capacity does not stop the proof here.
- **The API**: `"off"` kept hold 1 under the load; `2` held at 2;
  `"default"` returned to Auto, which rose under the load; `reset` went
  to 1 at once; `0`, `"fast"` and `2.5` threw. A reload after `"off"`
  came back at Auto (the app's choice ended with the app). Launched with
  `SOLIDRT_CADENCE_HOLD=off`, the policy read `off` and an app call of `2`
  left it there, logged as waiting under the override.

### Stage 5 on the Pixel 7 and the TV box (2026-11-05)

- **Pixel, the lock-in**: three cold launches of the tiles flight at the
  default policy, 64 s each: no hold change, 6 to 7 multi-refresh signals
  per run.
- **TV box** (TPM171E, 50 Hz, SwapPaced, Auto by default; armeabi-v7a), the
  probe in three 90 s phases: no load, hold 1 at 49.7 signals/s with no
  multi-refresh signal; a 20 ms load, one rise to 2 and a steady 25 fps
  with no further change; the load off, one step back to 1. The
  compositor's census under the load: 126 of 126 presents at 2 refreshes.
  No flapping.
- **Pixel, the load round trip**: the probe rose `1 -> 4` (the maximum at
  90 Hz) under a 40 ms load and stepped `4 -> 3 -> 2` by proof once it
  was off, then stopped at 2 (45 fps) for an empty frame. The offset
  stops it: the pacing delay sits at its input-arrival floor (6.7 ms at
  90 Hz, at every hold), and `6.7 + cpu + 2` passes 11.1 ms for any CPU
  term over 2.4 ms. Pinned at hold 1 the same probe made 1896 of 1897
  refreshes with a 3.9 ms CPU term (stage 1 measured the tiles flight the
  same way: 6.0 ms of CPU from 6.7 ms into the slot, every refresh made).
  The chain overlaps, so the delay is latency, not slot use.
- **Sponza on the Pixel** (`probes/sponza-compare`, re-baked with today's
  tool; it gained `quality` and `reset` debug commands): at the default
  quality (medium) the tour needs 50 to 55 ms of GPU per frame at full
  clock in its heavy views, past the 50 ms maximum, so no policy holds it:
  with the hold off every interval is 4 refreshes or more (18 fps), with
  it on the hold sits at the 4-refresh cap and swings 3/4 with the views,
  the heavy frames shown for 5 to 7 refreshes. The September standalone
  build (before quality tiers and the hold) runs the same scene at about
  35 fps; "medium" on this phone is a quality-defaults question, not this
  plan's. At low quality with the hold off the tour runs 45 to 56 fps
  uneven in its heavy stretches and 90 in its light ones; with the hold
  on (after the switch to low and a reset) it settles at 2, a steady 45
  fps, and keeps it; the user judged that good by eye.
