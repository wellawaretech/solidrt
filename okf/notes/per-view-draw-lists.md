---
title: Per-view draw list measurements and traps
description: What building per-view draw lists established on the 30,703-record board (Adreno 610 tablet) - the baseline, the cost of the rebuild point and its patch, why a patch follows changed records and is one buffer write, the churn floor of culling, the record layer's cull default, and the Adreno pass timer that attributes rather than totals.
created: 2026-11-07
---

# Per-view draw list measurements and traps

From [per-view-draw-lists](../done/per-view-draw-lists.md); the design is
[draw-order-and-culling](../design/draw-order-and-culling.md).

## Stage 0: baseline (2026-11-07)

`probes/record-layer-order-probe.tsx`, driven by
`probes/record-layer-order-bench.py` (its header has the protocol), on
the Galaxy Tab A7 (Adreno 610),
Player.dev built from the checkout at eead8ede plus the staged tree,
release. Cadence hold off (`SOLIDRT_CADENCE_HOLD=off`), governor
schedutil on both clusters, thermal status 0 and battery 19.5 to 20 C on
every row, window 1333 x 728 at scale 1.5, oversample 1.5. Each
scenario is the probe's debug command over 3 s, read from
`/stats?window=3000` right after; three runs each, median shown (the
runs agree within 0.5 ms of JS and 1.3 ms of GPU). JS is the window's
`p50Ms`/`p95Ms`, the UI thread's frame work. 141 frames in 3 s is the
full 47 Hz the GPU allows with the stock material.

| config | pan JS p50 | pan GPU | pan vertices | reveal JS p50 | p95 | reveal GPU | frames in 3 s | missed |
|---|---|---|---|---|---|---|---|---|
| none | 0.34 ms | 16.6 ms | 245,624 | 3.1 ms | 4.9 ms | 17.4 ms | 141 | 10 |
| `orderBy: "y"` | 0.36 ms | 16.7 ms | 245,624 | 8.6 ms | 13.4 ms | 16.0 ms | 131 | 18 |
| `cells: 512` | 0.44 ms | 15.4 ms | 10,040 | 16.0 ms | 23.9 ms | 23.5 ms | 74 | 83 |
| both | 0.43 ms | 15.8 ms | 21,216 | 15.5 ms | 24.4 ms | 22.7 ms | 78 | 79 |

- The probe reproduces the board's pattern: the reveal costs 2.8 times
  the unordered JS under `orderBy` and 5 times under `cells`, and with
  `cells` the reveal drops to 25 fps.
- The unordered reveal is 3.1 ms here, not the board's 1 ms: the
  cascade is a ring over a row-major grid, so its one dirty range spans
  about 23,500 records (2.4 MB) per frame. The target for the new path
  stays this row, measured the same way.
- New: under `cells` the reveal also costs GPU. The pass goes from 4.5
  to 7.9 ms and the frame from 15.4 to 23.5 ms, where `orderBy` alone
  adds none. The whole-prefix re-upload of every instance buffer each
  frame is the likely cause; the pan, which uploads nothing, does not
  show it.
- The pan's GPU gain from `cells` is 1.2 ms here (16.6 to 15.4), not the
  board's 5.7: the stock material's fragments are cheap, so the frame
  is not bound where the board's is. Vertices fall 24 times.

## Stage 1: the pure half (2026-11-07)

`alloy/src/gpu/draw_list.rs`, tests in `alloy/src/tests/draw_list.rs`
(`cargo test -p alloy --lib`: 948 pass).

- `CellIndex` (one per population): the slots per cell of a residency
  `Lattice`, by cull box center, updated from a dirty range of the
  `CELL_BOX_FLOATS` column. An empty box leaves its cell; a slot past
  the column reads empty, so a shrink passes the range it lost. Each
  cell's bound is the union of its members' boxes and their largest
  reach. It only grows while the cell has members and resets when the
  cell empties: too large a bound costs per-record tests, never a
  record.
