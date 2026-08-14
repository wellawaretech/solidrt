---
title: No way to tile or repeat a texture
description: Textures always blit once into their destination rect, so a repeating background has to be faked with one element per tile or a shader bake; Impeller already exposes wrap-mode addressing that would make it a prop.
created: 2026-08-14
---

# No way to tile or repeat a texture

What it looks like when you hit it: you want a mosaic or patterned background
and there is no way to say "repeat this texture". The two workarounds an app
author is left with:

- place one `<texture>` per tile. Many draw calls against the same GPU
  resource, so it works and costs little GPU, but it is userland plumbing that
  has to be recomputed on every resize.
- hand-write a `createShader` that samples the tile through a bound `sampler2D`
  with `fract(vUV * N)`. Correct, but it bakes a full render pass into a
  destination texture: extra memory, extra pass, and a redo on resize. Strictly
  worse than letting the GPU wrap the sample.

Cause: `Texture::build`
([alloy/src/rendertree/kinds/texture.rs:114](../../alloy/src/rendertree/kinds/texture.rs))
always calls `builder.draw_texture_rect(..., sampling, Some(&paint))`, a direct
blit with no addressing mode. Nothing in the path can express wrap.

## The engine already has this

Impeller (`impellers = "0.4.2"`, the Flutter Impeller bindings alloy depends
on) exposes exactly the primitive:

- `TileMode` (`Clamp | Repeat | Mirror | Decal`, sys.rs)
- `ColorSource::new_image(image, horizontal_tile_mode, vertical_tile_mode, sampling, transformation)`
  (lib.rs ~1989) - builds a paint "shader" that samples an image with
  wrap-around addressing, the same idea as Skia's `SkImage::makeShader` or CSS
  `background-repeat`
- `Paint::set_color_source` attaches it to any filled rect or path

Backend-agnostic through Impeller, zero extra texture memory, zero extra render
pass.

## Proposed shape

A `tileMode` prop on the texture element (alloy side plus the binding in
[flux/src/plugins/gui/properties/texture.rs](../../flux/src/plugins/gui/properties/texture.rs))
that, when set, builds a Paint with
`ColorSource::new_image(entry.impeller, TileMode::Repeat, TileMode::Repeat, ...)`
and fills the destination rect through that paint instead of calling
`draw_texture_rect`. The `transformation` matrix on the image shader is what
controls tile size and offset: scale so the source texture maps to the desired
tile size, translate to scroll the pattern.

Open before implementing: trace how `d-rect`'s fill-with-paint path works so
the new path matches it, and confirm whether the existing rect-fill plumbing
can carry an arbitrary `ColorSource` or needs its own draw path. Naming is also
open - `tileMode` mirrors Impeller, `repeat` mirrors CSS; decide against the
rest of the texture props.

Source: root TODO.md, migrated 2026-08-14.
