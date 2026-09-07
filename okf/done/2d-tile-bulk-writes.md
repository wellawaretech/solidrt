---
title: Seeding a tile world is one setTile call per cell
description: There was no bulk write, so an 18k-cell seed was 18k setTile calls each paying locate() and a frame copy; setTiles now writes a rect from a row-major array - frames, or indices into a frames table the layer owns (a Uint16Array for generated worlds) - one locate and one dirty mark per chunk, and a chunk the rect only clears never allocates.
created: 2026-08-29
completed: 2026-09-07
---

# Seeding a tile world is one setTile call per cell

## Symptom

An app filling a world from a generator wrote it cell by cell: 18,096
`setTile` calls for a modest two-world demo, each running `locate()` and
copying one frame's four UVs. Fine at that size (the flush batches to one
microtask), but the shape did not scale to a streaming world
([2d-baked-layers](../backlog/2d-baked-layers.md) B2, a re-fill callback
regenerating a chunk's cells on approach), which would pay it per chunk,
per approach, and a worker-generated world had no typed array to hand
over. Not having a `<Tile>` component was the right call and stays; this
was the imperative surface.

## What landed

`setTiles(col, row, cols, rows, cells, { tint? })` on the tile layer
(`packages/2d/src/tiles.ts`): a rect write, `cells` row-major and `cols *
rows` long, either `(Frame | null)[]` (null clears) or, with a `frames`
table given at creation (`createTileLayer(..., { frames })`, the
`<TileLayer frames>` prop, exposed as `layer.frames`), indices into it -
the typed-array form a generator or a worker produces. -1 clears in the
index form; a Uint16Array holds it as 0xffff, the same bits, so one rule
covers both spellings and the table caps at 65534 entries. One locate and
one dirty mark per chunk the rect touches, the per-cell loop inside the
layer over its own records, the same batching and flush. A chunk the rect
only clears never allocates, so a whole-world write of a sparse world
allocates exactly the chunks the per-cell seed would. Every entry of
either form is validated before the first write, so a bad one changes
nothing; the table is copied and checked at creation, its UVs kept as a
Float32Array the index write copies by offset. `setTile` and `setTiles`
share one cell writer, and `locate()`'s per-call tuple is gone.

examples/tiles.tsx seeds its 128x128 ring world with one `setTiles` over a
Uint16Array; probes/2d-tiles-bulk-probe.tsx seeds three layers (setTile,
index rect, frame rect) and holds them to the same 2192 cells, the same
46-chunk set and byte-identical bakes (`/texture` raw compare), plus the
clear-only, straddling-rect, plain -1, tint, table-copy and validation
paths. probes/2d-tiles-bulk-bench.tsx measures the write.

## Measured

Release client, desktop GPU, best of five, JS write time only (the flush
is the same GPU work in every case); a 128x128 world, 48px tiles, 169
chunks:

| write | setTile loop | setTiles Uint16Array | setTiles Frame[] |
| --- | --- | --- | --- |
| ring, 3460 cells, re-write over resident chunks | 5.6 ms | 6.7 ms | 5.7 ms |
| solid, 16384 cells, re-write | 25.4 ms | 14.3 ms | 14.0 ms |
| solid, first write, allocating 169 chunks | 273 ms | 247 ms | 192 ms |

So the rect write is 1.8x the per-cell loop on a solid world and at
parity on a sparse one - the shape for worker transfer and chunk re-fill,
not a large speedup. The first seed is dominated by chunk allocation at
1-2 ms per chunk, whatever the API.

## Findings

- The open question - table on the layer or per call - is settled by the
  engines: Unity's `SetTilesBlock` takes a rect plus a row-major array,
  Phaser's `putTilesAt` indices into the map's tileset, Godot's patterns
  atlas coordinates into the map's TileSet. The tileset belongs to the
  map, so the frames table belongs to the layer.
- A rect write allocates lazily per chunk on the first frame it carries
  there, single pass: a clear on an unallocated chunk is skipped, so the
  sparse seed stays sparse without a scan ahead.
- Chunk ALLOCATION ORDER differs between a per-cell scan (cell row-major)
  and a rect write (chunk row-major), so `layer.chunks` order is not a
  contract between the two: compare chunk sets by rect.
- Index validation runs as one read pass before the first write, so a
  bad entry changes nothing; the frames form needs no pass.
- `checkTint` accepts values past 1 (a tint may brighten); it rejects a
  wrong length or a non-finite entry only.
- A fill is `new Uint16Array(n).fill(i)` in the caller; a `fill` verb
  (Phaser has one) adds nothing the index form lacks.
- The per-cell floor in the interpreter is the record write itself: 13
  Float32Array stores (position, size, UVs, tint) per cell, about 0.85 us.
  Removing the per-cell closure, the modulo and the table lookup moved the
  index form by nothing measurable, and the validation pass costs the
  frame form 1-3 ms per 16k cells. A larger step needs the records filled
  natively from the index array, core work that belongs with the
  streaming stage, not this item.
- Chunk allocation (createBuffer + createPipelineTexture) is 1-2 ms per
  chunk here, so the streaming stage must pool chunk resources rather than
  create and destroy them per approach; noted on
  [2d-baked-layers](../backlog/2d-baked-layers.md) B2.
