---
title: Target damage measurements on the Adreno 610
description: What a partial render of a draw target costs against a full one on the SM-T500, measured with a tiled rig before the damage mechanism was built and with the built path after it - pass time follows the redrawn area with no large fixed cost for the no-clear pass, a damage render of one sprite's box costs 0.6 to 0.7 ms where the full pass costs 16.7 to 35.0 ms, the press frame sits 2.5 ms of GPU above an idle frame where a full pass sits 14 to 32 ms above it, and the tablet's window backbuffer is 4x multisampled, so the window side of damage saves nothing there.
created: 2026-11-11
---

# Target damage measurements on the Adreno 610

Step 1 of the damage work, measured on 2026-11-11 with no damage code in
alloy, and the built path, measured on 2026-11-12 (the last section).
The mechanism built after step 1 is
[gpu-target-damage-draw-lists](../done/gpu-target-damage-draw-lists.md);
what is left is [gpu-target-damage](../backlog/gpu-target-damage.md). The
probe and its bench: `probes/target-damage-probe.tsx`,
`probes/target-damage-bench.py`.

## The rig

- Device: Samsung SM-T500 (Adreno 610), release dev client.
- Target: 2000 x 1092 `rgba8`, single-sampled, 600 overlapping
  alpha-blended sprites (soft discs), one sprite pulsing. The pulsed box
  is about 14,000 px.
- Ballast: 8 cover draws on a 2048 x 2048 target, its pass 10.0 ms, so
  the GPU sits at one clock (the protocol of
  [gpu-subrect-draws](../done/gpu-subrect-draws.md)).
- Governor schedutil on both clusters, thermal status 0, battery 20.5 to
  21.0 C. Three press rows per mode; rows agree within 0.1 ms.
- PLAIN is one target rendered in full on every frame of the press.
  TILED splits the view into sub-targets, a culled list each, so a press
  dirties one tile and the parent renders partially (no pass-level
  clear, the changed tile wiped and redrawn). An 8 x 8 tile is about
  34,000 px.
- Two materials: a lighter and a heavier per-pixel cost, the probe's
  `work` knob at 4 and at 16. Calibration, PLAIN pass ms by work value:
  0 -> 10.7, 8 -> 22.8, 16 -> 35.0, 32 -> 60.5, 64 -> 111.5; linear, about
  1.5 ms per unit over a 10.7 ms base, so work 4 (not a calibration point)
  is the 16.7 ms pass below.

## Results

| mode | pass ms, lighter | GPU frame ms, lighter | pass ms, heavier | GPU frame ms, heavier |
|---|---|---|---|---|
| plain (full pass) | 16.7 | 31.8 | 35.0 | 50.1 |
| 2 x 2 tiles | 3.24 | 23.3 | 8.01 | 28.1 |
| 4 x 4 tiles | 1.08 | 21.1 | 2.33 | 22.3 |
| 8 x 8 tiles | 0.50 | 20.5 | 0.81 | 20.8 |
| plain again (drift check) | 16.7 | 31.8 | 35.0 | 50.2 |
| baseline, no press | none | 17.9 | none | 17.9 |

Wall clock per presented frame: plain 33.5 ms (lighter) and 50.0 ms
(heavier); 8 x 8 22.2 ms for both; baseline 18.6 ms.

## Findings

- Pass time follows the redrawn area: a quarter of the target costs
  about a quarter of the full pass. The no-clear pass has no large fixed
  cost on this tiler, so the whole-surface load the sub-target
  measurements suspected does not dominate a small partial render.
- The 8 x 8 press frame sits 2.6 ms of GPU above the baseline (2.9 ms
  heavier), the window composite of the full-window leaf included. A
  full pass sits 13.9 ms (lighter) to 32.2 ms (heavier) above it.
- Two caveats of the rig, in opposite directions. Each tile submits only
  its own records, where the shipped mechanism redraws the whole list
  under a scissor; at this population that is estimated, not measured,
  at about 0.2 ms more. An 8 x 8 tile is about 2.4 times the area of the
  pulsed box, so a damage rectangle redraws fewer pixels than the tile
  did.
- These are the tile rig's figures. The damage path as built is timed
  below.
- The device log shows the tablet's window backbuffer is 4x multisampled
  (resolved in tile at the swap). That is the window fast path, which
  never patches, so the window side of damage (a damaged target
  damaging only part of the leaf that shows it) would save nothing on
  this device.

## The built path (2026-11-12)

The same device, probe, ballast and materials, on a release dev client
built from the tree that holds the mechanism. Thermal status 0, battery
21.0 to 21.5 C, three press rows per mode, rows within 0.1 ms. PLAIN is
now one target whose press renders from rectangles; the force-full
switch (`/force-full-renders`) on the same build gives the full pass.

| mode | pass ms, lighter | GPU frame ms, lighter | pass ms, heavier | GPU frame ms, heavier | vertices per frame |
|---|---|---|---|---|---|
| plain, damage | 0.59 | 20.4 | 0.71 | 20.5 | 2400 |
| plain, force-full | 16.7 | 31.9 | 35.0 | 50.1 | 2400 |
| 8 x 8 tiles, damage | 0.37 | 20.3 | 0.49 | 20.4 | 80 |
| baseline, no press | none | 17.9 | none | 17.9 | none |

Wall clock per presented frame: plain with damage 21.8 ms for both
materials, plain forced full 33.5 ms (lighter) and 50.0 ms (heavier),
baseline 18.5 ms.

- A damage render of one sprite's box costs 0.59 to 0.71 ms where the
  full pass costs 16.7 to 35.0 ms. The press frame sits 2.5 ms of GPU
  above the baseline, the window composite of the full-window leaf
  included; that composite is the share the window side would save, and
  not on this device (the fast path, above).
- The whole list redrawn under the scissor (2400 vertices) costs 0.22 ms
  more than a tile's own list (80 vertices) for the same box: the
  estimate of the first run, now measured.
- A tile now renders its rectangle, not the whole tile: 0.37 to 0.49 ms
  where the tile rig read 0.50 to 0.81 ms.
- The forced full pass reproduces the first run's plain rows on the new
  build, so the two runs compare.
- Pixels: with the frame clock frozen, the target read back at three
  points of a press and after it is byte-identical with the switch off
  and on, for PLAIN (38 damage renders of about 10,600 px each against
  38 full renders) and for 4 x 4 tiles. The wipe and `glClear` store the
  same bytes on this driver, with the probe's inexact clear color.
- The scenarios damage must not move
  (`probes/record-layer-order-probe.tsx`, its four configs, three runs
  each): pan, churn and tint read the same on this build as on the
  client it replaced, in frames, JS time, GPU frame, pass time and list
  time, each within the run-to-run spread. The culled reveal is a burst
  of 47 frames with no ballast and its GPU frame swings by 4 ms between
  runs on both builds; alternating the switch over seven runs each gives
  a median GPU frame of 16.2 ms with damage and 16.1 ms forced full. A
  reveal renders 46 frames in full (more than 64 slots touched) and 2
  from rectangles of about 182,000 px.
