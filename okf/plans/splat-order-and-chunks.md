---
title: The chunk is the unit of precision, culling and level of detail - variable runs ordered by importance inside, cut per run by screen contribution in the sort, and no order block
description: The count dial drops a capture's subject first because the order block ranks the whole cloud by one view-independent number, a fixed 256-splat leaf is wide in sparse space, and nothing skips an off-screen chunk; one change of unit answers all three - the chunk becomes a variable-length run stored by importance inside, the sort's key pass tests each run against the view and cuts it where its splats fall below a screen contribution, the count is a budget over what passed, and the order block is deleted rather than re-ranked.
created: 2026-10-18
---

# The chunk is the unit of precision, culling and level of detail

## Symptom

From okf/done/splat-record-compression.md and okf/design/splats.md:

- **The count dial.** `scratch` renders of the lizard at 786k / 300k /
  100k (2026-10-18): at 100k the garden, path and plinth survive as
  coarse shapes and the lizard dissolves into a smear, because
  importance = volume x opacity ranks the big background splats first
  and the subject is all micro-splats. The prefix is the only LOD dial.
- **Sparse leaves.** The bake report on the lizard: position step over
  the splat's largest axis p99 7.7%, 0.9% of splats above a tenth, all
  in chunks of near-empty space (the top-left and the trees); a leaf is
  256 splats whatever their spread, so the box is wide there.
