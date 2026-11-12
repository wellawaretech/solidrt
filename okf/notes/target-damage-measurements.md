---
title: Target damage measurements on the Adreno 610
description: What a partial render of a draw target costs against a full one on the SM-T500, measured with a tiled rig before the damage mechanism was built and with the built path after it - pass time follows the redrawn area with no large fixed cost for the no-clear pass, a damage render of one sprite's box costs 0.6 to 0.7 ms where the full pass costs 16.7 to 35.0 ms, the press frame sits 2.5 ms of GPU above an idle frame where a full pass sits 14 to 32 ms above it, and the tablet's window backbuffer is 4x multisampled, so the window side of damage saves nothing there; on a desktop client the window side takes a press from the whole window to 5,214 px a frame; on the multisampled window, three ways to declare the damage region to the driver were measured (a press frame on the tablet from 24.7 ms of GPU to 18.0, 19.9 and 19.4 ms, on a Mali-T860 TV from 4.2 to 1.3, 3.5 and 2.8 ms), and the patch drawn into a layer and copied into the window scissored to the patch ships (one layer kept between frames, at least the patch's size and bounded, where the measured variant used a layer of the patch's own size), since the fastest renders outside the region, which the extension leaves undefined.
created: 2026-11-11
---

# Target damage measurements on the Adreno 610

Step 1 of the damage work, measured on 2026-11-11 with no damage code in
alloy, the built path, measured on 2026-11-12, and the window side,
measured the same day on a desktop client (the last section).
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

## The window side (2026-11-12, desktop)

The window side maps a target's rectangles through the node showing it
([gpu-target-damage-window](../done/gpu-target-damage-window.md)). It
pays only where the window patches, which no Android device does (the
multisampled backbuffer), so it was measured on a desktop release
client: an RTX 3070 on Wayland, a 1280 x 720 window at scale 1, the
probe's PLAIN config with the view filling the window, a 3 s press read
over a 2.4 s stats window, three runs per row.

| mode | window damage per frame, px | partial presents per press | frames |
|---|---|---|---|
| damage, either material | 5,214 | 162 to 163 | 144 |
| force-full, either material | 921,600 | 0 | 143 to 144 |

- Before the change the damage rows read as the force-full rows do: the
  whole window, no partial present.
- The damage is the pulsed sprite's two boxes with their edges: 0.57% of
  the window.
- No time is quoted. The GPU frame time on this machine moved between
  0.65 and 17 ms within one mode (a desktop GPU shared with the
  compositor), which hides a composite of this size. The counters are
  the result; a time wants a desktop with an integrated GPU.

## The damage region on a multisampled window (2026-11-12)

The question the window side left open: every Android window is
multisampled, takes the fast path and never patched. Both devices here
offer `EGL_KHR_partial_update` and a buffer age on the app's context, so
an experiment declared the frame's damage to the driver
(`eglSetDamageRegionKHR`) after the age query and before the frame drew,
in three variants:

1. The region, then the same whole draw into the window.
2. The patch drawn through the window-sized rig, then one copy into the
   window scissored to the patch.
3. The patch, grown to whole 64 px steps of size, drawn into a layer of
   its own size, then copied into the window inside the patch. An empty
   patch draws nothing.

Release dev clients built from the experiment's tree, the probe's PLAIN
config, cadence hold off. GPU time of a frame, ms:

| device | frame | off | 1 | 2 | 3 |
|---|---|---|---|---|---|
| SM-T500 (Adreno 610, 2000 x 1200, 60 Hz), bench protocol, ballast of 12 draws | press | 24.7 | 18.0 | 19.9 | 19.4 |
| SM-T500, same | idle scene (baseline) | 22.3 | 15.7 | 16.8 | 15.0 |
| Mali-T860 TV (1920 x 1080, 50 Hz, Android 8), no ballast | press | 4.2 | 1.3 | 3.5 | 2.8 |

- The ballast holds the tablet's GPU clock in every mode, so its rows
  compare. The baseline's content change (the ballast target) shows in
  no node, so its frame is the window pass and the passes ahead of it.
- Variant 2 pays a window-sized offscreen pass for every patch; variant
  3's offscreen pass follows the patch, and on the tablet's idle scene it
  is the cheapest of the three.
- With the variable off the figures are those of a whole draw; the
  earlier run with only variant 1 read 24.8 to 24.9 ms off and 17.9 to
  18.0 ms on for the press, and missed presents fell from 49 to 51 a
  press to 14 to 16.

Pixels, clock frozen, the same stepped press of an off-centre sprite: a
screen capture in each variant equals one with the variable off on both
devices, but for one or two levels of 255 on a few antialiased edge
pixels of the dev client's status badge. These vary between runs of the
same variant. The badge is composited over the frame every frame; no
content pixel differs.

Decided:

- Variant 1 is not shipped, though it is the fastest. The extension says
  that rendering outside the region gives undefined contents and that
  confining the rendering is the client's job. The renderer always
  clears the whole surface it draws into, so the whole draw cannot be
  confined.
- Variant 2 is not shipped: it conforms, but its offscreen pass covers
  the window.
- Variant 3 ships (`WindowRoute::PatchLayer`, on by default where the
  window offers the region, `SOLIDRT_WINDOW_PATCH=off` the kill switch):
  nothing renders into the window outside the declared region. The
  shipped layer is not of the patch's own size: one layer is kept
  between frames, at least the stepped patch's size, its patch in the
  corner and its old pixels invalidated first; it is allocated again at
  the patch's size after 60 patch frames in a row over four times the
  patch's area. The copy into the window places the whole layer at the
  patch and is scissored to the patch. A patch
  over half the window after its steps draws whole; that threshold is a
  starting point, not measured on a device.

The shipped path, measured the same day on release dev clients built
from the tree that holds it, default against `SOLIDRT_WINDOW_PATCH=off`,
three press rows per mode:

| device | press GPU frame ms, off | press GPU frame ms, patches | idle-scene GPU frame ms, off | idle-scene GPU frame ms, patches |
|---|---|---|---|---|
| SM-T500, ballast 12 | 24.7 to 24.8 | 19.5 to 19.6 | 22.4 | 15.5 |
| TV, no ballast | 4.1 to 4.3 | 2.7 to 2.8 | not measured | not measured |

- On the tablet a press frame goes from 25.3 to 25.8 ms a frame with 47
  to 50 missed presents to 20.7 to 20.9 ms with 28 to 29; the idle-scene
  frame from 23.5 ms with 40 missed to 16.67 ms with none.
- It reads as variant 3 did: the kept layer, its invalidate and the
  scissored copy cost nothing measurable beside it.
- Pixels, clock frozen, a stepped press at the top left and at the
  bottom of the window, patches against the kill switch: identical on
  both devices, the badge's rounding aside.
- A press running in real time while the app is left and returned to
  twice (the recents key, then home and back): patches resume, no
  declaration is refused, and the screen after it equals the kill
  switch's on both devices.

Not measured: the size threshold, the cost of a small patch after a
window-shader frame has grown the shared rig to the window's size (its
clear then covers that storage), a pause that keeps the EGL surface
current (the sequence the tracker's distrust of an unpresented
declaration is for; whether the interruptions above kept or replaced
the surface was not checked), a buffer age
above the ones these two swapchains give, a video or camera frame
(latched after the damage is resolved, so its damage does not reach this
path), and any device beyond these two.
