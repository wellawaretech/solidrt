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

- **Pixel canvas**: a fixed logical resolution (e.g. 320x180) rendered
  small and displayed big with hard pixels. The platform path already
  exists - `filter: "nearest"` textures upscale hard everywhere (core
  gpu.ts documents this as THE retro path) - so this is a component:
  a sprite/baked layer at logical size, an integer scale factor chosen from
  the window size, letterboxed centering.
- **Palette LUT**: a fragment pass mapping the layer's output through an
  N-color palette texture. A `<texture>` post-chain via the layer's
  `output` prop, or a view/window `shader` - both exist; ship the shader
  source + a palette-texture helper.
- **Scanlines / CRT**: same mechanism, window `shader` for whole-app
  treatment or a per-layer pass; the classic barrel + mask + vignette
  shader is ~50 lines of GLSL.

Do these only after baked layers (okf/backlog/2d-baked-layers.md): the
flagship retro demo is a scrolling tile world, which tier 2 alone renders
the expensive way.

## What is actually package work here

Almost none of it. The palette LUT and the scanline/CRT pass are core
shader features - a view or window `shader` with a program and a texture
uniform - and the pixel canvas is a component doing arithmetic on the window
size. Nothing in this item extends the sprite layer, which is why it reads as
a demo kit rather than an engine stage.

The one exception is the **frame animation helper**,
`createAnimation(frames, fps)` stepping a sprite's frame. That is sprite-layer
work, it is a handful of lines, and every sprite population wants it whether
or not it is going for a retro look - so it is NOT gated on baked layers and
should not wait behind this item. Take it with the next package pass.
