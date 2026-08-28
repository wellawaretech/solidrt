---
title: 2D layers render at layer resolution, so they are soft on HiDPI and shimmer at a fractional designSize fit
description: A sprite or tile layer draws into a texture sized in the numbers the app passed and is composited at whatever scale its box ends up at, with one sampler doing the whole resample - nearest snaps pixels to uneven widths and boils under motion, linear blurs. Render the layer at an integer oversample of its own resolution, composite linear.
created: 2026-08-22
---

# 2D layers render at layer resolution, so they are soft on HiDPI and shimmer at a fractional designSize fit

Two reports of one cause.

**HiDPI (2026-08-22):** the layer looks right on the development machine and
blurry on a phone, a retina laptop or a 4K panel. Nothing errors, nothing is
measurably slower, the sprites are just soft - and softer than the text and
shapes drawn next to them, which the render tree rasterizes at physical
resolution.

**Fractional fit (external feedback report, 2026-08-28):** a 320x200
`designSize` scene in a 1280x720 window fits at 3.6x. With `filter:
"nearest"` on the atlas and the tile chunks, one source pixel is drawn 3 or 4
device pixels wide depending on where it falls; run lengths across identical
features read 7 8 7 7 8, 32 33, 57 58. Standing still that is a shimmer:
every brick edge and glyph stem a different thickness. Scrolling, it boils:
each camera step changes the phase of every source pixel against the device
grid, so pixels flip between 3 and 4 wide as they move. `fps` reads 60 with
zero slow frames; it is geometry, not pacing. The report asked for an
integer-scaled `designSize` fit (hard pixels, letterboxed remainder). That
trades window space for exactness and is the wrong default: the scene should
fill the window it is given and a fractional scale should be resampled
properly, so that motion is fluent at any size. The integer fit is expressible
today anyway (a design-size view whose box is sized to `k * design` fits at
exactly `k`), so it needs no runtime feature.

## Cause

`createSpriteLayer(width, height, atlas)` creates a pipeline target of
`width x height` texels with `uViewport` at the same numbers; `createTileLayer`
bakes each chunk into a `chunkW x chunkH` texel target. Both are composited by
an ordinary leaf (`<texture>`, `d-texture`) that stretches the texture to
whatever box it lands in, and that stretch is one sampler: nearest or linear,
the target's `filter`. Nothing between the two points consults the device
scale of that box, whether it comes from `displayScale()`, from a `designSize`
fit, or from both.

One sampler cannot upsample pixel art at a fractional ratio: nearest picks a
whole texel per device pixel (uneven widths, phase boil), linear blends
neighbouring texels across every device pixel (the classic blur). Pixel art
wants the two-step resample: draw at an integer multiple of the source with
nearest, so texels stay square blocks, then scale that to the real box with
linear, which spreads the leftover fraction over a one-device-pixel band at
each block edge. Sharp inside, exact fractional coverage at the edges, fluent
sub-pixel motion, no wasted window.

## Proposed shape

An integer **oversample** factor per layer: the target holds `n * width x n *
height` texels while everything the app sees stays in layer pixels.
`uViewport` keeps `[width, height]` (the vertex stage maps layer pixels to
clip space, so a larger target only rasterizes finer), records, picking,
`handlersFor` and the camera are untouched, and the atlas is still sampled
nearest inside the layer. The target's own sampler becomes linear, which at an
exact 1:1 device mapping is identical to nearest and everywhere else is the
second step above. Texel cost is `n^2`; a 320x200 layer at n = 4 is 1280x800,
a full-window layer at n = 2 is the HiDPI cost every 2x screen pays already.

What is pixel-art specific in this is only the atlas sampler. Ordinary art
samples its atlas linear inside the layer, and oversampling then just means
"render at device resolution" - the HiDPI fix, nothing snaps to blocks.
Pixel art samples its atlas nearest inside the layer (`createAtlas(bytes, {
filter: "nearest" })`, which the reporting app already does), so texels
become square blocks at the oversampled resolution and the linear composite
does the fractional fit: each block edge is soft over about one device pixel
and the rest of the block is flat. Plain linear on the source would smear an
edge over one source texel, 3.6 device pixels at that fit - the blur pixel
art is known for; nearest alone is the uneven-widths shimmer above. One
mechanism, one sampler flag the app already sets. Two app-side habits go in
the docs with it: keep the camera fractional (rounding it to design pixels is
the reporting app's own 26-steps-a-second stutter), and a game that wants
period-correct whole-pixel scrolling snaps its own camera; the platform does
not impose it.

Staged, primitive first:

1. **The knob on both primitives.** `createSpriteLayer(w, h, atlas, {
   oversample })` and `createTileLayer(..., { oversample })`, default 1 (today's
   behaviour, unchanged for direct users), plus `setOversample(n)` on both
   handles: the sprite layer resizes its target in place (`setTargetSize` is
   id-stable, so `<texture src>` and bindings survive); the tile layer resizes
   every resident chunk target and marks all chunks dirty for one re-bake.
   Validation: positive integer, `n * size` within `maxTextureSize` (the tile
   layer already checks chunk size against it; the bound now includes n). The
   tile layer's `filter` option stays as an override but its default flips to
   linear, and its doc says nearest belongs to the atlas now. A named
   `MAX_OVERSAMPLE` caps runaway factors from a tiny layer in a huge box.
2. **The components pick n themselves.** `<SpriteLayer>` and `<TileLayer>`
   read their leaf's on-screen size in `onLayout` (`getBoundingBoxViewport` on
   the leaf, times `displayScale()`), take `n = ceil(device px / layer px)`,
   and call `setOversample` when it changes - resizes and display moves, so
   rarely. An explicit `oversample` prop pins it. Point to verify first: the
   window-relative box must compose the `designSize` fit (children of a
   design-size view live in design space); if it does not, that is a core
   fix before this stage, because a window-relative box that ignores the fit
   is wrong by its own definition.
3. **`@solidrt/3d`** gets the same knob and default once 1-2 have settled, so
   the decision is made once (the 3d layer composites the same way).

Not in scope: an integer-fit mode on `designSize` (see above; stays hardwired
contain + center, and the recipe for a hard-pixel canvas is docs), and
texel-edge anti-aliasing in the sprite shader for in-layer zoom (a fractional
`camera.zoom` inside the layer still samples the atlas nearest; oversampling
pushes that unevenness below a device pixel, which is enough until a report
says otherwise).

Verification: the 2d `tiles` and `sprites` examples in a window whose fit is
fractional (1280x720 over a 320x200 design), `/tree` showing the target at
`n x`, a snapshot of the leaf for even run lengths across a repeated feature,
and a scroll under `step_frames` for the boil. Looks are judged with the
human, not a pixel metric.

Open before implementing: whether `setSize` keeps taking layer pixels (it
should: one unit per API, the oversample is the only texel-side number), and
what a layer does when its window moves between displays of different scale
mid-run (stage 2's onLayout path covers it if the resize event fires there).
