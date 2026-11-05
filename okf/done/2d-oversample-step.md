---
title: The 2d auto oversample picks at a step, an eighth by default, not a whole number
description: The view oversample auto-pick rounded the display scale up to a whole number, so a 1.5x display shaded 1.78x its pixels; it now rounds to a multiple of an explicit `oversampleStep` (default 1/8, which every common display scale is), explicit oversample takes any positive number, pixel art sets the step to 1, and the tile bake density stays whole.
created: 2026-11-05
completed: 2026-11-05
---

# The 2d auto oversample picks at a step

## Symptom

A full-window record-layer view on a 1.5x Android tablet (Galaxy Tab A7,
Adreno 610, 2000x1200, window 1333x800 logical) picked oversample 2, the
ceiling of 1.5: a 2666x1600 target, 4.27 M texels for 2.4 M device pixels.
The composite then minified it 1.33x with no mipmaps, no sharper than 1:1.
Measured on the device with only the oversample switched: 176-183 ms per
pass at 2, 56 ms at 1. A line through those two points puts the display's
own 1.5 at about 106 ms. `setOversample` and `createView` rejected
anything but a positive integer, so the app could not ask for 1.5. The
same case was noted in [2d-tile-world-frame-hitches](../backlog/2d-tile-world-frame-hitches.md)
as headroom, not the cause of those hitches, and never filed on its own.

The whole number came from [2d-layer-display-scale](2d-layer-display-scale.md),
where it is right for one case only: a nearest atlas (pixel art) needs
each atlas texel to cover a whole number of target texels, or blocks come
out 1 or 2 texels wide, the shimmer that item fixed. For linear and
distance-field atlases nothing needs it, and it costs up to 1.78x the
fragments on the common Android scales (1.5, 2.625, 2.75).

## Decision

The pick rounds the measured scale up to a multiple of `oversampleStep`,
a prop on `<SpriteLayer>`, `<View2d>` and `<TileLayer>`, and the last
argument of `fitOversample`. The default is 1/8: every common display
scale (1.25, 1.5, 1.75, 2.625, 2.75, 3.5) is a multiple, so the target is
the device pixels and the composite is 1:1; elsewhere the pick is at most
an eighth over (a 3.6 designSize fit picks 3.625, was 4). Pixel art sets
`oversampleStep={1}` and gets the old behaviour. This matches Unity and
Godot, where render scale is fractional and integer scaling is the opt-in
for pixel art (URP render scale, Godot's integer stretch mode, Unity's
Pixel Perfect Camera).

Rejected:

- Deriving the step from the atlas filter (whole when any atlas is
  `filter: "nearest"`). Magic: the layer would infer intent from a sampler
  setting, and miss atlas literals, sprite fonts and binding-level filter
  overrides, since the Atlas record carries no filter. The step is the
  app's call, stated in AGENTS.md.
- The exact scale, no grid (the 3d `renderScale` fill's shape). The whole
  number also absorbed an animated measured scale: an ancestor scale tween
  from 1 to 2 re-picked once. An exact pick resizes the target on every
  layout of the tween and trips the thrash sentinel. At an eighth the same
  tween re-picks eight times, and a coarser step is the app's lever (the
  thrash warning names it).

## What landed

- `oversampleStep` on DensityProps, reactive, validated (positive number).
  `DEFAULT_OVERSAMPLE_STEP` and the step-aware fit and pick live in
  [oversample-math.ts](../../packages/2d/src/oversample-math.ts).
- Explicit `oversample` / `setOversample` take any finite positive number;
  the target is `scaledSize(size, n)`, rounded and at least one texel.
  `maxOversample` takes a fractional cap.
- The view's oversample follow has no shrink hysteresis any more: the
  pick is a pure function of the current box, step and bounds, with no
  memory of earlier picks. The step already quantizes it and float noise
  is the fit's epsilon; what is left (a measured scale oscillating on a
  step boundary, a breathing ancestor transform) re-picks on every swing,
  which the thrash sentinel reports with the explicit fixes (pin, a
  coarser step, a cap). A path-dependent hold also left targets
  oversized for good: a window moved from a 3x to a 1.6x display held 3.
- The tile density keeps its margin (`DENSITY_SHRINK_MARGIN`, 0.25 under
  the next whole number down), now a parameter of `pickOversample`
  rather than a shared constant: its wish is oversample times camera
  zoom, a live input, and a pinch hovering at a whole number would
  otherwise re-bake every resident chunk on each crossing.
- A dev diagnostic guards the default: `createAtlas` records `filter` on
  the Atlas (a plain fact, a literal may state it), and a sprite or
  record layer over a "nearest" atlas warns once ("Uneven pixel art")
  when one of its views takes a fractional oversample, naming
  `oversampleStep={1}`. It never changes the pick, the same pattern as
  the thrash sentinel. A tile layer does not warn: its bake density is
  whole, and its views sample the bake linear.
- A bound now overrides the margin, as the cap already did: lowering
  `maxPixels` or shrinking the window lowered the fit but not the
  measured scale, so the margin held the old factor until the scale
  itself moved.
- The tile layer's bake density stays a whole number (`checkDensity`,
  `DENSITY_STEP`): tile and chunk edges must land on whole texels, or an
  edge texel is half covered by two tiles (the classic tilemap seam), and
  a nearest tileset bakes square blocks. A tile VIEW samples the bake
  linear, so its own oversample is fractional at the default step even
  over a nearest tileset.

Verified headless: the pure fit and pick (every common scale picks
itself, a random sweep stays within a step above, an immediate shrink
with no margin at both steps, the density's margin, a budget and a cap
overriding it, validation), and on a test window at display scale 1.5 a
fill view picks 1.5 with its target equal to the device pixels, 2 at step
1, and shrinks back when the step returns to the default. The full 2d suite
passes. The pass saving on the tablet is the report's own estimate from
its two measured points, not a measurement of this change.
