---
title: 2d sprites shade their whole frame quad, half of it empty for an irregular shape
description: Every sprite draws as the unit quad over its atlas frame, so a non-rectangular shape shades its transparent corners; a measured fill-bound pass spends about twice the shape's fragments, and a convex mesh per frame (8 vertices packed into the record, pulled by gl_VertexID in the layer's one instanced draw) would cut them by about a third - opt-in per atlas, pushed out in screen pixels for antialiasing and distance-field outlines.
created: 2026-11-06
completed: 2026-11-06
---

# 2d sprites shade their whole frame quad

## Symptom

A sprite is the unit quad (`aPos`, 4 vertices, instanced) over its atlas
frame. A shape that does not fill its frame shades every transparent
corner, and neighbouring quads overlap, so a board of irregular shapes
shades its screen about twice.

Measured on an Adreno 610 (SM-T500, display scale 1.5): a full-window
record-layer view of about 600 aperiodic tiles, 2.4 M target texels
(`rgba8`, single-sampled, no depth, alpha blend), each tile shape filling
49% of its frame quad.

| material | view pass GPU |
|---|---|
| stock `unlit()`, no `surface` | 17.5 ms (7.3 ms per million texels) |
| a `surface` with real per-pixel work | 35.5 ms |

The 2d tiles example on the same device draws at 3.5 ms per million
texels with no overlap: the stock pass costs 2.1x that, the quad-to-shape
ratio (2.06). The stock pass is fill-bound on this overdraw and nothing
else.

For that shape, from its outline:

| geometry | area over the shape's |
|---|---|
| frame quad (today) | 2.06x |
| tight bounding box | 1.93x |
| convex hull (72 points, unsimplified) | 1.29x |

The unsimplified hull removes 37% of the fragments, about 6.5 ms of the
stock pass. The 8-vertex mesh below and its antialiasing margin give some
of that back, so expect about a third. With a `surface` the saving
depends on what the corners cost, and they are not necessarily cheap: a
`surface` that samples or takes derivatives before a transparent pixel
returns (derivatives want uniform control flow) pays that work in every
empty corner. Estimates, not an A/B: there is no way to draw a sprite
with other geometry to measure one.

The bounding box barely helps, so atlas padding is not the cause; the
shape is.

## How other engines do it

| engine | tight geometry | how it draws |
|---|---|---|
| Unity | "Tight" sprite mesh, the import default: a concave mesh from alpha; a hand-drawn Custom Outline in the Sprite Editor | sprite batcher merges per-sprite vertices into a dynamic buffer on the CPU |
| Unreal Paper2D | render geometry modes: Full Rect, Tight Bounding Box, Shrink Wrapped (from alpha), Custom Polygon, Diced | CPU-merged batches |
| TexturePacker, cocos2d-x | polygon mode exports vertices and triangles per frame; cocos2d-x has `AutoPolygon` | CPU-merged batches |
| Spine | mesh attachments drawn by the artist, partly for fill rate | CPU-skinned and merged |
| Godot 4 | none automatic; an editor action converts a Sprite2D to a Polygon2D | rects batch, a polygon item draws on its own |
| PixiJS, Phaser, Bevy | none built in | instanced or batched quads |

Every engine with tight geometry makes the mesh per frame, from alpha, at
import. We take that half. We do not take the other half, merging
vertices on the CPU every frame: here that is JS per frame (or per-vertex
CPU work and upload in core), which the record layer exists to avoid, and
it breaks `orderBy`, a native gather over fixed-size records. The
instanced equivalent is a fixed vertex budget per instance, with the
vertex stage pulling its own vertex by `gl_VertexID`. It costs a few
degenerate vertices per instance (about 5k vertex invocations on the
measured board, against millions of fragments saved). What the CPU merge
has that this does not, a mesh of any size and custom stages that keep
reading a real `aPos`, both come back below as additions.

## Shape

