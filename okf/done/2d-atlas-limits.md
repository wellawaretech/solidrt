---
title: A sprite layer draws one pre-packed atlas, fixed at creation
description: Every sprite in a layer samples one texture chosen at creation, and createAtlas only decodes an already-packed sheet, so a second sheet costs a second full-size render target and runtime-supplied images have no way in at all.
created: 2026-08-22
completed: 2026-10-06
---

# A sprite layer draws one pre-packed atlas, fixed at creation

Two symptoms, one root: the atlas was a single immutable texture, decided
before the layer exists.

**A second sheet was expensive out of proportion.** `createSpriteLayer`
took one `atlas` and built one pipeline target around it, so drawing from
two sheets meant two layers - and a layer is not a draw, it is a full-size
offscreen texture, its own render pass, and its own composited `<texture>`
leaf in the tree. What should cost nothing costs a second canvas. Apps
responded by cramming everything into one sheet, a real constraint on how
content is authored and a hard ceiling on texture size.

**Runtime images have no path in.** `createAtlas(bytes, ...)` decodes one
encoded sheet and slices it with `grid`/`namedFrames`. If the images
arrive while the app runs - downloaded, user-supplied, generated, decoded
from a document being edited - there is nothing to pack them into. That is
the normal case for a 2D-heavy application and the abnormal case for a
game, which is why v1 did not hit it. Still open, as its own item:
[2d-atlas-runtime-packing](../backlog/2d-atlas-runtime-packing.md).

## What landed (the first symptom)

Several atlases in one layer, bound as ONE draw, 2026-10-06:

- A `Frame` carries its texture (`{ texture, u0, v0, u1, v1 }`), stamped
  by `grid`/`namedFrames`/`fullFrame` over the `Atlas` record - what
  PixiJS (Texture = base texture + frame), Phaser (Frame.texture), Unity
  (Sprite = texture + rect) and Godot (AtlasTexture = atlas + region) all
  do. `AtlasSize` and `FULL_FRAME` went: a frame without a texture is not
  a frame.
- `createSpriteLayer(atlases: Atlas[], opts)`, the same for the record
  and tile layers and the `atlases` prop on `<SpriteLayer>`/`<TileLayer>`:
  up to `limits.maxTextureUnits` sheets (16 on every GLES 3.0 device), of
  any sizes and sampler states, validated at creation (empty, duplicate,
  past the cap: throw). `layer.atlases` exposes the list.
- The record stores the frame's texture as its index in that list (style
  record float 11, `STYLE_FLOATS` 12; record and tile records float 15,
  `FLOATS_PER_SPRITE` 16), and the fragment stage is generated per layer
  (`fragmentFor(count)` in shaders.ts) with one sampler per atlas, a flat
  per-instance index picking it. Derivatives are taken from the unclamped
  uv before the branch and the tap is `textureGrad`, so mip selection is
  the quad's own and well defined inside the branch - the two things the
  PixiJS/Phaser multi-texture batch (plain `texture()` in a branch) does
  not get right. The single-atlas layer skips the branch and keeps the
  same tap.
- A frame from a sheet the layer did not declare throws at the write and
  leaves the record as it was (every layer kind, the tile layer's frames
  table included).
- The tile layer's chunks moved from `createPipelineTexture` (a compile
  per chunk) onto the layer's one `createSpritePipeline` through
  `createShaderTarget`: one compile per layer.

## Why not the other routes

- A draw range per atlas (grouped by atlas, ordered within) is not
  expressible on GLES 3.0 - draw entries have no first-instance offset -
  and gives up interleaved depth across sheets, which `orderBy: "y"`
  needs for a crowd drawn from several sheets. Unity and Godot break
  their batch per texture instead, which costs draws.
- A texture array (`sampler2DArray`) removes the branch and the sampler
  cap but pads every sheet to one size and one sampler state, and needs
  a new core primitive. Padding a 256-square UI sheet to a 2048-square
  environment sheet is the wrong default for a package whose point is
  that sheets stay as authored.
