---
title: A tile is a frame and nothing else, so tinting a tile world means duplicating cells in the atlas
description: setTile takes a Frame or null; the baked chunk records already carry a tint at defaults, but neither the tile nor the layer exposes it, so day/night, damage states, team colours, biome shifts and fog-of-war dimming all fall back to atlas duplication or an overlay rect.
created: 2026-08-29
completed: 2026-08-31
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

## Findings

Landed 2026-08-31 (uncommitted), both levels, with one deliberate
deviation from the shape above.

- **Per tile**: `setTile(col, row, frame, { tint })` writes the record's
  tint floats and dirties the chunk like any cell write. Absent keys keep
  their values (a re-set keeps the cell's tint); a cell coming up from
  empty starts at `[1, 1, 1, 1]` - including one re-set after a clear,
  since a cleared cell only zeroes w/h and "absent cell has no tint"
  decides the reset. A clear (`frame: null`) ignores the options object
  entirely, per the open question: unwritten cells draw nothing.
- **Premultiply**: matched the sprite convention exactly - tint is
  straight `[r, g, b, a]` 0..1 multiplying the premultiplied sampled
  texel, same numbers, same semantics, same trap note.
- **Per layer**: NOT the composite-leaf route the shape sketched.
  `d-texture` has no tint (`TextureProps.color` contributes alpha only by
  its documented contract) and growing one is core work this item did not
  justify. Instead the shared fragment stage (shaders.ts) gained a
  `uTint` uniform multiplied over the per-instance `vTint`, and
  `setTint(rgba)` (plus the `tint` creation option and `<TileLayer>`
  prop) writes it with `setTargetParams` per resident chunk. That is
  still zero record work - the flush learned a render-only path (per-chunk
  `wrote` flag: tint and oversample re-bakes render without re-uploading
  records) - but it does re-render every resident chunk per write, so the
  docs say to drive it from slow state, not per frame. If a day/night app
  ever needs per-frame layer tint, the composite-side tint (d-texture
  growing one) is the escalation and stays additive.
- GLSL uniforms default to ZERO, so `uTint` is pinned to identity at all
  three pipeline creation sites (records.ts, layer.ts, tiles.ts) - the
  uCameraRot lesson again; a missed site renders transparent black.
- The records and node sprite layers got no tint surface (out of scope
  here), but the uniform is in their fragment stage now, so a layer-level
  sprite tint later is a params write away.

Verified live (temporary tint-probe.tsx, control-API raw snapshot, pixel
asserts): plain tile / sprite / records layers stay white (the identity
pin on all three pipelines), a delayed `setTint` (the dynamic
setTargetParams + re-render path) reads back exactly `[255, 128, 64]`,
and a per-tile tint multiplies under the layer tint to `[0, 128, 0]`.
