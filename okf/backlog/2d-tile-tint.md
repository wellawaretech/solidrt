---
title: A tile is a frame and nothing else, so tinting a tile world means duplicating cells in the atlas
description: setTile takes a Frame or null; the baked chunk records already carry a tint at defaults, but neither the tile nor the layer exposes it, so day/night, damage states, team colours, biome shifts and fog-of-war dimming all fall back to atlas duplication or an overlay rect.
created: 2026-08-29
---

# A tile is a frame and nothing else, so tinting a tile world means duplicating cells in the atlas

What it looks like when you hit it: a two-plane parallax (a far layer that
should read darker and desaturated under the near one) or any per-cell
state that is the same art at a different colour. The far-layer case has
a passable answer - a translucent `d-rect` over the layer, which is
arguably right for aerial perspective - but the per-cell cases (damage
states, team colours, biome blends, a fog-of-war dim on unexplored cells)
have none short of a variant cell per colour in the atlas.

## Cause

[tiles.ts](../../packages/2d/src/tiles.ts) `setTile(col, row, frame)` writes
the frame's UV rect into the chunk record and leaves the rest at defaults.
The chunk records ARE the full 13-float sprite layout with rot and tint
(see [2d-baked-layers](2d-baked-layers.md)), so the shader already
multiplies by a tint; the layer just never lets anyone set it.

## Shape

Two levels, cheapest first:

- **Per layer**: a `tint` option and `setTint(rgba)` on the primitive, a
  `tint` prop on `<TileLayer>`. This is NOT a re-bake: the composited
  `d-texture` leaves can carry it (a tint on the leaf, if `d-texture`
  grows one, or a uniform on the composite). Covers day/night and the
  parallax case with zero bake cost.
- **Per tile**: `setTile(col, row, frame, { tint })` writing the record's
  tint floats, dirtying the chunk like any other cell write. Same options
  object rule as everywhere: absent keys keep their values.

Open before implementing: whether per-tile tint should be premultiplied
like the atlas pixels (the records layer's convention decides; match it),
and whether a tint on an absent cell (`frame: null`) is meaningful (no:
unwritten cells draw nothing, and that is what makes sparse layers see
through).
