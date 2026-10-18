---
title: Gaussian splats
description: The splat viewer in one place - the .sol3s bake and its chunked 16-bit record, the data textures and the one-texel vertex decode, the packed depth key the core sorts on its own thread, the importance order block the count dial reads, the blend space a capture is trained against, the decisions behind each with the alternatives that were rejected and measured, and the known limits with their open items. Read before touching splat-data.ts, splat.ts, the order module or the splat probe.
created: 2026-10-18
---

# Gaussian splats

The splat viewer landed over three weeks and seven records: the viewer
itself, the vertex-wall measurements, the key column, the sort thread,
the record compression and the two human verdicts that shaped it. Each
answered its question in its own file and the next session re-derived the
picture from the fragments. This document is the picture, kept current;
the records are linked where they carry the measurements.

The code this describes: `packages/3d/src/splat-data.ts` (the parsers,
the bake, the container, the reference decoder; the runtime-free
`@solidrt/3d/splat` entry), `packages/3d/tools/splat.ts` (`sol tool
3d/splat`), `packages/3d/src/splat.ts` (the material, the data textures,
`createSplatMesh`), `packages/3d/src/mesh.ts` (the indexed instance order
and the hand-off), `alloy/src/gpu/order.rs` (the key vocabulary, the
radix sort, the packed key), `alloy/src/gpu/order_worker.rs` (the sort
thread), `alloy/src/context/order.rs` (the registry and the mirrors),
`flux/src/alloy_plugins/gpu.rs` (`transferRecords`),
`probes/splat-mesh-probe.tsx` (the bench ladder).

## Vocabulary

- **Capture.** A trained 3D Gaussian splatting scene: a point list of
  position, covariance as scale and rotation, opacity, and color as
  spherical-harmonic (SH) coefficients. Arrives as a trainer's `.ply`,
  the `.splat` interchange or Niantic's `.spz`.
- **Bake.** `sol tool 3d/splat`: parse at pack time, quantize, order,
  write `.sol3s`. Runtime loading is a header parse plus byte views;
  nothing per splat runs on the device.
- **Record.** One splat's 16 bytes in the record block, one `rgba32ui`
  texel. Records sit in SPATIAL order (a k-d partition).
- **Chunk.** 256 consecutive records (`SPLAT_CHUNK_SIZE`); record id / 256
  is the chunk. Each chunk has a box for positions and a box for log
  scales; the record's position and scale words are unorms over them.
- **Order block.** The record ids by importance (size times opacity,
  descending): the DRAW population. A count prefix is its first n.
- **Id stream.** The one instance buffer: the sorted record ids, 16 per
  drawn instance (`SPLAT_GROUP`), written whole by the core on each
  re-sort, the sentinel `0xFFFFFFFF` past the population.
- **Mirror.** The core's copy of what its sort reads: for a splat cloud,
  (record id, the two position words) per splat in draw order, 12 bytes,
  plus the chunk table. Nothing else of the record lives engine-side.
- **Key.** The depth a record sorts by: dot(decoded position, view
  direction), decoded from the words and the chunk box in the key pass.
- **Order gate.** The ~2 degree direction change (`ORDER_DIRECTION_EPS_COS`)
  past which the scene re-feeds the direction and the core re-sorts.

## The pipeline

1. **Parse** (`parseSplat`): the three formats into one `Gaussians`
   product (linear scales, (w, x, y, z) rotations, color bytes, SH
   coefficients). `.spz` arrives gunzipped by the tool; the entry stays
   decompression-free.