- `DrawList` (one per view entry): a cell is skipped when its grown
  bound misses the view, taken whole when its bound lies inside the
  view, and tested per record otherwise. A bound that is not finite,
  or that reaches to or behind the eye with a screen reach, is tested
  per record: `reaches` tests such a box ungrown, which does not cover
  its members. The visible bits are scanned in slot order, then sorted
  by the key when there is one (`OrderScratch::sort_slots`, the stable
  radix). `touches(dirty, ...)` is the rebuild gate: a dirty slot was
  visible, or its new box reaches the view.
- `gather_list` writes a zero record for a slot past a buffer, and a
  slot past the key buffer sorts as key 0, so every buffer of an entry
  stays aligned on the list.
- Tests: paint order across cell boundaries, an edge cell's per-record
  test and screen reach, stable ties both ways, equality with a
  brute-force reference over 4,000 random records through 12 rounds of
  writes and shrinks (seven views, one with the eye on the plane, three
  orders), the updated index against a rebuilt one, `touches`, the
  aligned gather.
- `mod draw_list` carries `#[allow(dead_code)]` until stage 2 consumes
  it; stage 2 removes the allow.

## Stages 2 and 3: lists in the Context, the gathered paths retired (2026-11-07)

Done as one change. Sending every app-written order to a list made the
gather at publish, the cell readers and spans, `OrderKey::Cell` and the
bands unreachable; keeping them compiling until stage 3 would have been
the shim the plan rules out.

- **Model** (`gpu/order.rs`): `InstanceOrder::key` is optional (cells
  alone draw in slot order), and `cells` is `{ lattice, extent }` beside
  any key. Until stage 4 renames the options, `from_options` reads
  today's shape: a number is square cells `[b, b, 0]`, an `[x, y, z]` a
  lattice, with or without a key.
- **Registry.** The indexed form stays in `context/order.rs`; the lists
  are `context/draw_list.rs`, over the same buffer mirrors. A write to a
  buffer a list reads (`writeBuffer`, the lease, a spatial sink with its
  boxes) is a dirty range into the mirror; it reaches the GPU only while
  an entry draws the buffer directly (the set is recomputed at the
  rebuild point after any entry change; a buffer that becomes direct
  gets one full upload). A list's first attach seeds a missing mirror by
  one readback and asks the spatial core to republish the buffer's
  staging whole, so sink-fed boxes arrive. When the last list goes, the
  mirror's records go back to the GPU buffer.
- **Rebuild point:** `Context::flush_draw_lists`, at the top of
  `apply_content_changes` (after the spatial flush, before the frame
  takes its content changes) and in `render_target`. Own buffers carry
  the label "draw list", start at 256 records and grow to the next power
  of two.
- **Facts:** `/gpu` reports `list: { population, visible, rebuilds,
  buildMs }` per entry (`instanceSpans` and `instancesDrawn` are gone);
  `/stats` reports `drawLists`, `drawListRebuilds`, `drawListVisible`,
  `drawListMs`. `packages/flux-types/gui/gpu.d.ts` already describes the
  lists (the meaning of `cells` changed in this stage); its names change
  in stage 4.
- **Verified** on the release dev client: alloy lib 942 and flux lib 83
  pass; the 3d tests (257) pass, but none of them declares an instance
  order, so stage 6 is where 3d lists get tested. The 2d tests pass
  (194, `records.test.tsx` with `orderBy` among them) except 5 in
  `view-culling.test.tsx`, all on one assertion: the culled view's
  pixels equal the unculled twin's byte for byte, and the list draws 4
  records where the cell selection drew 9, because an edge cell's
  records are tested one by one. Stage 5 updates those counts.
- **Until stage 5:** a 2d layer's second view is a reader with no order,
  so it draws its records unculled from the source buffer (now a
  direct reader, so it is uploaded), and its `orderFeed: "cells"` write
  fails ("the entry has no instance order"), which drops that feed with
  one warning.

## Stage 4: flux and types (2026-11-07)

