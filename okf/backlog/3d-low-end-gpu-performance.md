---
title: 3D fill and pass count put low-end Android GPUs far off 60 fps
description: The third-dimension demo runs at 13 fps on an Adreno 610 tablet. Measured budget: ~44 ms fragment work, ~13 ms of flat per-pass overhead, ~2 ms composite. The levers are per-pixel (shadow taps, render scale) plus one structural fix (shadow atlas); the compositing path, geometry, shadow map resolution and the stats overlay are all measured non-factors.
created: 2026-08-27
---

# 3D fill and pass count put low-end Android GPUs far off 60 fps

[packages/3d/demos/src/the-third-dimension.tsx](../../packages/3d/demos/src/the-third-dimension.tsx)
runs at 13.1 fps (76.3 ms/frame) on a Samsung SM-T500: Adreno 610, Android
12, GLES 3.2, 60 Hz, displayScale 1.5. Fullscreen the window is 1333x800
logical, 2000x1200 physical.

The workload is much larger than its description suggests. "One shape, three
shadows, three views" is really:

| target | pixels |
|---|---|
| scene | 1320x1200 = 1.58 M |
| top-right view | 680x600 = 0.41 M |
| bottom-right view | 680x600 = 0.41 M |
| **3D shaded per frame** | **2.40 M** |
| shadow maps (3x 1024²) | 3.15 M depth |

2.40 Mpx is more than 1080p (2.07 Mpx). A view is a full re-render of the
scene, not a cheap camera copy, so the view count and the caster count
multiply: every caster costs its shadow taps in every view. With the
backdrop and the alpha-blended ground overdrawing each other it comes to
roughly 4.4M fragment invocations per frame, and each lit fragment does
three lights of Lambert plus Blinn specular plus **27 dependent texture
fetches** (3 casters x a 3x3 PCF loop).

## Measured budget

Every line obtained by subtraction: change one variable, take the delta in
wall-clock frame time. Do not use the GPU timer figures - see
[gpu-timer-attribution.md](gpu-timer-attribution.md).

| component | ms | how isolated |
|---|---|---|
| Fragment work | ~44 | remainder |
| - shadow lookups in receiving shaders | ~15 | 9 taps to 1 tap saved 12.3 ms |
| - scene + view fill (backdrop, ground, knot) | ~17 | 0 casters, renderScale 1.5 to 0.15: 44.5 to 27.5 ms |
| - shadow map rasterization | ~7 | SHADOW_MAP 1024 to 128 |
| - shadow caster geometry | ~3 | knot 16,640 to 288 triangles |
| Per-pass overhead, 6 passes x 2.15 | ~13 | pass ladder, below |
| Baseline frame + present (vsync-capped) | ~17 | zero-pass probe hits 59 fps |
| Window composite | ~2 | zero-pass probe, below |

**Per-pass cost is flat at ~2.15 ms** regardless of size or content.
Measured with [floor-probe.tsx](../../packages/3d/demos/src/floor-probe.tsx)
by arming N 128x128 targets whose shader writes a constant, nothing else in
the frame:

| live passes | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| frame ms | 17.51 | 20.74 | 22.72 | 25.20 | 27.89 | 28.40 | 30.41 |

**The window composite is nearly free.** With zero GPU passes, the demo's
exact composite shape - three textures tiling 2000x1200 plus two text runs -
costs 1.9 ms over a plain full-window fill, and the device holds 59 fps
(vsync) doing it.

## Dead ends

Kept because each one looked convincing and cost real time. The pattern in
all of them: a plausible mechanism, confirmed by a counter that turned out
to be lying, never checked against wall-clock frame time.

1. **"The PCF taps are the bottleneck."** They are ~15 ms of a 76 ms frame.
   Cutting 9 taps to 1 moved the demo 14 to 17 fps - real, but 16 percent of
   the frame, not the answer. The tell was there immediately and was
   missed: turning casters off entirely bought +16 fps while removing 8 of 9
   taps bought +3, which cannot be true if taps dominate.
2. **"The window composite costs 22 ms."** It costs 1.9 ms. `gpuFrameExecMs`
   reported 8.4 / 11.5 / 18.6 / 22.2 ms for identical composite work, and
   401 ms on a frame with no passes at all. Acting on it would have meant
   rewriting the compositing path to fix a cost that was never there.
3. **"Per-pass overhead is ~10 ms."** It is 2.15 ms. The 10 ms figure came
   from varying caster count while fill and map size were still live, so it
   swept up work that had nothing to do with the pass.
4. **"Shadow map resolution matters."** 1024 to 128, a 64x reduction in
   area, buys 7 ms.
