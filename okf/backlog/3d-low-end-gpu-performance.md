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
Measured with a since-removed floor-probe demo (a fullscreen d-rect plus N
armable constant-shader targets)
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
   3x3 PCF loop, 9 fetches per caster per fragment, and the taps are the
   single largest fragment cost measured here. The fix is already shaped in
   [gpu-depth-compare-sampling.md](gpu-depth-compare-sampling.md): ES 3.0's
   `sampler2DShadow` does the comparison in hardware at one LINEAR tap, four
   taps' worth of work in one, with better quality than the loop can reach.
   That item is the way to spend this budget; a plain tap-count knob is the
   fallback if it stalls.
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

- Philips TPM171E TV (Mali-T860, 1080p, displayScale 1), 2026-09-02,
  four-spot rig with 2048 maps: the demo's authored state runs 3.4 fps /
  290 ms - the MIN_RENDER_SCALE 1.5 supersampling floor alone costs
  167 ms (scale 1.0 is 123 ms), and the four 2048 shadow maps + taps
  cost 72 ms of that 123 (0 casters: 51 ms). Rendering and shadows are
  CORRECT on Mali (sampler2DShadow, atlas, spot cones all verified by
  snapshot) - it is purely a budget problem. The supersampling floor is
  the wrong default for the TV class, which is the render-scale policy
  question again: nothing distinguishes this device from a desktop
  (displayScale 1 both), and the attribution verdict does not trip on
  Mali either (timers armed). A per-pass-cost startup probe remains the
  candidate behavioral key.
- Shadow map resolution AT SPOT CONES is not the non-factor the ortho
  measurement above suggested: 1024 -> 2048 on four casters costs
  ~15 ms on the Adreno tablet and ~50 ms-class on the Mali TV (within
  the 72 ms shadow share), because a cone spreads texels so the map
  area is what fights the jaggies. Quality-first keeps 2048 (user call,
  2026-09-02); the TV pays it.
- Render scale, measured 2026-09-02 (four-spot rig, landscape): 1.5x is
  73.0 ms; 1.0x is 53.5 ms (~19.5 ms back, the largest single lever, as
  predicted) but visibly soft on the 1.5x display; 1.0x + 4x MSAA on the
  scene and the side atlas is 62.5 ms - MSAA costs ~9 ms here (the
  in-tile resolve is a sampling draw on this device), still 10.5 ms
  under native. MSAA on TOP of the 1.5x supersample is strictly wasted
  (90.6 ms portrait) - one AA mechanism at a time.
- A cap policy keyed on the timer attribution verdict was built and
  REVERTED the same day: the verdict is false on the Adreno 610 AND the
  Mali-T860 (both drivers attribute the synthetic pass pair honestly
  even where real frames misattribute), so the fact had zero true
  positives and shipped as speculative API. The shape that worked and
  can be rebuilt against a real fact: verdict as an Option on GpuLimits
  (fact only, filled beside PassTimer::new), marshalled as an absent-
  when-unmeasured limits field, policy as a @solidrt/3d
  defaultRenderScale() the fill-mode <Scene> consumes. Detecting
  "fill-bound tiler" behaviorally needs its own probe - a startup
  per-pass-cost ladder measuring the flat ~2.15 ms/pass signature is
  the candidate; vendor lists stay rejected.
- Re-measured 2026-09-02 with the shadow atlas (sub-targets, 6 passes ->
  3) and hardware depth-compare sampling (one tap per caster) both in:
  17.1 fps / 58.4 ms / 3 passes, against the 13.1 fps / 76.3 ms / 6
  passes above. Levers 2 and 3 are spent; render scale and the
  ground/backdrop shaders are what remains.
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
- Point shadows (six face tiles, landed 2026-09-02) measured on the same
  tablet with probes/point-shadow-probe.tsx (fullscreen room, 4 casters,
  4x MSAA, one casting point light at mapSize 512): 58.6 ms with the six
  face passes, 46.1 ms without - **~12.5 ms for a casting point light**,
  consistent with six passes at the ~2 ms flat per-pass cost above (the
  512-tile depth draws and the receiver's one extra tap are the small
  rest). The per-pass overhead dominates, so a future single-pass atlas
  render (all tiles of one target drawn in one pass with viewport
  switches, no per-tile FBO cycle) would cut most of it; a casting point
  light on this class of hardware is otherwise a ~budget-quarter item.
  GPU timer attribution stays unusable on this device (exec micros read
  0), so the number is an A/B of castShadow, not a timer read.