- **Names as planned**, with one change: the switch is `cells.cull`,
  not a `cull` beside `cells`, because `createPipelineTexture` (and the
  pipeline options) already take `cull: "none" | "back" | "front"`,
  the face culling mode, on the same object (the Names section above
  is corrected). `instanceOrder` holds the key alone (`retain` gone);
  `cells: { size, origin?, extent?, cull? }` sits beside it on
  `createShaderTarget`, `createPipelineTexture` and `addDraw`; an entry
  with cells and no key culls in slot order with `cull: true`, and
  draws its buffers directly without it. An absent `cull` is off until
  stage 7's policy.
- **Updates and feed:** `setDraw`/`setDrawRange` take `view` and
  `viewport` (`orderDirection`, `orderLod` stay); flux:spatial
  `bindDraw` takes `viewFeed: "order" | "cull"`. In Rust, `DrawSink`
  has one `view_feed: Option<ViewFeed>`, `SinkWriter::write_view`, and
  `DrawUpdate` has `view` and `viewport`.
- **Alloy model:** `OrderOptions { key: Option<KeyOptions>, cells }`,
  `from_options` returns `None` for nothing declared; `Cells` carries
  `cull`; `InstanceOrder::culls()` is the one place the switch is read.
- **Call sites renamed mechanically** so the tree builds and typechecks
  (`bun run sol check`: 280 entries): core's create wrappers pass
  `cells`; the 2d views take a `ViewOrder { instanceOrder?, cells? }`
  (layer.ts and records.ts declare `cells: { size, cull: true }`, the
  behavior they had); 3d `scene.ts` binds `viewFeed: "order"` and stops
  forwarding `retain` (mesh.ts still accepts it: stage 6);
  `probes/retain-probe.tsx` loses `retain`. The flux-types declarations,
  `okf/design/splats.md` and the docs follow.
- **Verified** on the rebuilt release dev client: alloy lib 942, flux
  lib 83, the 3d tests 257 pass; the 2d tests are exactly stage 3's (194
  pass, the same 5 counts in `view-culling.test.tsx`).

## Stage 5: core and 2d (2026-11-07)

- **Writes.** `RecordStream.publish(whole, ...)` is whole only for a
  grown buffer's first fill. Both layers publish their dirty range under
  any order; the record layer's second publish after a growth swap is
  gone.
- **Views.** Every view's entry declares the layer's order and cells;
  `ordered` and `rehome` are gone. `ViewDeps.cells` is `culls`.
- **Option.** `cells?: { size }` is `cull?: boolean | { cellSize }` on
  both layers and `<SpriteLayer>`; `true` takes `CULL_CELL_SIZE` (512,
  layer.ts). `checkCells` is `cullCellSize`, a size check only: every
  `orderBy` combines with `cull`. Absent is off until stage 7. The probe
  maps its `cells` config to `cull: { cellSize }`, so the bench script
  is unchanged.
- **Pick (D7).** Hits sort by the key, stable on the slot order: the
  record layer by its field (`descending` flips it), the node layer by
  `renderOrder` or each hit's world y. D7 in the design is corrected.
- **Alloy fix.** A list read its cull boxes from the key buffer's
  mirror. Under `orderBy: "renderOrder"` with `cull` the key is in the
  sprite records (buffer 2) and the sink stages the boxes beside the
  poses (buffer 1), so every record read as boxless and the culled view
  drew nothing. `ListEntry::box_slot` is now separate: the key buffer
  when the boxes are its records' fields (`extent`), else the first
  instance-step buffer.
- **Docs.** `layer.ts`, `records.ts`, `views.ts`,
  `packages/2d/AGENTS.md`, `agents/views.md` and `agents/tiles.md` lose
  the cell-boundary trade, the gather costs and the pick limitation.
  The old measured costs are gone from views.md until stage 8 measures
  the new path.
- **Tests.** `view-culling.test.tsx`: an edge cell's records are tested
  one by one, so the 2x2 block a 200 px view shows is 4 drawn (9 under
  cell selection). New: painter order across cell boundaries in slot
  order, under `y` and under `renderOrder` (the last found the alloy
  bug); a record layer's descending field order culled; the cell size
  check; pick order on both layers, with a `renderOrder` tie falling back
  to slot.
