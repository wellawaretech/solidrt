---
title: A splat cloud is resident at 32 bytes per splat plus half-float SH where the field ships 8-16
description: The .sol3s record is 32 bytes and SH bands are uncompressed halves (32/48/96 bytes per splat), so a 1M SH3 cloud is ~140 MB of textures and key column on the phone; the field's chunk-quantized 16-byte record (Unity's importer, PlayCanvas's compressed ply) with 8-bit SH halves the textures and the download, and the engine's sort keys on the packed position word, a third of the key bytes.
created: 2026-09-27
completed: 2026-10-18
---

# A splat cloud is resident at 32 bytes per splat plus half-float SH where the field ships 8-16

## Symptom

okf/notes/gaussian-splats-against-the-field.md: phone captures of
800k-1M splats with SH3 cost ~140 MB resident at 1M (the 96 MB SH
texture, the 32 MB record texture, the 12 MB key column) and a 100 MB
download; the compressed formats the capture apps export (spz) and the
viewers ship (PlayCanvas sogs, ksplat) hold the same cloud in ~30 MB.
The sort's ids and permutations (4 bytes per splat each) come on top and
do not depend on the record format.

## The field

- Unity's Gaussian splatting importer and PlayCanvas's compressed ply
  converged on the same record: 256-splat chunks with per-chunk bounds,
  positions and log scales as 11/10/11 bits over the chunk box, the
  rotation as a smallest-three quaternion in 32 bits, color rgba8 - 16
  bytes per splat, decoded in the vertex stage.
- spz stores SH as a byte per coefficient around 128; ksplat and
  PlayCanvas's SH element are 8-bit too. sogs and Unity's importer go
  further with a k-means palette over the SH vectors (2-3 bytes per
  splat), on a GPU or in minutes of CPU jobs.
- Every chunked format sorts spatially (Morton) and drops the
  importance order the .splat interchange has.

## Design (decided 2026-10-18)

### The record and the container (version 3)

One rgba32ui texel per splat, four words, plus an rgba8 color texel
beside it (the 2026-10-18 revision: the first cut packed the position
into one word at 11/10/11 bits with the color in word 3, the field's
layout, and the human side-by-side rejected it on the lizard's
sub-millimetre scales; see Findings):

| word | content | encoding |
|---|---|---|
| 0 | position x, y | 16-bit unorms over the chunk's position box, x low |
| 1 | position z | 16-bit unorm, low half; high half reserved |
| 2 | rotation | smallest-three: 2-bit largest index, three 10-bit components over [-1/sqrt2, 1/sqrt2] |
| 3 | scale | 11/10/11 bits, unorm over the chunk's log-scale box |
| color block | color, opacity | rgba8 sRGB, its own texture, sampled as unorms |

- The chunk table is a second block: 256 splats per chunk
  (SPLAT_CHUNK_SIZE), 12 floats per chunk (position min and max,
  log-scale min and max), 48 bytes per chunk. It uploads as an rgba32f
  texture of three texels per chunk (`uSplatChunks`, bound per mesh).
- The SH block is a byte per coefficient, spz's (byte - 128) / 128,
  clamped at bake, in 1/2/3 rgba32ui texels per splat at degree 1/2/3
  (9/24/45 bytes used of 16/32/48), decoded with shifts as the color
  is. The header's SH block carries `encoding: "u8"` so a palette
  encoding (phase 2) is additive.
- The order block: the record ids by importance (size times opacity,
  descending), 4 bytes per splat, the draw population a count prefix
  dials. The records themselves sit in spatial order (below), so the
  storage order serves compression and the order block serves the dial.
- The header gains `chunkSize`, `antialiased` and the chunk and order
  block ranges. Version 2 is rejected with the re-bake message.
- The covariance leaves the bake. Its justification was the smaller
  record (12 bytes of halves against the raw scale plus quaternion); the
  compressed scale plus rotation is 8 bytes, and the vertex ALU is
  measured invisible (stage 1's per-corner build read 21.5 ms/M against
  the baked 22.7 the same day; stage C's noraster equals setup at every
  count and K). The y-up flip still bakes: positions and the quaternion
  composed with the half turn, the SH signs as today.

### Order and chunking at bake

Chunk quantization needs spatial locality, the count dial needs
importance order; the storage order serves the first, the order block
the second (decided 2026-10-18 on measurement, see Findings: the first
design ordered records by importance band and chunked within a band,
which cost 12x in position precision).

- k-d chunks: the cloud is split recursively at the median of its
  longest axis, the split snapped to a multiple of 256, until every leaf
  is one chunk. Leaves are compact boxes with no Morton seams; the one
  partial leaf is the cloud's last chunk.
- The order block ranks the records by importance; the engine's mirror
  holds (id, packed position) pairs in that order, 8 bytes per splat,
  and a count prefix is the first n of it, exactly as today.
