---
title: Extrude atlas cells into gutters so a mipmapped sheet does not bleed
description: The layer shaders clamp samples into their frame, which stops edge bleed at mip level 0, but a mip chain averages blocks that straddle cell edges before any sampling decision; the fix is a load-time repack that copies each cell into a gutter of replicated edge pixels, the thing Unity's packer and TexturePacker call extrude.
created: 2026-09-07
completed: 2026-10-06
---

# Extrude atlas cells into gutters

## Symptom

A sheet with cells that touch draws clean at mip level 0: the sprite and
tile fragment stage (`packages/2d/src/shaders.ts`) clamps every sample
into its frame, pulled in half a texel, so neither a linear tap nor float
interpolation reaches the cell next door. Turn on `mipmap` for that atlas
(a tileset drawn far below its texel size under a zooming camera) and the
neighbour comes back: mip level k is built by averaging 2^k texel blocks
of the level above, and a block on a cell edge mixes both cells before
the shader gets a say. No clamp, inset or spacing on the UV side can undo
an average that already happened in the texture.

`grid`/`namedFrames` `inset` was the stopgap: shaving 2^k texels keeps
level k inside the cell, at the cost of losing that border of every
sprite. A transparent gutter (`spacing`) was the other one, and it bleeds
transparent instead of the neighbour, which is a fade at the edge rather
than a flash.

## What landed

Both sheet forms, as pure modules (`packages/2d/src/extrude.ts` over
`packages/2d/src/pack.ts`), headless-tested in `tests/extrude.test.ts`
and `tests/pack.test.ts`, GPU-pinned in `tests/atlases.test.tsx` (an
extruded, mipmapped sheet drawn 2 px wide samples pure red at a cell edge
where the plain sheet's chain mixes the green neighbour in):

- `extrudeGrid(image, cols, rows, gutter, source?: GridOptions)` returns
  `{ image, options }`: the repacked sheet (`cell + 2 * gutter` per cell
  per axis) and the GridOptions that slice it (`marginX/Y = gutter`,
  `spacing = 2 * gutter`, the cell size explicit).
- `extrudeRects(image, rects, gutter)` returns `{ image, rects }`: the
  rects re-placed by a shelf packer (next-fit decreasing height,
  `packRects` over `packWidth`, a power-of-two width) and the new rect
  table for `namedFrames`.
- Validation (throws): `gutter` a power of two >= 1, every cell or rect
  size a multiple of it. The reason is alignment: with cell edges on
  multiples of g, the level-k texel block on either side of an edge lies
  inside the cell's own replicated colours for every k <= log2(g); an
  unaligned edge halves the clean depth for the same gutter, so the
  alignment is required rather than documented away. The shelf packer
  keeps the alignment for free (placements are sums of the sizes placed
  before them).
- `inset` came out of `grid`/`namedFrames` and `NamedFramesOptions` went
  with it: with the shader clamp covering level 0 and extrusion covering
  the chain, it had no remaining job.

The copy is one bulk row copy per output row plus the side gutters pixel
by pixel; a 1024-square sheet is a few milliseconds at load. A build-stage
bake (okf/backlog/asset-build-stage.md) would call the same pure function.

## Non-goals

`createAtlas` does not extrude: the slicers are where rects become UVs and
the extruder is where pixels move, and a sheet declaring its gutter once
for both is a second consumer's ask. The clean depth is documented as a
rule (gutter 4 covers mip levels 0 through 2), not computed from the
sheet's drawn size, which the package cannot know.
