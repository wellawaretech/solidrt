---
title: A culled or ordered population pays its whole size on every record write, and culling changes its paint order
description: A 2d layer with orderBy or cells (and a 3d record mesh with an order) keeps its instance buffer in key or cell order, so a write to one record re-sorts and re-gathers the whole population (14 to 15 ms of JS thread per writing frame at 30,703 records on an Adreno 610 tablet, 1 ms unordered), cells reorder paint across cell boundaries and forbid renderOrder, and pick ignores the order; the fix is a draw list per view over its visible set, records left in slot order (okf/design/draw-order-and-culling.md, D2 to D7).
created: 2026-11-06
completed: 2026-11-07
---

# A culled or ordered population pays its whole size on every record write, and culling changes its paint order

## Symptom

A record layer of 30,703 hull records on an Adreno 610 tablet (Galaxy
Tab A7), animating a few hundred records per frame (a reveal's
cascade):

| layer | JS thread per frame, p50 | p95 |
|---|---|---|
| no order | 1 ms | 7 ms |
| `orderBy: "y"` | 7 to 14 ms | 19 ms |
| `cells: { size: 512 }` | 14 to 15 ms | 21 ms |

With an order or cells, a write to one record publishes the whole used
prefix (`stream.publish(true, count)`: a byte range has no fixed
position in a permuted buffer), and the core re-sorts and re-gathers
every record of every instance buffer of the entry
(`gather_for_publish` / `ordered_instance_publish`, `order_permutation`
or `order_cells`, `gather_permuted`, `republish_slots`), then
re-selects every reader of a cell-culled population. The cost follows
the population, not the write. `cells` was shipped with this cost
measured ([2d-sprite-view-culling](../done/2d-sprite-view-culling.md)):
it wins 5.7 ms of GPU per frame in a pan (41.7 to 36.0 ms) and loses
13 to 14 ms of CPU in every frame that writes records.

The same layout causes three more defects:

- `cells` without `orderBy` draws a grid of cells row by row, so two
  sprites overlapping across a cell boundary can swap: culling changes
  what draws over what.
- `cells` with `orderBy: "renderOrder"` or `{ field }` throws: the cell
  order and the draw order must be one physical order.
- `pick()` on an ordered layer walks slot order, so the topmost hit is
  not the sprite drawn on top (a documented limitation in `layer.ts`
  and `records.ts`).

## Decided

[draw-order-and-culling](../design/draw-order-and-culling.md), D2 to
D7: the buffer layout is never the cull structure. A population the app
writes, with culling or an order on, draws per view from a draw list:
records stay in slot order in the core mirror, a write is a dirty range
into it, cells are a membership index, and each view culls, orders
(slot order without a key) and gathers its visible records into its own
instance buffers on a frame where its view or a visible record
changed. Handed-off populations (splats) keep the index-stream instance
order. Alloy offers culling and order as opt-ins and reports facts;
the culling default is the layer API's (D5).

## Done looks like

- **Alloy.** A per-view draw list for an entry over a population with
  culling and/or an order: the membership index over the residency
  lattice, the visible set (cells, then record boxes in edge cells),
  the order (slot order or the stable radix by key), the gather into
  the view entry's buffers, the rebuild only on a view change or a
  write touching a visible slot. Facts per view: population, visible
  count, rebuilds and their cost (in the order counters and `/gpu`).
- **Writes.** A write to a culled or ordered population is a dirty
  range into the mirror, from the lease (`packages/core/src/gpu.ts`
  record streams, no `publish(true, count)` on an ordered stream) and
  from the spatial record sinks.
- **2d.** `cells` combines with every `orderBy`; without `orderBy` a
  culled layer paints in slot order. `checkCells` loses its order rule;
  the docs in `layer.ts`, `records.ts`, `packages/2d/AGENTS.md` and
  `agents/views.md` lose the cell-boundary trade and the pick
  limitation.
- **3d.** A record mesh with a field key, or a retained projected key,
  draws from the draw list; splats stay on the index stream.
- **Pick.** Hits on an ordered layer are ordered by the key, stable on
  slot order (D7).
- **Retired, after an inventory.** The cell-ordered gather and its
  spans (`OrderKey::Cell`, `order_cells`, `select_cells`, the cell
  readers, `SetDrawSpans` and the instance binding at an offset, if no
  other user remains), the gather-at-publish path for app-written
  populations, the per-publish re-select.
- **Default.** Culling on by default where D5 says, after the in-place
  patch and the cull margin it needs; `cull: false` opts out.
- **Measured** as the design's Measuring section says, on the same
  board and tablet: the pan keeps the 36.0 ms of `cells`, and the
  reveal with `cells`, with `orderBy: "y"` and `cells`, and with
  `orderBy: "renderOrder"` and `cells` is near the unordered layer's
  1 ms p50. `orderBy` without culling on a fully visible board is
  measured and stated, not promised: it sorts and gathers the
  population per writing frame by design (D3).

## Involves

`alloy/src/gpu/order.rs` (membership, visible set, the slot-order scan,
the sort reused), `alloy/src/context/order.rs` (the draw list per
view entry, the mirror as the write target, the gather into view
buffers), `alloy/src/spatial/mod.rs` (the order feed as the view
input; cell boxes from the record sinks), `alloy/src/raster/`
(removing span binding if unused), `flux/src/alloy_plugins/gpu.rs` and
`spatial.rs` with `packages/flux-types` (the opt-ins, the facts),
`packages/core/src/gpu.ts` (dirty-range publishes on ordered streams),
`packages/2d/src/records.ts`, `layer.ts`, `views.ts`, `pick.ts`,
`packages/3d/src/mesh.ts`, `lattice` (the culling default).