2. **Stand up**: y-down captures (`.ply`, `.splat`) are rotated to y-up -
   positions (x, -y, -z), the quaternion composed with the half turn
   (q' = (-x, w, -z, y)), the SH coefficients odd in (y, z) negated.
   `.spz` is y-up already. `--keep-orientation` opts out.
3. **Partition** (`kdOrder`): a k-d split at the median of the longest
   axis, the left size snapped to a multiple of 256, down to leaves of
   one chunk; the remainder is the last, partial chunk. Compact boxes,
   no Morton seams.
4. **Quantize** per chunk: position as three 16-bit unorms over the
   chunk's position box (x | y << 16, then z; 16 bits reserved),
   rotation as smallest-three (largest index on top, three 10-bit
   components over [-1/sqrt2, 1/sqrt2] in 1022 steps so zero is exact),
   scale as 11/10/11-bit unorms over the chunk's log-scale box (log
   clamped to [-12, 10]), color as rgba8 in the color block, SH as a
   byte per coefficient, spz's (byte - 128) / 128, in 1/2/3 texels.
5. **Rank**: importance per splat, the order block as record ids.
6. **Report**: the chunk position step over each splat's largest axis
   (median, 99th percentile, the share above `SPLAT_STEP_VISIBLE`), the
   clamped SH coefficients. The tool prints it; the bake is lossy and
   says what it lost.
7. **Container** (`encodeSplat` / `decodeSplat`): "SOLS" | version 3 |
   JSON header (count, chunkSize, shDegree, antialiased, bounds, block
   ranges, the SH block's `encoding`) | records | colors | chunks |
   order | SH. Decode views the bytes; version 2 is refused with the
   re-bake message.
8. **Runtime** (`createSplatMesh`): a RecordMesh in the INDEXED form
   over the stock material for (shDegree, antialiased). The record block
   is handed to the core with the packed key (`transferRecords(id,
   records, { stride: 16, position: 0, packed: { format: "unorm16x3",
   chunk: 256, bounds }, order })`) and uploaded in row bands, in place,
   as `uSplatRecords`; the chunk table as `uSplatChunks` (rgba32f, 3
   texels per chunk), the colors as `uSplatColors` (rgba8), the SH block
   as `uSplatSh`. The header bounds are the cull and picking box.
9. **Vertex stage**: the id from the instance's 16 ids by quad, the
   record texel, the chunk's three texels, the decode (`decodeRecord`
   in splat-data.ts is the reference it must agree with), the covariance
   R S St Rt rebuilt, the projection through the Jacobian of
   `uViewProj * uModel` (no focal or view uniforms), the 0.3 px^2
   dilation, optionally the Mip-Splatting compensation, the SH bands per
   corner from `uCamPos`.