- **Verified** on the rebuilt release dev client: `bun run sol check`
  (280 entries), the 2d tests 203 pass, alloy lib 942, flux lib 83.


## Stage 6: 3d (2026-11-07)

- **Writes.** `publishRecords` sends the dirty range on an ordered mesh
  too. `markLiveRecords` (the whole re-seed at attach) and the prefix
  republish in `setRecordCount` are gone: a list's first attach seeds
  its mirror itself, and a count change is a population change the
  list rebuilds on.
- **`retain` gone** from `InstanceOrderOptions`, `ResolvedInstanceOrder`
  and the transfer path (no forced retain).
- **Views.** Since stage 2 only the scene entry declared the order, so
  a `scene.createView` drew an ordered mesh in slot order. Every view
  but a shadow view now declares it and is fed its own camera: each
  sorts under its own view. A shadow view draws unordered, as a
  depth-only pass gives the same result in any order. The index form
  (splats) keeps one id stream sorted for the scene's camera, read as
  it is by other views:
  [splat-order-per-view](../backlog/splat-order-per-view.md).
- **Tests.** `packages/3d/tests/instance-order.test.tsx`: a field key,
  re-ordered by a one-record write, ties on slot; a position key drawn
  by the scene's camera and a view from behind, each back to front.
  The first 3d tests that declare an order.
- **Docs.** `mesh.ts` and `agents/instancing.md` lose `retain`, the
  whole publish and "one buffer holds one order".
- **Verified:** `bun run sol check` (3d, 82 entries), the 3d tests 259
  pass. `probes/order-3d-probe.tsx` is deleted: its check read the
  record buffer back through `/buffer`, which now stays in slot order,
  and the new test covers what it verified.

## Review of stages 5 and 6 (2026-11-07)

- **Box source.** The stage 5 fix read sink boxes from the first
  instance-step buffer but `extent` boxes from the key buffer, and the
  flux-types contract said sink boxes come from the key buffer, which a
  JS-written key buffer (2d `renderOrder`) can never hold. One rule
  now: the cull boxes always come from the first instance-step buffer
  (its fields under `extent`, else its sink), whatever buffer holds the
  key. `draw_list.rs`, the `Cells` and `key_buffer` docs in
  `gpu/order.rs`, and `gpu.d.ts` (`InstanceCells`, the several-buffers
  paragraph) say so.
- **3d override views.** Stage 6 made every non-shadow view declare the
  order; an override whose material does not bind the key stream would
  have thrown in `entryInstanceOrder`. Such a view now draws unordered:
  it has nothing to sort by.
- **Stale text** left by stage 5: `LayerBase.pick` still said the key is
  not consulted; the sprite records comment in `layer.ts`,
  `agents/records.md` (twice) and the views paragraph of
  `agents/views.md` still described the whole-prefix gather.
- **Verified** on the rebuilt release dev client: alloy lib 942,
  `bun run sol check` (281 entries), the 2d and 3d tests 462 pass.

## Stage 7: culling on by default (2026-11-07)

- **D5 revised** (agreed): the default is the layer API's, as Three's
  `frustumCulled` is `Object3D`'s; no lattice `CullPolicy`, no
  population threshold, no live switch. Both 2d layers cull by default;
  `cull: false` opts out, `{ cellSize }` tunes. The record layer takes
  the same default: its sprites are sprites to the app (handles,
  frames, pick, clamps), and its main user is the static board where
  culling wins most. Stage 8 measures the per-frame demos (starlings)
  on and off; if the patch does not bring them near parity, the record
  layer's default flips to off with no API change.
- **In-place patch** (`gpu/draw_list.rs`): each build keeps every
  listed slot's position and every position's key. Under an unchanged
  view and population, `DrawList::change_culled` / `change_all`
  classify a write: a slot joining or leaving the drawn set, or a drawn
  slot's key moving, rebuilds; otherwise the drawn written slots'
  positions are patched (`ListChange::Patch(positions)`), re-gathered
  by `Context::patch_list` with one write per buffer. `touches` is
  gone. Facts: `list.patches` in `/gpu`, `drawListPatches` in
  `/stats`.