## Plan

No backwards compatibility: each piece retires in the stage that
replaces it, with no dual path and no shim.

### Decisions

1. **Per entry, independent.** Every view's entry declares the order
   and builds its own draw list. No ordering entry and readers, no
   re-homing, no "already ordered" error; two views may order
   differently.
2. **Writes are dirty ranges into the mirror.** `writeBuffer` accepts
   ordered buffers; the spatial sinks send their dirty range; a
   stream publishes whole only for a grown buffer's first fill. The
   source GPU buffer is uploaded only while an entry draws it directly
   (a list-only population never uploads it; a later direct entry gets
   one full upload from the mirror).
3. **One rebuild, at the latest point.** At `Context::submit` and
   `submit_clean`, after the frame's writes and the spatial flush, ahead
   of the `Frame` command; and before a manual `render_target` for its
   lists. A list rebuilds only when its view changed or a write touched
   a slot that was or becomes visible.
4. **Lists own their instance buffers** per instance slot, created by
   the context and grown by doubling. The raster entry binds them in
   place of the declared buffers and draws `[0, visible)`; the raster
   thread never learns ordering exists.
5. **The default is the layer API's** (D5, revised 2026-11-07): the
   2d layers declare `cull` to alloy explicitly; no lattice policy, no
   population threshold, no live switch. Superseded: a `CullPolicy` in
   lattice beside `CadencePolicy` with a `min_population` threshold.

### Names

- flux:gpu entry: `instanceOrder` holds the key only (field or
  position, `descending`, `buffer`, `indices`, `lod`); beside it
  `cells: { size, origin?, extent?, cull? }` (how to cut the population
  and read its boxes, a fact, with `cull` the opt-in; absent = lattice's
  default). `cull` sits inside `cells` because the entry options already
  have a `cull`, the face culling mode. `InstanceCellOrder` goes.
- Draw updates: `orderView` -> `view`, `orderViewport` -> `viewport`.
  `orderDirection`, `orderLod` stay.
- flux:spatial `bindDraw`: `orderFeed: boolean | "cells"` ->
  `viewFeed: "order" | "cull"`; in Rust `order_feed` and `cell_feed`
  -> one `view_feed`. The core picks the gates from it.
- `retain` goes (every list keeps its mirror; the index form always
  does).
- 2d: `cells?: { size }` -> `cull?: boolean | { cellSize }`; absent =
  the layer's default (D5); without `cellSize` a named 2d constant
  (256 to 1024 measured the same).

### Stages

Each stage leaves the tree green. `free -g` before every cargo build,
`CARGO_BUILD_JOBS=8`.

0. **Baseline** on the tablet with today's build: the pan and the
   reveal, each with no option, `orderBy: "y"`, `cells`, and both;
   CPU governor and thermal status recorded.
1. **Alloy, pure half** (`gpu/draw_list.rs`, tests in
   `src/tests/draw_list.rs`): the membership index over the residency
   `Lattice` from a dirty slot range and its boxes; the visible set
   (cells against the view, then record boxes in edge cells with the
   `minScreenPx` reach); the slot-order bit scan; the stable keyed radix
   over the visible slots; the gather. Tests: painter order kept under
   culling, stable ties, equality with a brute-force reference.
2. **Alloy, context:** lists per entry, dirty writes into the mirror,
   the rebuild points, the private buffers, the facts per entry
   (population, visible, rebuilds, build time) in `OrderCounters` and
   `/gpu`. The 3d record mesh's field key and projected key move here.
3. **Retire:** `OrderKey::Cell`, `order_cells`, `select_cells`,
   `Cells`/`CellPacker`; `CellReader`, `readers`, `Reselect`;
   `RasterCmd::SetDrawSpans`, `set_entry_spans`, `clip_spans`, the span
   loop in `gl/pass.rs`, `instance_spans` (`point_instances` stays, the
   slicer's); the gather-at-publish paths for app-written records and
   their sibling republish (kept for the index form); the gathered
   `by_buffer` owner rule. `cargo test -p alloy`.
4. **Flux and types:** the names above in `alloy_plugins/gpu.rs`,
   `spatial.rs`, `gpu.d.ts`, `spatial.d.ts`; `writeBuffer` on ordered
   buffers.
5. **Core and 2d:** dirty-range publishes (`packages/core/src/gpu.ts`,
   `records.ts`, `layer.ts`); every view declares the order
   (`views.ts`, no `ordered`, no `rehome`); `checkCells` order rule gone;
   pick sorts hits by key, stable on slot (the node layer reads world y
   per hit; correct D7 in the design); `cull` option; AGENTS.md and
   `agents/views.md`; `view-culling.test.tsx` gains painter order under
   culling, culling with `renderOrder`, pick order. `bun run sol test`.
6. **3d:** `mesh.ts` dirty publishes, `retain` gone; splats unchanged;
   the 3d tests and `probes/3d-packed-records-probe.tsx`.
7. **The default and what it needs:** in alloy, a write that keeps a
   record visible and its key unchanged patches it in place in each
   list (a slot-to-position map kept at rebuild; anything else
   rebuilds); a cull margin on the cells, and `cullMargin` on a 2d
   material whose vertex stage moves geometry beyond the record's rect;
   then culling on by default per D5, with `cull: false` the opt-out.
8. **Measure** as stage 0; results into the design's Measuring
   section; decide
   [2d-sprite-layer-view-relative-records](../backlog/2d-sprite-layer-view-relative-records.md)
   (a per-view gather from the core's f64 poses may replace its
   chunking for node layers).

## Findings

Cut into [../notes/per-view-draw-lists.md](../notes/per-view-draw-lists.md).
