---
title: A sprite layer has no display-scale story, so it is soft on a HiDPI screen
description: createSpriteLayer renders into a texture sized in the numbers the app passed, and the composited leaf scales that to its layout box, so on a 2x display the whole layer is upsampled unless the app doubles the size and the camera zoom itself.
created: 2026-08-22
---

# A sprite layer has no display-scale story, so it is soft on a HiDPI screen

What it looks like when you hit it: the layer looks right on the development
machine and blurry on a phone, a retina laptop or a 4K panel. Nothing errors,
nothing is measurably slower, the sprites are just soft - and softer than the
text and shapes drawn next to them, which the render tree rasterizes at
physical resolution.

Pixel-art content wants exactly this behaviour (render small, upscale hard),
which is why v1 did not notice. Everything else does not.

## Cause

`createSpriteLayer(width, height, atlas)` creates a pipeline target of
`width x height` texels and sets `uViewport` to the same numbers; the
resulting texture is composited by an ordinary `<texture>` leaf, which
stretches it to whatever layout box it lands in. Nothing between those two
points consults `displayScale()`. An app that wants crisp output has to size
the layer at `logical * displayScale()`, multiply the camera zoom by the
same factor, and redo both when the scale changes - and then remember that
its own layer-pixel coordinates are no longer logical pixels, which the
pointer path also has to account for.

This is the same contract every GPU target in the platform has (sizes are
texels, the app decides), and `@solidrt/3d` has it too. What makes it worth
settling here is that a sprite layer is not a manual GPU target an app opted
into pixel by pixel - it is a content surface that sits in the UI and is
expected to match it.

## Proposed shape

Decide, then document either way. The candidates:

- **Scale-aware by default**: the layer takes logical sizes, allocates
  `size * displayScale()` texels, folds the scale into the camera params
  (never into records), and re-sizes when the scale changes. Layer pixels
  stay logical, so app code, picking and the existing `handlersFor` scaling
  are unaffected. A `pixelated`/`scale: 1` opt-out preserves the pixel-art
  path.
- **Explicit, documented**: keep texels, and give the AGENTS.md traps section
  the recipe (size, camera zoom, and the resize hook) so nobody has to
  rediscover it.

The first is the better default for a package whose output composites into
the UI. Whichever wins should apply to `@solidrt/3d` as well, so this wants
deciding once rather than per package.

Open before implementing: whether `setSize` should keep taking texels while
the constructor takes logical units (it should not - one unit per API), and
what happens to a layer whose window moves between displays of different
scale mid-run.
