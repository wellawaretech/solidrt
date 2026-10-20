---
title: Batch the spatial flush's entry writes and index the raster's draw list
description: A whole-tree flush sent one params message per moved node (a string and two vectors each) and the raster found every entry by a linear scan of the target's list, quadratic in the entry count; now one PlaceDraws command per target per flush over fixed-size records, and an id index on the draw list.
created: 2026-10-21
completed: 2026-10-21
---

# Batch the spatial flush's entry writes and index the raster's draw list

Prompted by a reading of the whole-tree bench in
okf/done/3d-large-world-precision.md (3000 nodes under one rotating group,
every node recomputed per frame): the walk's per-node cost was said to be
the params write, not the math - one `UpdateDrawParams` per moved node, a
`String` and two `Vec`s each, and on the raster side `entry_mut` finding the
entry by a linear scan of the target's `Vec<DrawEntry>`, so a flush of 3000
entries did about 4.5 million id comparisons per frame on the raster thread.

## What landed

- `gl::entry::DrawList`: the mesh target's draw list, a `Vec<DrawEntry>`
  (list order is draw order) with a `HashMap<u64, usize>` index. Every
  mutation goes through it (push, insert-before, remove, reorder, rebuild),
  each re-indexing the entries it moved, so the index cannot drift. Every
  per-entry raster command (`UpdateDrawParams`, `SetDrawRange`,
  `SetDrawBuffers`, `UpdateDrawTextures`, `RemoveDraw`, the shift, the
  gauge reset) now finds its entry in one lookup. The scan is gone for all
  of them, not only for the spatial walk.
- `gpu::DrawPlacement` and `RasterCmd::PlaceDraws`: one draw entry's share
  of a flush is a fixed-size record - `uModel`, `uNormal` and `uLodFade`,
  each present when this flush changed it - and a target's placements go
  down the channel as one command per target per flush. The spatial core's
  per-entry vocabulary is exactly those three names (`SinkWriter` documents
  them), so the record is the honest shape of the protocol, not a
  special case; the names live once, as `DrawPlacement::MODEL`/`NORMAL`/
  `FADE`, shared by the bind-time validation and the raster-side fold.
- `context::spatial::Writer` collects placements per target
  (`write_params`, `write_fade`), validating each against the mirror as it
  is collected so a gone entry still releases its binding at that write,
  and `finish` sends them; `shift_params` sends the target's pending
  placements before the shift so a target's writes keep their call order
  whatever the core does. Both writer sites (`spatial_flush`,
  `spatial_set_count`) end in `finish`.
- `merge_record` (every params write, JS included) and the placement fold
  overwrite an existing array value in place when the component count
  matches, instead of cloning a fresh `Vec` per param per write.

Kept per entry on purpose: `write_count` (the visibility switch) is an
edge-triggered range update through `Context::update_draw`'s validated
transaction (bounds, order registry, retained re-sort), and `write_param`
(rig curves into arbitrary scalar names) is a general params write. Both
now cost one index lookup on the raster; neither is the per-moved-node
write.

## Findings

- The bench's "flush ms" is a main-thread timer around `flush()`, and the
  raster channel is an unbounded `mpsc::Sender`, so a send never waits on
  the raster thread. The quadratic scan therefore never appeared in the
  flush ms figures (2.2 us per node on the desktop, 32 us on the ARMv7 TV):
  it ran on the raster thread and showed as raster occupancy and frame
  time. What flush ms held was the main-thread share: the f64 walk and its
  gates, plus three to five heap allocations and an mpsc node per moved
  node. Expect the batching to lower flush ms moderately and the index to
  lower raster-thread time per frame separately; the two halves land in
  two different numbers.
- `probes/scene-walk-whole-bench.tsx` remains the protocol (release
  clients, cadence hold off on devices, no concurrent builds on the
  desktop); read the raster occupancy from `/stats` beside its flush ms
  when re-measuring, since that is where the removed scan lived. The
  re-measurement on the three devices is listed in okf/tiny.md.
