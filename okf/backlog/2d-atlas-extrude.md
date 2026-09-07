---
title: Extrude atlas cells into gutters so a mipmapped sheet does not bleed
description: The layer shaders clamp samples into their frame, which stops edge bleed at mip level 0, but a mip chain averages blocks that straddle cell edges before any sampling decision; the fix is a load-time repack that copies each cell into a gutter of replicated edge pixels, the thing Unity's packer and TexturePacker call extrude.
created: 2026-09-07
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

`grid`/`namedFrames` `inset` is the stopgap: shaving 2^k texels keeps
level k inside the cell, at the cost of losing that border of every
sprite. A transparent gutter (`spacing`) is the other one, and it bleeds
transparent instead of the neighbour, which is a fade at the edge rather
than a flash.

## Done looks like

A pure load-time repack next to `frames.ts`, taking a `DecodedImage` and
the grid geometry, returning a new `DecodedImage` and the `GridOptions`
that slice it (`marginX/Y = g`, `spacing = 2g`). Each cell is copied
into a gutter of `g` texels filled by replicating the cell's own edge
pixels, so every mip level down to 2^k <= g averages the cell's own
colours at its edge. The frames are the plain cell rects over the new
sheet; nothing in the layers or shaders changes and the atlas is an
ordinary texture. Validated: `g` a power of two, and `cell + 2g` a
multiple of it, so cell edges land on mip texel edges; otherwise throw.

## What it involves

Stage 1, uniform sheets: two nested copy loops with clamped source
coordinates, about the size of `frames.ts`, plus a checks-rig case with a
direct oracle. A CPU copy in JS at startup, a few milliseconds for a 1024
square sheet, so no Rust. `createAtlas` stays thin: the app extrudes
first and passes the result through.

Stage 2, hand-packed named rects: an edge-to-edge sheet has no room for
gutters, so extruding means re-placing the rects with a shelf packer that
returns the new rect table. Roughly twice stage 1. Only when a named-rect
sheet actually asks for mipmaps.

Know before choosing: the gutter must be as wide as the deepest mip
relied on (g = 4 keeps levels 1 and 2 clean), and the sheet grows - a 16
px cell with a 4-texel gutter is 24 px, 2.25x the texels. Once this
lands, `inset` on the slicers has no remaining job and comes out.