10. **Fragment**: the gaussian falloff times opacity, cut at e^-4 and
    below 1/255, capped at 0.99 (the reference rasterizer's cap),
    premultiplied out.
11. **Order**: the scene feeds the view direction mapped into the mesh's
    frame; past the gate the registry re-sorts the mirror - in the flush
    below `SYNC_SORT_MAX_KEYS` (16384), on the `sol-order` thread above
    it - and writes the id stream. A parked camera uploads nothing.

## Decisions

**D1. Bake at pack time, never parse on the device.** Every WebGL viewer
parses `.ply` at runtime (seconds, 2x memory). The bake is the model
loader's shape. [done/gaussian-splats.md](../done/gaussian-splats.md).

**D2. An ordinary record mesh, not a splat subsystem.** The cloud is
depth-tested against geometry, placed by the transparent entry sort,
picked by its bounds, transformed by its node. Nothing splat-shaped
enters the scene model; the material class is exported for forks.

**D3. The sort runs in the engine, synchronous with the frame below
16k keys and on its own thread above.** Every surveyed viewer sorts on a
worker and lags by frames; ours is exact for small clouds and lags one
sort for large ones, never stalling the frame.
[done/gpu-instance-order.md](../done/gpu-instance-order.md),
[done/splat-sort-spike.md](../done/splat-sort-spike.md). Rejected: a
cheaper 16-bit key (the exact key costs nothing once off the thread), an
adaptive gate (trades staleness for frames, removes no stall).

**D4. The indexed form: records in a data texture, ids in the instance
buffer, 16 splats per instance.** Measured on the Pixel 7 at 1M: the
one-quad-per-instance frontend was two thirds of the vertex side (12.6
cycles per instance); K = 4, 8 and 16 collapse it alike; the per-vertex
fetch is free; vertex ALU is invisible next to fetch and fill.
[notes/gaussian-splats.md](../notes/gaussian-splats.md) (stage C and D).
Rejected: transform feedback (attacks ALU, which was never the term).

**D5. The engine keeps a key column, never the records.** The mirror
holds what the sort reads; the texture is the app's upload of the same
bytes. [done/splat-key-copy.md](../done/splat-key-copy.md).

**D6. Blend in display space for captures.** Captures are trained against
sRGB blending; linear light lays a milky veil over stacked translucents.
Judged by a human pair; `blendSpace: "display"` on the scene.
[notes/gaussian-splats.md](../notes/gaussian-splats.md).

**D7. The covariance is rebuilt in the vertex stage, not baked.** The
first bake precomputed it as six halves to spare the vertex stage; stage
C then measured vertex ALU as invisible, so the bake's only remaining
justification was record size, and the compressed scale plus rotation is
8 bytes against 12. Reverted with the compression; the phone read the
rebuild plus the chunk fetches at about 1.5 ms per million splats.
[done/splat-record-compression.md](../done/splat-record-compression.md).

**D8. Positions are 16-bit unorms over a 256-splat chunk box; the
field's 11/10/11 was built, judged and rejected.** The field's record
(Unity's importer, PlayCanvas's compressed ply) packs position, rotation,
scale and color into 16 bytes with 11 bits of position. Built first;
indistinguishable on the train; visibly worse on the lizard's cheek,
where metal scales are micro-splats of 0.0002 units inside a 2.2-unit
chunk - a 1 mm step against splats a fifth of that. 16 bits per axis
takes the lizard's worst splat from 22 times its size to 0.7 and the
99th percentile to 9%, and the second verdict passed. The cost: the
color left the record for its own rgba8 texture (20 bytes per splat on
the GPU, not 16) and the mirror is 12 bytes, not 8. sogs made the same
move to 16-bit means. [notes/splat-record-compression.md](../notes/splat-record-compression.md).

**D9. Storage order is spatial; the draw order is a separate block.**
The first design ordered records by importance band and chunked within a
band so a count prefix needed no order block; measured on the train it
cost 12x in position precision (every band spreads over the whole scene,
so a chunk of same-band splats is a slab). The order block costs 4 bytes
per splat on disk and lets the storage order serve compression alone.
Generic in the core: any indexed population drawn in an order other than
its storage order hands off `order`.

**D10. The core decodes the packed key itself.** The sort could have
keyed on a float column decoded at load, but decoding a million
positions in JS at load is a stall and a float column is 12 bytes per
splat the file does not carry. The key pass precomputes dot(box min, d)
and the per-axis step along d per chunk, then does three multiply-adds
per record on the integers; the chunk table stays in cache. On the Pixel
7 the packed sort reads at or under the float sort; on a laptop the
mapped id materialization (a random gather) costs about 25%. Rejected
for now: carrying (key, id) as one u64 through the radix passes (would
also delete the per-pass random key read); the phone showed no need.

**D11. SH as bytes, a palette later.** spz's byte encoding halves the SH
texture against halves and round-trips an spz exactly; the header's
`encoding` field makes a palette encoding additive
([backlog/splat-palette-sh.md](../backlog/splat-palette-sh.md)).

**D12. The reference rasterizer's two rules ride along.** The 0.99 alpha
cap, and the Mip-Splatting compensation behind a per-capture header flag
(`--antialiased`), because the trainer's mode is a fact the file should
carry, not a renderer guess.
[done/splat-raster-fidelity.md](../done/splat-raster-fidelity.md).

## Known limits and open items

- **Fill is the phone's wall.** At 1M full resolution the Pixel 7 spends
  ~44 ms in fill (display blend), untouched by the record format; the
  structural answer (front-to-back tile blending) needs GLES 3.1 compute,
  which is not on the table. The levers are overdraw cuts
  ([backlog/splat-overdraw-fill.md](../backlog/splat-overdraw-fill.md))
  and drawing less: per-chunk frustum culling in the sort, which the
  chunk table now makes possible
  ([backlog/splat-order-and-chunks.md](../backlog/splat-order-and-chunks.md)).
- **The linear blend path costs twice the display path on Mali**, in
  HEAD, since some change after 2026-09-27; not the splats'
  ([done/mali-linear-scene-path-doubled.md](../done/mali-linear-scene-path-doubled.md)).
- **The count dial drops a detailed subject first.** Importance is
  volume times opacity; a capture whose subject is all micro-splats
  loses the subject before the garden at 100k. The order block takes any
  ranking ([backlog/splat-order-and-chunks.md](../backlog/splat-order-and-chunks.md)).
- **Sparse leaves.** A leaf holds 256 splats whatever their spread; in
  near-empty regions the box is wide and 0.9% of the lizard's splats
  move more than a tenth of their size (same item).
- **The entry's order check does not know the key is packed**: it still
  demands 12 bytes from the key offset, so a packed layout with the words
  last is refused with "does not fit"
  ([backlog/splat-record-loose-ends.md](../backlog/splat-record-loose-ends.md)).
- **16 reserved bits per splat**, the antialiased path without a capture
  to judge it, the constant log-scale floor, and the test gaps at the
  flux and 3d boundaries: same item.
- **No level of detail or streaming** beyond the count prefix; a
  city-scale capture is out of scope.
- **Floaters.** A capture is clean near the camera path it was taken
  from; elsewhere it is smears. The app keeps the camera near the path.

## Measuring

- The ladder is `probes/splat-mesh-probe.tsx`: `bench { count, scale,
  orbit, shader, blend }` remounts at azimuth 0; `shader: "noraster"`
  is the vertex side alone; `/stats?window=10000` gives
  `gpuFrameExecMsPerFrame` (this Mali's pass timers read 0),
  `orderSortMs` the last sort on the thread.
- Phone rows are valid only cold (skin under 35.5 C, status 0), cadence
  hold off (`--es sol_env SOLIDRT_CADENCE_HOLD=off`), with every clock
  sample at 848 MHz, same pose, 6 s settle and a 10 s window; sub-vsync
  rungs idle and downclock, so their ms are ceilings
  ([notes/gaussian-splats.md](../notes/gaussian-splats.md)).
- A format change is judged by a snapshot diff against the previous
  format's render (same client size, pose and blend: ImageMagick's MAE
  and max over 255, then a crop of the worst region at 2x) AND a human
  flip; the numbers passed the 11-bit record that the flip failed.
- The bake's report is read against the dense region's tail, not the
  global share: the lizard's 99th percentile at 2.9 times the splat's
  size was the 11-bit verdict, two hours before the human gave it.

## Records this gathers

[done/gaussian-splats.md](../done/gaussian-splats.md),
[notes/gaussian-splats.md](../notes/gaussian-splats.md),
[notes/gaussian-splats-against-the-field.md](../notes/gaussian-splats-against-the-field.md),
[done/gpu-instance-order.md](../done/gpu-instance-order.md),
[done/splat-key-copy.md](../done/splat-key-copy.md),
[done/splat-sort-spike.md](../done/splat-sort-spike.md),
[done/sort-thread-core-placement.md](../done/sort-thread-core-placement.md),
[done/splat-record-compression.md](../done/splat-record-compression.md),
[notes/splat-record-compression.md](../notes/splat-record-compression.md),
[done/splat-raster-fidelity.md](../done/splat-raster-fidelity.md).