- **Culling.** The Pixel 7 at 1M full resolution spends ~44 ms in fill
  (display blend) and ~8 in the vertex side; off-screen splats are
  culled per vertex but still sorted, materialized and fetched, and
  nothing on screen is drawn less. The core holds every chunk's box
  (the mirror's `bounds`) and could skip whole chunks before the key
  pass with a frustum instead of a direction.

## Why one unit, not three jobs (decided 2026-10-21)

The item was first shaped as three sessions: re-rank the order block so
a prefix thins everywhere, split or pad sparse leaves, and test chunk
boxes against a frustum in the key pass. Widened to engine grade they
are one change:

- The first and third job are the same decision, "which records does a
  count select", answered today by a view-independent global list. Any
  bake-time re-ranking still thins a far chunk and a near chunk alike.
  The place to decide is the key pass, which under the third job tests
  every chunk's box against the view anyway and so knows each chunk's
  distance: it can cut each chunk where its splats fall below a screen
  contribution, the screen-error metric mesh LOD cuts by and what
  Hierarchical 3DGS cuts its tree by. The lizard-versus-garden problem
  dissolves: the subject's chunks are near and keep their micro-splats,
  the garden's are far and thin.
- Once the cut is per chunk the order block is dead weight: records
  inside a chunk are stored by importance descending (free, the chunk
  box does not care about the order within it, so D9's precision
  measurement stands), and a chunk's cut is a prefix of its run. That
  removes 4 bytes per splat on disk, turns the 12-byte draw-order
  mirror into 8-byte words in storage order read sequentially, makes
  culling a run skip, and replaces the random id gather D10 priced at a
  quarter of the sort with slot arithmetic.
- The second job stops being padding: a chunk is a variable-length run,
  split until its box step is under SPLAT_STEP_VISIBLE of its smallest
  splat, and the record's reserved 16 bits carry its chunk index for
  the shader (the decision okf/backlog/splat-record-loose-ends.md asked
  for). Nothing in the file is pre-release-compatible with version 3;
  version 4 replaces it outright.

Rejected on the way:

- Re-ranking the global order (bit-reversed leaves within importance
  bands, contribution at a reference distance): view-independent, so a
  prefix still thins near and far alike, and it keeps a block the
  per-run cut makes redundant.
- Padding short leaves to 256 with never-listed records: keeps `id /
  256` in the shader at the price of dead texels, and a budget to tune;
  the chunk index in the record costs nothing the record did not carry
  as zeros.
- Importance as volume x opacity (the .splat convention): ranks a blob
  over a disc of the same footprint, and a capture's surfaces are discs.
  Replaced by projected area x opacity (the two largest axes), the
  view-independent part of what the cut measures. Judged, not assumed:
  see Steps.

## Design

### Format, version 4 (`splat-data.ts`)

- **Runs.** `kdOrder` splits at the median of the longest axis while a
  node holds more than SPLAT_CHUNK_MAX (256) records, and goes on
  splitting a leaf while its box step along its largest axis exceeds
  SPLAT_STEP_VISIBLE of the leaf's smallest largest-axis scale and the
  leaf holds at least 2 x SPLAT_CHUNK_MIN (16) records. No snap to
  multiples of 256 and no partial last chunk: a chunk is a run of
  SPLAT_CHUNK_MIN..SPLAT_CHUNK_MAX consecutive records. A cloud is capped
  at SPLAT_CHUNK_LIMIT (65536) chunks, the 16-bit index; a bake that
  would exceed it stops splitting leaves and the report says so.
- **In-chunk order.** Within a run the records are stored by importance
  descending, importance = s1 x s2 x opacity with s1 >= s2 the two
  largest scales (the projected area at unit distance times what it
  blends in). The draw order block is gone; `SplatData.order` with it.
- **Record.** Word 1's high 16 bits are the record's chunk index. The
  rest of the record is version 3's.
- **Chunk blocks.** `chunks` stays 12 floats per chunk (the two boxes,
  the `uSplatChunks` texture); `runs` is a u32 per chunk, the run's
  first record (the next run's start ends it, the count ends the last);
  `ramp` is SPLAT_RAMP_SAMPLES (16) f32 per chunk, the importance at
  rank floor(i x len / 16) of the run, i = 0..15, monotone
  non-increasing - what the cut reads instead of records.
- **Header.** `chunkSize` becomes `chunkMax`, `chunkMin`; the block
  ranges gain `runs` and `ramp` and lose `order`. Version 3 is refused
  with the re-bake message.
- **Report.** The position step as today, now over variable runs, plus
  the chunk count and the smallest run; `decodeRecord` reads the chunk
  from the record.

### Core (`alloy/src/gpu/order.rs`, `context/order.rs`, `order_worker.rs`)

- **Hand-off.** `transferRecords(id, records, { stride, position,
  packed: { format: "unorm16x3", bounds, runs, ramp } })`: no `chunk`,
  no `order`. `check_packed` validates runs (ascending, first 0, every
  run within 1..=SPLAT_CHUNK_MAX, the last ending at the count) and the
  ramp (16 per run, non-increasing). The mirror holds the packed words
  only, 8 bytes per record in storage order (`PACKED_WORD_BYTES`), and
  the run table (bounds, starts, ramp) beside it.
- **The view feed.** `SinkWriter::write_order_view(target, draw, clip:
  [f32; 16], viewport: [f32; 2])` replaces `write_order_direction`: the
  node's model-to-clip matrix (the target's view-projection times the
  node's world) and the target's size in pixels. From the matrix alone
  the core derives the six frustum planes in the model frame
  (Gribb-Hartmann), the depth direction the key sorts by (the w row for
  a perspective projection, the z row for an orthographic one - exactly
  what the direction feed computed as W3x3^T f), and the pixel scale at
  a run's box (the same Jacobian the vertex stage projects with,
  evaluated at the box's nearest point). The entry update field is
  `orderView`; `orderDirection` is gone (its test rewrites against the
  view).
- **The selection pass**, the first thing a sort job (or the in-flush
  sort below SYNC_SORT_MAX_KEYS) does: for each run, the box against
  the planes (out: skipped whole), then the cut: the run's pixel scale
  squared times the ramp gives each sixteenth's contribution in pixels;
  the prefix kept is the largest whose ramp sample is at or above
  `threshold` pixels, interpolated to a rank. A target without a view
  (never fed) keeps every run whole. Then the budget: if the kept
  records sum past the entry's instance budget x K, bisect the
  threshold upward until they fit (SPLAT_BUDGET_BISECTIONS, ~20 over a
  4k-run table, under a millisecond). Without a view the same bisection
  runs on the world-frame ramp, which is the old global importance
  prefix. The output is a list of (start, len) selections, the
  population the key pass reads sequentially.
- **The key pass and sort** as today over the selected records: dot(box
  min, d) plus three multiply-adds per record; the permutation over the
  compacted slots; `materialize_indices` maps a compacted slot back to
  its record id through the selection list (a prefix sum over runs, no
  gather).
- **The drawn count.** The entry's `instanceCount` is the app's budget
  in instances (what `setRecordCount` sets, as today). The range the
  raster thread draws is the registry's: ceil(selected / K) instances,
  sent as the draw range when a sort lands, never above the budget. The
  sentinel past the selection stays as the shader's guard.
- **LOD state.** `instanceOrder.lod: { threshold }` (pixels; default
  SPLAT_LOD_THRESHOLD, 0 = no thinning, the budget alone) on the entry's
  declaration, updatable as `orderLod` on the draw update (quality rows
  change it). Names to confirm.
- **The job** carries the view, the budget, the threshold and the run
  table Arcs; `SortDone` carries the selected count. Settle and counters
  unchanged.

### Spatial (`alloy/src/spatial/mod.rs`)

- `order_feed_pass` writes `write_order_view` from the target's
  view-projection (`view_projs`) and the node's world, under two gates:
  the direction turned past ORDER_DIRECTION_EPS_COS (as today, the
  re-sort gate; okf/tiny.md's "measure a tighter gate" line is measured
  inside this plan), or the eye moved more than ORDER_VIEW_MOVE_FRACTION
  of its distance to the entry's world box (the half diagonal when
  inside), or the viewport changed - the re-selection gates. Both
  tunings are named constants set from the Pixel rows below.

### Runtime (`packages/3d`)

- `splat.ts`: the shader reads the chunk from `rec.y >> 16`; the
  hand-off passes runs and ramp; `createSplatMesh(data, { count?,
  lodThreshold?, ... })`, `count` the budget and `lodThreshold` in pixels
  (default SPLAT_LOD_THRESHOLD; the human flip decides its value);
  `SplatMesh` gets the same props; `setRecordCount` stays the budget
  dial. `mesh.ts`: `resolveInstanceOrder` takes `packed` without
  `order`; the entry's `lod`.
- `scene.ts`: `orderFeed` unchanged; the feed itself changed shape in
  the core.
- Types and docs: `packages/core/src/gpu.ts`, `packages/flux-types/gui/
  gpu.d.ts` (`RecordKey`, `PackedPosition`, `orderView`, `lod`),
  `packages/3d/AGENTS.md` Splats and the SplatMesh row, `okf/design/
  splats.md` (D9 and D10 rewritten, D13 the run as the unit, the limits
  list), `probes/splat-mesh-probe.tsx` (`count` is the budget; a
  `threshold` command).

### Generic claim

Any chunked point population handed off with a box, a run and a ramp
per chunk is culled and thinned by the core this way; nothing in the
core names splats. The ramp is "a monotone importance per run", the
threshold "a contribution in pixels": a point cloud, a particle cache
or an impostor field fits the same hand-off.

## Non-goals, all additive later

- Merged aggregate splats per chunk for far LOD (a bake-side merge of a
  run's tail into a few coarse gaussians, drawn in place of the cut):
  the run has room for them as extra records the ramp ranks below the
  cut.
- Streaming: the run is the unit; a cloud whose runs load on demand is
  a hand-off that grows.
- The on-screen fill wall: culling and thinning cut vertex and sort
  work and the fragments of what is thinned; overdraw of what stays on
  screen is okf/backlog/splat-overdraw-fill.md.

## Steps

1. **Format and bake**: `kdOrder` to runs, the in-chunk importance
   order, the chunk index in the record, the runs and ramp blocks, the
   v4 container, the report, the tool's output; bun tests (every run
   within the size bounds and under the step threshold or at
   SPLAT_CHUNK_MIN, the in-chunk order monotone and the ramp with it,
   the chunk index decodes, the container round-trips, v3 refused by
   name).
2. **Core**: the packed mirror in storage order, the run table, the view
   feed through `SinkWriter`, the selection pass, the budget bisection,
   the drawn range on landing, the flux marshalling, the type docs;
   alloy tests (the selection drops a run behind the eye and keeps one
   in view, the cut follows the ramp and the pixel scale, the budget
   bisection lands under the cap, the selected order equals the float
   sort of the same records, the materialized ids match the runs, the
   no-view path equals the old importance prefix, hand-off validation
   by name); the flux gpu test for the hand-off shape; the spatial
   gates' tests.
3. **Runtime**: the shader's chunk read, `createSplatMesh` and the
   component, `resolveInstanceOrder`, the probe; app tests (the count
   dial as a budget, a far chunk's micro-splats thinned and the near
   chunk's kept read back from the id buffer, a chunk behind the camera
   absent from the id buffer, the sort-thread crossing as today).
4. **Measurement**, Pixel 7 protocol (cold, hold off, 848 MHz rows,
   okf/design/splats.md "Measuring"): the train at 1M full resolution
   from the default pose and from inside its volume (fill, vertex,
   `orderSortMs`) against HEAD; the lizard at the default pose at
   thresholds 0, 0.25, 0.5, 1 px; the selection pass ms on the thread.
   Laptop: the same for the trend. The two gates set from these rows.
5. **Judgement**: lizard and train at the default pose and close,
   snapshot diff (threshold 0 against HEAD must be at the noise floor;
   the new importance order is a ranking change the diff shows only
   under a budget) and the human flip on the thinned pairs; the lizard
   at a 100k and a 300k budget against scratch/renders. The train must
   not get worse at any rung.
6. Close: design doc, AGENTS.md, tiny.md's gate line, the loose-ends
   item's 16-bit bullet; Findings cut to notes.

## Findings