- **Cull margin**: `Cells::margin` (flux `cells.margin`, record units,
  `with_cull_margin`), applied by `CellIndex::read` everywhere a box is
  tested (the index, the build's edge cells, the change check). 2d:
  `shaderMaterialClass({ cullMargin })` (layer pixels, validated), the
  layers pass `material.cullMargin`.
- **Found by the new default**: `views.setCount` on a culling layer
  with no view (none yet, or all disposed) threw "spatial node has no
  draw sink"; it now tells the sink only while a view exists, and a
  later view binds with the published count.
- **Tests.** Alloy: `change` patches a kept drawn set and rebuilds on a
  join, a leave, an emptied box and a moved key; the margin grows
  every box the index and the list test; a property test keeps six
  lists (two views, three orders) through 200 rounds of jitter, jumps
  and key writes and checks every kept or patched list against a fresh
  build, every written drawn slot inside its patch, and both paths
  taken; `with_cull_margin` validation. 2d: the default culls on both
  layers and `cull: false` draws all; a write that keeps a sprite drawn
  patches (rebuilds unchanged, pixels equal the unculled twin) and one
  that moves it out rebuilds; a shifted vertex stage draws with its
  `cullMargin` and is culled without it.
- **Docs.** `layer.ts`, `records.ts`, `material.ts`, the component,
  `gpu.d.ts` (`InstanceCells.margin`), `packages/2d/AGENTS.md`,
  `agents/views.md`, `agents/materials.md`,
  `packages/cli/agents/debugging.md`, the design's D3 table and known
  limits.
- **Verified** on the rebuilt release dev client and flux binary:
  alloy lib 945, flux lib 83, `bun run sol test` 1006 pass (154 files).

## Stage 8: measured (2026-11-07)

The probe gains `churn` (the whole board in view, every tile moved by up
to 3 px each frame, every record published: the starlings workload at
30,703 records; starlings itself caps at 1,000 boids and sheds on a
phone, too small to show a difference), and its `cells: null` config
now means `cull: false`. Galaxy Tab A7 (SM-T500, Adreno 610),
Player.dev built from this tree (18:21), release, cadence hold off,
schedutil on both clusters, thermal status 0 and battery 22.5 to 23.0 C
throughout (stage 0: 19.5 to 20 C), window 1333 x 728 at scale 1.5,
oversample 1.5. Three runs each; the runs agree within 0.3 ms of JS and
3 frames except where noted. `list` is `drawListMs` sampled mid-run (the
rebuild point, on the UI thread, NOT in the JS figure).

| config | pan GPU | pan verts | reveal JS p50 / p95 | reveal frames | reveal list | churn frames | churn list |
|---|---|---|---|---|---|---|---|
| `cull: false` | 16.7 ms | 245,624 | 3.7 / 4.7 ms | 135-141 | - | 40-44 | - |
| `orderBy: "y"`, unculled | 16.5 ms | 245,624 | 3.6 / 4.6 ms | 82-89 | 9.8 ms | 37 | - |
| culled (default) | 13.8 ms (one run 16.6) | 5,600 | 3.8 / 4.9 ms | 118-129 | 6.0 ms | 37-38 | 6.9 ms |
| culled, `orderBy: "y"` | 13.7 ms | 5,560 | 3.8 / 5.1 ms | 116-120 | - | 37 | 8.3 ms |

Stage 0 for the reveal, same probe: JS p50 3.1 / 8.6 / 16.0 / 15.5 ms
and 141 / 131 / 74 / 78 frames in the same row order.

- **The pan** keeps the culling win: 16.7 to 13.8 ms GPU (stage 0: 16.6
  to 15.4) and 44 times fewer vertices; the per-frame rebuild under a
  moving camera costs 0.1 ms of JS.
- **The reveal's JS** is now the unordered layer's under every option
  (3.6 to 3.8 ms p50, p95 under 5.1): the whole-prefix gather is gone
  from the write. The culled rows went from 74 to about 120 frames.
- **But the rebuild point costs 6 to 10 ms per frame in the reveal**,
  outside the JS figure, and `orderBy: "y"` unculled regressed from 131
  to 85 frames. The cause: the reveal writes about 300 records per frame
  but publishes them as ONE dirty range of about 23,500 slots (a ring
  over a row-major grid; `updateRecords` takes one range), and every
  per-slot step at the rebuild point walks that span: the extent boxes
  (`refresh_boxes`), the cell index (`update`) and the patch-or-rebuild
  check (`change`). Under a key the patch then spans nearly the whole
  list (the touched positions are scattered in key order), so about
  3.2 MB is re-gathered and uploaded per frame. Stage 0 paid the same
  span in its gather; the draw list was meant to make the cost follow
  the write, and with a coarse range it follows the range instead.
- **The churn** (every record changes): culling costs 6.9 ms per frame
  (8.3 with the y order) over a 70 ms frame (the probe's own JS loop is
  28 ms and dominates), 40-44 frames to 37. That is the per-record
  floor of the patch path at 30,703 records, about 0.23 us a record on
  this tablet; at starlings' 1,000 it is about 0.2 ms.
- **The churn's GPU** reads higher culled (pass 16.7 against
  9.1 to 12.3 ms unculled) with 3 times fewer vertices; not explained
  yet (the list's 1 MB upload per frame is one candidate).

Open, to decide before the record layer's default is final (stage 7):
the rebuild point should follow the records that changed, not the
range marked (the mirror can find them while it copies the write: a
record whose bytes did not change is not dirty), and a patch should
upload runs of changed positions, not their whole span.


## Stage 8 follow-up: the rebuild point follows what changed (2026-11-07)

- **Changed records, not the marked range** (`context/order.rs`
  `ChangedSlots`, `context/draw_list.rs` `Mirror::write_bytes` /
  `write_boxes`): a write into a list mirror compares each record it
  covers with what the mirror holds and marks only the records it left
  different (a sink's boxes likewise per slot); a seed, a hand-off and a
  new box source still mark every record. The mirror's byte range is
  gone. `refresh_boxes`, the cell index update and the patch-or-rebuild
  check walk the changed slots (gathered per list, ascending, each
  once), so the reveal's 23,500-slot range costs its 300 changed
  records plus one comparison pass over the bytes.
- **Patches in runs** (`gpu/draw_list.rs`): `ListChange::Patch` carries
  runs of positions; positions at most `PATCH_GAP_RECORDS` (8) apart
  share a run, and past `PATCH_MAX_RUNS` (64) runs the gap doubles until
  they fit. `Context::patch_list` writes one run per buffer write, so a
  keyed list's scattered patch uploads the changed records, not their
  span. (Superseded: a patch is one span and one write per buffer, see
  "One write per patch" below.)
- **Tests.** Alloy: the runs (apart, merged, a scattered set capped and
  covered), the property test against runs. 2d: a whole-range
  `updateRecords` with nothing changed costs no patch and no rebuild;
  two far-apart records retinted under `orderBy: "y"` through a
  whole-range publish patch once, with the unculled twin's pixels.
- **Verified** on the rebuilt release dev client and flux binary: alloy
  lib 946, `bun run sol test` 1007 pass. The tablet re-run is pending.

## Stage 8 re-measured, with the follow-up (2026-11-07)

Same tablet and protocol; Player.dev from the follow-up tree (18:44),
battery 18 % on USB power (no battery saver, clocks uncapped at 1.80 /
2.02 GHz), thermal status 0 and battery 23.0 C on every row. The bench's
window now covers the whole 3.6 s scenario from its start (the first
re-run's 3 s window ended 3.6 s in and missed a culled reveal's only
visible half second, so it read 0 to 3 frames and the bench aborted).
Frames are per 3.6 s; `list` is `drawListMs` sampled 1.5 s in. Three
runs each, one session (the `tint` rows from a second session at once).

| config | pan GPU | reveal frames | reveal JS p50 | reveal list | churn frames | churn list | tint GPU / pass |
|---|---|---|---|---|---|---|---|
| `cull: false` | 16.6 ms | 163-170 | 3.1 ms | - | 50-53 | - | 17.0 / 3.7 ms |
| `orderBy: "y"` unculled | 15.7-16.6 ms | 131-164 | 3.5-3.9 ms | 2.1-3.0 ms | 42-44 | 10.8-11.8 ms | - |
| culled (default) | 14.9-16.7 ms | 32-39 | 3.2-3.7 ms | 0 | 43-44 | 5.6-5.8 ms | 15.9 / 4.9 ms |
| culled, `orderBy: "y"` | 14.6-16.7 ms | 38 | 3.1-3.8 ms | 0 | 43-44 | 6.4-6.9 ms | - |

- **A culled reveal does no work once the cascade leaves the view**: it
  shows a change in its first half second only (32 to 39 frames, none
  missed), and its off-screen writes cost no patch (`list` 0 at 1.5 s).
  Under culling, reveal frames count the frames that changed the
  picture, not throughput.
- **The y order unculled**: the reveal's list time fell from 9.8 to 2.1
  to 3.0 ms and its frames from 82-89 per 3 s to 131-164 per 3.6 s,
  against 163-170 unordered: better, not yet the same.
- **The churn** (30,703 records, every one visible and rewritten each
  frame): culling costs 5.6 to 6.9 ms of rebuild point per frame and
  about 1 ms of GPU, 43-44 frames against 50-53 (14 % fewer); at
  starlings' 1,000 boids that is about 0.2 ms. The y order unculled
  sorts the population each frame (10.8 to 11.8 ms), as D3 says it
  must.
- **The culled pass timer reads higher than the unculled one** (pan 4.5
  against 3.4 ms, tint 4.9 against 3.7) with 45 times fewer vertices,
  while the frame-level GPU reads lower (tint 15.9 against 17.0, pan
  14.9 against 16.7 in the same session). The tint scenario uploads
  nothing, so the list's uploads are not the cause. Not explained; the
  frame figure of the culled pan also varied between sessions (13.8,
  about 16.5, 14.9 ms) where the unculled one held at 16.6.
- **Decided by these rows, not yet**: the record layer's default. Its
  documented use (a JS loop rewriting every record each frame) is the
  churn, where culling costs 14 % of the frames at the board's size.

## The record layer's default (2026-11-07)

Decided from the re-measure (agreed): the record layer defaults to
`cull: false`, the sprite layer keeps culling on. `cullCellSize` takes
the kind's default; `RecordLayerOptions.cull` documents it; D5,
`agents/views.md` and `agents/records.md` say so; the default test
checks both kinds and a record layer with `cull: true`. 2d tests 207
pass, `bun run sol check` passes.

## Far-apart views decided (2026-11-07)

Stage 8's last item:
[2d-sprite-layer-view-relative-records](../backlog/2d-sprite-layer-view-relative-records.md)
(was `2d-sprite-layer-chunked-records`). The per-view gather replaces
the region-cell chunking: the record sink stages f64 positions in the
list mirror and each list writes its records relative to its own
view's target origin. The mirror's f32 records cannot serve, since
they are already relative to the one origin and stepped at 10^7 px
from it. The item stays in the backlog, with the design's limit entry
updated.

## The y-order reveal's patch runs (2026-11-07)

A simulation of the probe's reveal over its grid, with the patch's run
rules: the y-ordered list keeps the tiles in row order (the zero-size
slots sort first at y 0), so a ring's 300 written records land on
about 92 runs at gap 8 once the ring is 60 frames out. Past
`PATCH_MAX_RUNS` (64) the gap doubles to 64 and the runs cover 3,500 to
5,800 records: 3,505 per frame on average for 292 written, 12 times the
change. Uncapped they would be about 110 runs covering 300. Each run is
one `RasterCmd::WriteBuffer` per buffer, each a `glGetIntegerv`, two
binds, a `glBufferSubData` into a buffer the previous frame may read,
and a scan of every shader for readers; the unordered layer sends one
write a frame. Which of the two (bytes covered or commands sent) costs
the frames is for the tablet to say.

## One write per patch (2026-11-07)

The order counters gain `list_patch_records` (`drawListPatchRecords`:
the list records the patches re-gathered), and the bench prints it per
patch beside the raster thread's command time (`rasterCmdMsPerSec`).
Galaxy Tab A7, Player.dev release from this tree, cadence hold off,
schedutil on both clusters, thermal status 0, battery 21.5 to 22.0 C,
on USB power. The `orderBy: "y"` unculled reveal, three runs each:

| patch | frames / 3.6 s | missed | frame GPU | raster | records per patch | writes per patch |
|---|---|---|---|---|---|---|
| runs, cap 64 | 140-144 | 38-60 | 18.6-21.2 ms | 166-185 ms/s | 3,151-3,216 | 27.5 |
| one span (cap 1) | 175-179 | 0-4 | 16.3-16.6 ms | 37-54 ms/s | 5,663-5,704 | 1 |
| `cull: false`, unordered | 165-171 | 10-12 | 17.2-17.4 ms | 119-120 ms/s | - | - |

A write into a buffer the GPU still reads costs per write on this
Adreno, not per byte: the one-span patch covers 1.8 times the records
and costs a quarter of the raster time and no frame GPU. The uncapped
runs were not measured; 110 writes a frame would be worse than 28. So
a patch is one span, from the first changed position through the last,
and one `WriteBuffer` per buffer: `ListChange::Patch(Range)`,
`PATCH_GAP_RECORDS`, `PATCH_MAX_RUNS` and the runs gone
(`gpu/draw_list.rs`, `context/draw_list.rs` `patch_list`). Tests:
`a_patch_spans_its_changed_positions` (slots and keyed positions), the
property test against the span; alloy lib 946 pass.

All four configs re-measured with the span (same session; frames per
3.6 s, `list` sampled 1.5 s in):

| config | pan GPU | reveal frames | reveal JS p50 | reveal list | churn frames | churn list | tint GPU / pass |
|---|---|---|---|---|---|---|---|
| `cull: false` | 16.2-16.7 ms | 163-175 | 2.8-3.1 ms | - | 51-53 | - | 17.0-17.1 / 3.8-3.9 ms |
| `orderBy: "y"` unculled | 16.5-16.6 ms | 172-178 | 3.7-3.8 ms | 2.2-5.0 ms | 43-59 | 9.8-12.0 ms | 16.6 / 3.8 ms |
| culled (default) | 14.9-16.7 ms | 38 | 3.1-3.6 ms | 0 | 43-44 | 5.7-6.7 ms | 15.4-16.0 / 5.0-5.1 ms |
| culled, `orderBy: "y"` | 14.6-14.7 ms | 32-39 | 3.2 ms | 0 | 42-44 | 6.9-7.4 ms | 16.0-16.1 / 5.0 ms |

The y order unculled now matches the unordered reveal (172 to 178
frames against 163 to 175, 2 to 3 missed against 10 to 14). The culled
churn patches every frame (all 10,235 tiles visible, none leaving), a
span of the whole visible list in one write, at the same frames as
before.

## The culled pass timer (2026-11-07)

The same probe on this machine's Intel Mesa GPU (RPL-P, which does not
bin; the 18:38 client), two runs each: the culled pass reads 0.26 to
0.39 ms in the pan and 0.34 to 0.36 in the tint against 0.73 to 0.80
and 0.80 to 1.22 unculled, and the frame-level GPU is the same for both
(1.0 to 1.7 ms). On the Adreno the culled pass reads about 1.2 ms higher
while the frame reads 1 to 1.5 ms lower. The culled draw is cheaper;
the Adreno's per-pass timer attributes the binned work differently with
45 times fewer vertices, and does not total it. Compare culled and
unculled on the Adreno by the frame-level figure (the design's
Measuring section says so).