- **A mesh on the frame.** A `Frame` carries an optional `mesh`: the
  polygon in frame-uv space (0..1 over the unflipped frame rect), plus
  the packed form the record takes. The convex hull is the first thing
  that fills it; a concave mesh, a TexturePacker polygon import or a
  hand-drawn outline are later sources of the same field, so they arrive
  additively. Frames stay self-describing, as frames.ts intends.
- **Made when the frame is cut.** `createAtlas(image, { mesh: "hull" })`
  keeps a coverage bitmask of the pixels (1 bit per texel, 512 KiB for a
  2048x2048 sheet), since frames do not exist yet when the atlas is
  created. `grid`, `namedFrames` and `fullFrame` over such an atlas fill
  `Frame.mesh` from the mask inside each rect. An atlas literal (a
  render target, a camera frame) has no mask and its frames no mesh.
  Opt-in because a material may draw outside the shape (a glow, a bevel,
  a drop shadow inside the frame) and a rectangular sprite gains nothing.
- **What the mesh hugs.** The covered texels as squares, padded half a
  texel for the bilinear reach. For an `sdf` atlas the covered texels
  are the edge (the true field at 0.5), not alpha above zero: the field
  is nonzero almost to the frame's border, so a mesh around it would
  save nothing. Everything that depends on the screen, not the texture,
  is the vertex stage's push (below).
- **The vertex budget.** Every instance in a layer runs the same budget,
  8 vertices. For a convex hull that is enough: an 8-gon around a circle,
  the worst case, covers 5.5% more than the circle (a 16-gon 1.3%), so a
  larger budget buys almost nothing. A hull over the budget is simplified
  outward to fit, never cutting into the shape; for a generated hull that
  is the normal path, not an error. A polygon supplied whole (a tool
  export or a hand-drawn outline, later sources) over the budget is
  simplified the same way and warns once, naming the frame. A shared
  budget costs only when it grows, which takes concave meshes (see the
  open questions), and then only where sprites are small enough that
  vertex work matters.
- **Packed into the record, not a table.** The 8 vertices are 16 bytes
  on an 8-bit lattice over the frame, a field in the record layout and
  the sprite layout of a layer whose atlases include a hull atlas. The
  record is what raw writers (`records(layer)` plus `updateRecords`) and
  the `orderBy` gather already move, so neither needs a second path; a
  table would need a per-instance row index, a sampler unit counted
  against `limits.maxTextureUnits` with the atlases, and row bookkeeping
  across a layer's atlases. The lattice is conservative: the polygon is
  offset by half a lattice diagonal before its vertices round, so
  rounding never cuts in. A zero field means the full quad, so a frame
  without a mesh, or a raw writer that has not written one, draws as
  today instead of drawing nothing. `setSprite` writes it from the frame;
  the raw path gets a helper beside `writeFrame`.
- **One draw, one strip for every instance.** The pipeline is a
  `triangle-strip`, so the 8 vertices go in zigzag order (0, 1, 7, 2, 6,
  3, 5, 4); unused ones collapse onto the last one, and degenerate
  triangles cost a vertex invocation and nothing else. A layer without a
  hull atlas keeps the 4-vertex quad and its record size: no cost for
  anyone who does not opt in.
- **Flips mirror the mesh.** A flip is stored by swapping the uv rect
  (`writeFrame`), so a mesh vertex at `aPos` p would sample the mirrored
  texel. The vertex stage mirrors each axis where the record's rect is
  reversed (`iUv.z < iUv.x`, likewise y). The mesh itself is
  flip-independent, so writing it ignores flips.
