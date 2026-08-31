---
title: Retro presets for @solidrt/2d
description: The pixel-art identity kit - fixed logical resolution with integer nearest scaling, palette LUT, and scanline/CRT passes - as thin layers over what already exists
created: 2026-08-19
---

# Retro presets for @solidrt/2d

Stage 3 of the 2D extension (okf/plans/2d-extension.md). "Retro" is an
aesthetic constraint, not an architecture: everything here is a thin preset
over existing machinery, and its value is identity (demos, docs, the
showcase) as much as function.

- **Pixel canvas**: a fixed logical resolution (e.g. 320x180) displayed
  big and sharp. With layer oversampling
  (okf/backlog/2d-layer-display-scale.md) this is nothing but `<view
  designSize>` around the layers: the fit fills the window at any ratio and
  the layers resample properly, so motion is fluent at every size. The
  hard-pixel variant (integer scale, letterboxed remainder) needs no feature
  either: size the design-size view's box to `k * design` and it fits at
  exactly `k` - a docs recipe, and at most a `fit` option on a canvas
  component, never a mode on `designSize`.
- **Palette LUT**: a fragment pass mapping the layer's output through an
  N-color palette texture. A `<texture>` post-chain via the layer's
  `output` prop, or a view/window `shader` - both exist; ship the shader
  source + a palette-texture helper.
- **Scanlines / CRT**: same mechanism, window `shader` for whole-app
  treatment or a per-layer pass; the classic barrel + mask + vignette
  shader is ~50 lines of GLSL.

The tile layer these build on ships (`createTileLayer` / `<TileLayer>`);
only its streaming-worlds stage B2 is still open in
okf/backlog/2d-baked-layers.md, and nothing here needs it.

## What is actually package work here

Almost none of it. The palette LUT and the scanline/CRT pass are core
shader features - a view or window `shader` with a program and a texture
uniform - and the pixel canvas is a component doing arithmetic on the window
size. Nothing in this item extends the sprite layer, which is why it reads as
a demo kit rather than an engine stage.

The one exception was the **frame animation helper**, which never belonged
here (sprite-layer work, wanted by every sprite population retro or not) and
shipped 2026-08-31: `createAnimation(frames, fps, { loop })` in
`packages/2d/src/animation.ts` - a clip with a shared wall-clock timer
stepping attached sprites, play/pause, one-shot hold + `onEnd`. See the
package AGENTS.md and `examples/anim.tsx`.
