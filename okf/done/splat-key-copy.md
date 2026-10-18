---
title: The engine holds a full 32-byte record copy of a splat cloud when the sort needs 12
description: An indexed instance order keys on the record's position, but the hand-off mirror the core sorts is the whole record block (32 bytes per splat, 32 MB at 1M) next to the same bytes in the record texture; the mirror should hold the key column only, and the data textures should fill from the file block in place instead of a padded copy, so the phone's residency and load peak both drop.
created: 2026-09-27
completed: 2026-10-17
---

# The engine holds a full 32-byte record copy of a splat cloud when the sort needs 12

## Symptom

okf/notes/gaussian-splats-against-the-field.md: `transferRecords` hands
the full record block to the core as the sort's key source, and the
indexed order reads only the 12-byte position per record. At 1M splats
that is 32 MB where 12 would do, on top of the 32 MB texture and the
4 MB id stream. Beside it, `splatTexture` pads each data block into a
fresh row-aligned copy before `createTexture` copies it again: 128 MB
of transient memory at load for a 1M SH3 cloud, which is the peak that
reaped the phone client.

## The field

- PlayCanvas and the three.js splat viewers (GaussianSplats3D, Spark)
  hand their sorter a float32x3 centers array, never the record; the
  record data lives in planar GPU textures the sorter never sees.
- Unity's Gaussian splatting package keeps positions in their own GPU
  buffer, apart from shape and SH, because positions have several
  consumers (sort, bounds, picking, editing); its importer bakes
  per-stream formats and the runtime allocates buffers by size and
  fills them, never through a padded CPU copy.
- WebGPU and Unity allocate textures by size and write regions into
  them. Only three.js's DataTexture takes a full-size block at creation.

Two lessons: an engine's sort holds only its key, and a data texture is
filled from the source block in place.

## Design

1. Core: the indexed order's mirror holds the key column. Under the
   index form the mirror is never republished (a re-sort writes ids, a
   publish onto it is cancelled), so keeping the whole record keeps 20
   bytes per record the engine never reads. `transferRecords(id,
   records, { stride, position | field })` declares the record layout
   and the core copies the key column out as it takes the hand-off (12
   bytes per position key, 4 per field key); no full copy exists
   engine-side, and the app hands a view of the file bytes. At attach
   the entry's `indices.stride` and key offset must equal what the
   hand-off declared, else the attach throws naming both. The gathered
   `retain` form keeps whole records because it republishes them: the
   mirror holds what a republish needs. Generic: every indexed
   population gets it, nothing in the 3d package extracts anything.
2. Core: `createMutableTexture` takes no seed (null), allocating
   storage with unspecified contents, like Unity's texture constructor
   and WebGPU's createTexture; `uploadTexture` with a rect fills it.
3. 3d: `splatTexture` allocates the texture at its size and uploads the
   block verbatim in row bands straight from the file view, the tail
   row as a last partial rect. The padded copy is gone for the record
   and SH textures; the transient at load is one band. The banded
   rect upload is also the hook a streaming loader needs later.
4. Docs: flux-types hand-off and `indices`, the 3d `InstanceOrderOptions`
   `records` wording, the splat section of the 3d AGENTS.md, the
   SPLAT_ATTRIBUTES comment. Tests: the key column's parse, extraction
   and attach mismatch in the alloy order tests.

## Scope decisions

- The file format stays. A planar v2 container (centers block, shape
  block, spare word dropped) is exactly the reshaping
  okf/backlog/splat-record-compression.md does with per-block encodings;
  a bump the next item bumps again is churn. With step 1 in place,
  compression's "decode at load into the key copy" needs nothing new
  from core: it hands a float32x3 stream and declares stride 12.
- okf/done/splat-sort-spike.md measures after this lands, once: the
  key pass streams a third of the bytes it did.

## Verification

- alloy order tests (key column parse, extraction, order match), the
  core upload test for a null-seeded mutable texture filled by rect
  uploads, and a new 3d app test (packages/3d/tests/splat-mesh.test.tsx)
  that paints two splats back to front, re-sorts them as the camera
  crosses to the other side, and dials the count from 0; all suites
  green on the desktop client.
- Pixel 7, train cloud (1,026,508 splats, SH 0) on the splat probe: the
  key column is 12.3 MB by construction (count x 12) where the record
  block was 32.8 MB; dumpsys meminfo read the Native Heap at 113 MB with
  the cloud mounted and 103 MB after a remount at 1000 splats, a delta
  consistent with the column (swap movement makes PSS noisy at the
  10 MB scale). The padded upload copies are gone by construction. The
  before figures are the old code's arithmetic, not a measurement: the
  old client cannot run the new hand-off.