5. **"Geometry matters."** 16,640 to 288 triangles, 58x, buys 2.6 ms.
6. **"The stats overlay costs something."** Zero, measured twice.
7. **"cpuPct 112% means a CPU bottleneck."** This device idles at ~111%.
   App-side CPU is 0.22 ms of JS and 0.9 ms of command issue per frame.

Method that survived: divide a frame-counter delta by a `timeMs` delta over
a fixed window, vary exactly one thing, and drive the other variables to
zero before attributing anything. `fps` (a 1-second average) and `frameMs`
(an EMA) both mislead under bimodal frame times.

## What done looks like

60 fps is not reachable on this class of hardware with these shaders - even
a single view at native scale would land around 35-40 fps by the per-pixel
numbers. The target is that a low-end Android GPU lands in a usable bracket
rather than a slideshow, and that the per-pixel costs stop being invisible
to app authors.

Ranked by expected recovery, and by whether the fix is local to the demo or
belongs in the library:

1. **Cap render scale on Android** (~17 ms at stake; demo or policy). The
   demo renders 3D at displayScale 1.5. Games on this hardware render at
   720p and upscale. Needs deciding whether this is an app-level knob, a
   `@solidrt/3d` default, or a runtime policy keyed on the GPU.
2. **Cheaper shadow filtering** (~12 ms; library). `SHADOW` in
   [packages/3d/src/glsl.ts](../../packages/3d/src/glsl.ts) is a hand-rolled
   3x3 PCF loop, 9 fetches per caster per fragment. GLES 3.0 has
   `sampler2DShadow` with a hardware 2x2 comparison in one instruction. That
   is a better default everywhere, not only on weak hardware, and the tap
   count wants to be a knob rather than a constant.
3. **Shadow atlas, and fewer passes generally** (~6.5 ms; runtime plus
   library). Three casters means three passes, and the two view panels are
   two more. Both collapse to one pass each once a draw can name a
   sub-rectangle of its target, which is runtime work with several
   consumers and is written up separately in
   [gpu-subrect-draws.md](gpu-subrect-draws.md). The only structural fix
   here, it costs no visual quality, and it composes with
   [3d-shadow-cascades.md](3d-shadow-cascades.md), which would otherwise
   multiply the pass count again.
4. **The ground and backdrop fragment shaders** (part of the ~17 ms; demo).
   The top-down view costs 7.7 ms against the side view's 4.6 ms at
   identical pixel count, because the ground fills its frame - so the ground
   shader, not the knot, is where the per-pixel scene cost sits.

Explicitly not worth doing, with numbers above: composite path, geometry
reduction, shadow map resolution, the overlay.

## Findings

Appended during the work, per the rule in [../README.md](../README.md); cut
into `notes/` when this closes.

- Per-pass cost on Adreno 610 is ~2.15 ms flat, independent of target size
  (measured at 128x128) and of shader content. Pass count is therefore a
  first-class budget item on tiled mobile GPUs, not an implementation
  detail. Anything that turns one pass into several - per-light shadow
  targets, per-panel views, cascades - spends 2 ms a piece before it draws
  anything.
- The window compositing path is not a cost centre on this hardware:
  three full-size textures plus text is 1.9 ms, and a full-window fill
  sustains vsync.
- Pausing one panel's camera saves nothing while any shared scene state is
  animating, and this is correct behaviour rather than a gap in the
  demand-driven gating. A camera pose is only one of a view's inputs: the
  light rig turning re-renders the shadow maps, every material samples
  them, so every view redraws. Per-panel pause only pays once the scene
  itself is still. Measured: pausing either small view leaves 6 passes and
  12.9 fps; pausing the panel that gates the rig leaves 2 passes and 24.5
  fps; pausing everything leaves 0 passes and 51 fps.
- Ground-truth pass ladder for the demo, by pausing rather than by timers.
  Trustworthy where the GPU timers are not:

  | | frame | delta |
  |---|---|---|
  | composite only, 0 passes | 19.6 ms | |
  | + 2 view passes (680x600 each) | 40.9 ms | +21.2, so 10.6 ms per view |
  | + scene + 3 shadow passes | 77.9 ms | +37.0 |

  The timers claimed those two views cost 4.6 and 7.7 ms against a true
  21.2 total - understated by 40 percent. The idle row also cross-checks
  the probe's 18.86 ms for the same composite, reached independently.
- Real 3D shading on this GPU runs about **26 ms per megapixel** with the
  demo's shaders (three lights, Blinn specular, 27 shadow taps). That
  number is what any budget for this class of hardware has to start from:
  a 33 ms frame with a 19.6 ms floor leaves room for roughly 0.5 Mpx of
  shaded 3D, against the 2.40 Mpx the demo currently asks for.