- The tool reports the loss: the chunk's position step over each
  splat's largest axis as the median, the 99th percentile and the share
  above a tenth (SPLAT_STEP_VISIBLE), and the clamped SH coefficients.

### Engine

- Core, a packed position key: `transferRecords`' key gains a packed
  form, `{ stride: 16, position: 0, packed: { format: "unorm16x3",
  chunk: 256, bounds: Float32Array }, order: Uint32Array }`. The alloy
  order module's `KeyColumn` carries the encoding and the chunk table
  (validated at hand-off: ceil(count / chunk) x 6 floats); the hand-off
  gathers the mirror as (id, packed words) records in `order` (the draw
  population: a prefix is its first n), and the key pass precomputes
  dot(min, d) and the per-axis step times d per chunk, then each record
  is three multiply-adds on the unpacked integers. The mirror is 12
  bytes per record, read sequentially. The entry's `instanceOrder` is
  unchanged; the attach check compares stride and offset as today.
  Native types only: any large point population drawn in an order
  other than its storage order keys this way.
- 3d: the indexed `instanceOrder` accepts a uint32 position attribute
  when `packed` is given; `createSplatMesh` passes the chunk table
  through and binds the chunk texture.
- The vertex stage fetches one record texel and three chunk texels,
  decodes, rebuilds the covariance (R S St Rt, the stage 1 probe's
  code) and projects as today.
- Fidelity folded in (closes okf/backlog/splat-raster-fidelity.md,
  the shader is rewritten anyway): the reference's 0.99 per-splat alpha
  cap, and the Mip-Splatting compensation sqrt(det(S) / det(S +
  dilation)) multiplied into the alpha when the header's `antialiased`
  flag is set (`--antialiased` on the tool, a per-capture fact).

### At 1M splats

| | today | proposed |
|---|---|---|
| record texture | 32 MB | 16 MB |
| color texture | 0 | 4 MB |
| SH3 texture | 96 MB | 48 MB |
| chunk texture | 0 | 0.2 MB |
| engine key mirror | 12 MB | 12 MB |
| id buffer | 4 MB | 4 MB |
| resident, SH0 / SH3 | 48 / 144 MB | 36 / 84 MB |
| download, SH0 / SH3 | 33 / 129 MB | 24 / 72 MB |

## Scope decisions

- Phase 2, palette SH: product quantization at bake (three 15-dim
  subspaces, 256 centroids each, 3 bytes per splat plus a 46 KB
  codebook) takes SH3 from 48 MB to 3 MB, behind `encoding` in the
  header. The engine half is a codebook texture and an index fetch; the
  bake half is a k-means to engineer, with its own quality judgement, so
  it follows phase 1 rather than holding it.
- Entropy coding of the download belongs to the asset build stage
  (okf/backlog/asset-build-stage.md), not the format: an interleaved
  texel block uploads verbatim, which is the zero-parse load.
- Per-chunk frustum culling in the sort is possible on the chunk table
  later; it needs a frustum feed, not the direction feed, so it is
  additive and separate.

## Steps

1. Format and bake: splat-data.ts (bands, the k-d chunker, the
   quantizers, the v3 container), the tool's flags and loss report, the
   bun tests (round-trip error bounds per field, the band prefix
   invariant, chunk count, container).
2. Core: the packed key in alloy's order module and registry, the flux
   marshalling, the flux-types doc, alloy order tests (packed key order
   equals the float key order on a synthetic cloud, hand-off
   validation).
3. Runtime: the vertex decode, the chunk texture, the fidelity rules,
   createSplatMesh and the component, the 3d AGENTS.md section, the app
   test on the new bake.
4. Measurement on the Pixel 7 (cold, hold off, 848 MHz rows): the
   vertex side at 1M with the decode and chunk fetch against stage D's
   7.4-8.0 ms, sort thread ms at 1M against 53, residency via dumpsys.
   Quality: train and lizard rendered v2 against v3 through a snapshot
   diff, then the human side-by-side verdict on the pair.

## Outcome

Built and closed 2026-10-18 in one day: the format (version 3), the
core's packed key, the runtime decode, the fidelity rules, the
measurements on the laptop and the Pixel 7, and two human side-by-side
verdicts - the first rejected the field's 11-bit position on a capture's
micro-splats and the record was revised to 16 bits before the second
passed. Not done on purpose: phase 2, palette SH
([../backlog/splat-palette-sh.md](../backlog/splat-palette-sh.md)); the
u64 radix idea (the phone showed no need); a variable chunk size for
sparse regions (the 16-bit step made it moot at the lizard's tail,
0.9% of splats above a tenth of their size, all in near-empty space).
The linear-blend regression found on the way is HEAD's and has its own
item ([../done/mali-linear-scene-path-doubled.md](../done/mali-linear-scene-path-doubled.md)).

## Findings

Cut into [../notes/splat-record-compression.md](../notes/splat-record-compression.md).
