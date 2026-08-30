---
title: Seeding a tile world is one setTile call per cell
description: There is no bulk write, so an 18k-cell seed is 18k setTile calls each paying locate() and a frame copy; fine today because the flush batches to a microtask, but a larger world or a procedural refill on approach wants a rect write from a typed array.
created: 2026-08-29
---

# Seeding a tile world is one setTile call per cell

What it looks like when you hit it: an app fills a world from a generator
and writes it cell by cell. 18,096 `setTile` calls for a modest two-world
demo was fine - it batches to one microtask flush and the cost was not
visible - but the shape does not scale: each call runs `locate()` and
copies one frame's four UVs, and a streaming world
([2d-baked-layers](2d-baked-layers.md) B2, a re-fill callback regenerating
a chunk's cells on approach) would pay it per chunk, per approach.

Not having a `<Tile>` component is the right call and stays. This is
about the imperative surface.

## Shape

- `setTiles(col, row, cols, rows, frames)`: a rect write, `frames` a
  `(Frame | null)[]` in row-major order, or a `Uint16Array` of indices into
  a frames table the layer was given (the typed-array fast path for
  generated worlds). One `locate()` per chunk the rect touches, one dirty
  mark per chunk, the per-cell loop inside the layer over its own records.
- Same batching and the same flush; the rect is just a cheaper way to
  dirty the same chunks.

Open before implementing: whether the frames-table form belongs to the
layer (`createTileLayer(..., { frames })`) or is passed per call; the
former matches how an atlas-sliced world actually looks (one `grid()` and
indices everywhere) and is what a `Uint16Array` needs to mean anything.