- **The screen push.** After the scale, in world pixels, the vertex
  stage pushes each vertex out along its miter by an antialiasing margin
  (a named constant, about a screen pixel: what mip blur and the sdf
  decode's half-pixel edge need) plus the sprite's outline
  (`vOutlinePx`). It reads the neighbours from the same record, so the
  miter costs no fetch. This answers both the per-sprite outline (a
  fixed mesh cannot know it) and minified sampling. Sharp tips make long
  thin miters; they are left uncapped.
- **Never outside the quad.** The push can leave the frame quad, and
  clamping vertices to the quad is wrong (a clamped convex polygon can
  cut a corner the unclamped one covered). Instead the fragment set of a
  hull layer zeroes coverage outside the frame rect (`vFrame`, already
  flat), so a hull layer never draws a pixel the quad would not.
- **Custom vertex stages.** Today they declare `in vec2 aPos`, an
  attribute bound to the quad buffer, which a pulled vertex never
  reaches. The layer prepends a `spriteCorner()` helper to the vertex
  source: on a quad layer it returns `aPos`, on a hull layer the pulled,
  mirrored and pushed vertex. The stock stage and `SPRITE_VERTEX_BODY`
  use it. A stage written against `spriteCorner()` works on both; one
  that declares `aPos` itself keeps working on quad layers and throws on
  a hull layer, naming `spriteCorner()`. The program cache already keys
  on the atlases, so the hull variant is one more key.

## Open questions

- Record encoding: 16 bytes as raw bit patterns in 4 floats (a uint32x4
  read; a per-element float assignment can canonicalize a NaN pattern,
  so writers must copy with a same-type typed-array `set`), or
  integer-valued floats, 3 bytes each, in 6 floats (no trap, 8 bytes
  more).
- Simplification: the outward 8-gon algorithm, and the area it gives up
  against the unsimplified 1.29x.
- Concave meshes (Unity's default) need no new mechanism: the same pull
  with a triangle list and a larger budget. Their cost is vertices per
  instance and triangulation, and they help where a convex hull does
  not (an L-shape, a ring, a sprite with a long limb). Later, through
  the same `Frame.mesh`.
- The coverage mask's lifetime: kept for the atlas's life so frames can
  be cut at any time, or dropped once the app says it is done slicing.
- Load cost. The mesh is built at runtime, when frames are cut: JS scans
  the coverage mask, which for a large sheet in QuickJS may be a visible
  load cost (unmeasured). Unity, Paper2D and TexturePacker build it
  offline, at import. Two ways out, not exclusive: a native scan (the
  mask and the frame rects in, the meshes out), which also serves
  atlases built from pixels in code; or a bake that stores the meshes
  with the asset, which removes even that. The bake is a consumer of
  [asset-build-stage](../backlog/asset-build-stage.md): that item's naming decision
  (the path the app wrote must still work) decides where baked meshes
  live. A bake also needs the frame rects, which today live in app code
  (a `grid` call); a sheet with an exported frame table (TexturePacker
  JSON) has them, a grid sheet does not.
- Tile layers bake through the same pipeline: a mesh there saves bake
  fill but the bake is not per frame, so it may not be worth applying.
- Text: large msdf glyph quads are padded by the field's range and
  overdraw the same way; `createSpriteFont` atlases are a later consumer.
- Hit testing stays the frame rect (pick is CPU-side over the rotated
  rect); whether a mesh atlas should also pick by the mesh is a separate
  decision.

Related: [2d-skeletal-sprites](../backlog/2d-skeletal-sprites.md) is the other path
where a sprite draws geometry other than its quad. It needs per-vertex
weights and its own pipeline; it shares the idea, not this path, and
nothing here forecloses it.

## Done looks like

1. A go/no-go gate before the rest: a throwaway branch where the record
   layer draws 8 vertices and a custom stage holds the tile's hull as a
   GLSL constant. A/B on the Adreno 610: the same board with quads and
   with the hull, stock material and a `surface` that works before its
   early return, view pass GPU per run under the same scripted drag.
2. `mesh: "hull"` on `createAtlas`, `Frame.mesh` from the slicers, the
   record field on the record and sprite layers (written by `setSprite`
   and the raw helper), the mirror, the screen push, the fragment
   frame mask, and `spriteCorner()`.
3. Tests: a hull layer renders the same image as a quad layer with the
   stock material, for flipped sprites, rotated and screen-clamped
   sprites, an `sdf` sprite with an outline, a mipmapped atlas drawn
   small and a `nearest` atlas drawn large. A headless test that every
   generated mesh contains every covered texel of its frame (the mesh
   math lives beside frames.ts, pure).
4. The 2d agents docs: when to opt in, what a `surface` costs in empty
   corners, and `spriteCorner()` for custom stages.

## Outcome

Built as shaped, with these deviations and decisions:

- **No gate first.** The go/no-go A/B was dropped as a precondition (the
  vertex side is about 5k invocations against millions of fragments) and
  run afterwards instead, on the SM-T500 (Adreno 610) with the cadence
  hold off: a scratch board of 600 irregular tiles (straight edges, each
  48.6% of its frame, so quads 2.06x the shape as on the measured
  board; the 8-point mesh is 1.23x) over a full-window record layer,
  2000 x 1200 target texels. View pass GPU per frame, two ABAB rounds
  each, skin about 24 C:

  | material | quad | hull | change |
  |---|---|---|---|
  | stock `unlit()` | 25.25 ms | 18.25 ms | -28% |
  | a `surface` with 4 taps before its early return | 59.5 ms | 41.95 ms | -30% |

  An earlier session on the same build read -19% and -24% (quad 25.95
  ms, hull 20.93 ms; GPU clock not readable without root); each session
  was stable within itself. An empty layer's pass is 0.55 ms, so the
  pass is fill alone. The frame-mask discard costs about 0.7 ms (hull
  without it: 20.2 ms against 20.9 ms).
- **Record encoding:** integer-valued floats, 3 bytes each, 6 floats
  (`MESH_FLOATS`), so a float copy keeps the field. A hull layer's record
  is 26 floats (records layer) or 22 (node layer's sprite record).
- **Rounding:** points round to the nearest lattice point in JS (the
  hull of the rounded points taken again, and a shape thinner than a
  cell gets its outward-rounded box); the conservative half-cell
  diagonal is a term of the vertex stage's push instead of a JS offset,
  which would have pushed points past the frame the lattice spans.
- **Simplification:** greedy outward edge removal (least added area,
  new corner inside the frame), the bounding box when no candidate is
  left.
- **Mip chains push further.** `createAtlas` records `mipmap` on the
  atlas, and a hull layer over a mipmapped atlas pushes by
  `MESH_MIP_EDGE_PX` (3 screen pixels) instead of `MESH_EDGE_PX` (1): a
  minified tap reads a coarser level whose blur reaches past the shape.
  The size sweep in tests/hull-meshes.test.tsx fails at 1 px.
- **`createAtlas` takes `sdf`**, since the mask needs to know the sheet
  is a field before frames exist.
- **`spriteCorner(size, zoom, outlinePx, uv)`**, and a vertex stage
  that reads `aPos` throws on every layer (not only on hull layers): the
  layer prepends the corner input, so one rule serves both variants.
  `SPRITE_VERTEX_BODY` maps the uv through a `unit` local.
- **The mask stays for the atlas's life.** Dropping it once slicing is
  done would be additive.
- **Load cost, measured.** QuickJS reads about 50 ns per texel whatever
  the loop shape, so a 2048-square mask is about 0.23 s on desktop flux;
  on the tablet a 2048-square hull atlas took 0.95 s (mask plus upload)
  and 256 frames of 128 px another 0.9 s; a 343 x 281 sheet 21 ms and
  5 ms. Accepted: a second or two at startup for a large sheet is fine.
  Native code is not an option for a 2d-only scan (only a generic
  spatial-core primitive would be); the way out is the bake, a consumer
  of [asset-build-stage](../backlog/asset-build-stage.md).
- Tile layers bake quads, hit testing stays the frame rect, and
  sprite-font atlases get no mesh; each open direction is a line in
  ideas.md (concave meshes, sprite-font meshes, picking by mesh).

