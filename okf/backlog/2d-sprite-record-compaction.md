---
title: Record compaction for the live sprite layer, a view mask on the ordered gather
description: A sprite layer draws every record whether or not any view sees it (an off-screen record is one degenerate quad in the vertex stage), which stops being free at hundreds of thousands of records; the fix is a mask on the core's ordered gather so instanceCount shrinks to the visible prefix, re-evaluated whenever the camera or any pose moved, switched on past a measured record count
created: 2026-10-14
---

# Record compaction for the live sprite layer

Part 7 of [2d-baked-layers](../done/2d-baked-layers.md), split out when
stage B2's residency pieces landed: its premise ("one core query per
camera move") does not hold, and its threshold is unmeasured, so it is
shaped here rather than stubbed there.

## Symptom

The sprite layer is one draw entry over the whole population: the flush
publishes the used prefix and `instanceCount` covers all of it. A record
outside every view still costs one degenerate quad in the vertex stage
(zero fragments). Decided 2026-09-06: the frustum gate flips an ENTRY's
instance count, so wiring it to a 2d camera would cull the whole layer or
nothing; a sprite is a record, not a draw (Unity, Godot and Three cull per
object because a sprite IS a draw there; for an instanced population none
of them culls per instance - it is the user's job, GPU culling in Unity's
BRG). Nothing to win at tens of thousands; at hundreds of thousands the
vertex stage pays for the invisible.

## Done looks like

- **A mask on the ordered gather.** The core's instance order
  (`alloy/src/gpu/order.rs`, the registry in `context/order.rs`) permutes
  every instance-step buffer of an entry into key order at publish; a
  mask restricts the permutation to the records the views see, and the
  entry's instance count follows the mask's length. One additive verb,
  `setInstanceMask(target, draw, slots | null)`, on an ordered entry;
  the gathered, retained and index materializations all honor it. A
  layer without `orderBy` needs an order to gather under: an identity
  key (`OrderKey::Slot`, stable slot order) so compaction does not force
  a y-sort.
- **The query.** The visible set is `spatial.overlap` over the union of
  the view rects (a rotated view as the AABB of its rotated viewport),
  node ids to slots through the layer's map. It must run whenever the
  camera moved OR any record's pose moved since the last mask - a sprite
  walking into a still view must appear - so a live world re-queries
  every flush that moved something; the core knows the dirty poses, so
  the re-query is gated by that, not by the camera alone.
- **The switch.** `createSpriteLayer({ compact: true })`, off by default:
  on only past the record count where the gather and the query are
  cheaper than the vertices they save. Measure that threshold on the
  tiled-GPU tier when the first population of that size arrives; the
  number goes in the docs beside the option.
- **Picking unchanged.** Pick resolves by record order (the known
  limitation under `orderBy` stands); the mask is draw-only.

## Involves

`alloy/src/gpu/order.rs` (the mask in `order_permutation`),
`alloy/src/context/order.rs` (per-entry mask state, validation),
`flux/src/alloy_plugins/gpu.rs` + flux-types (`setInstanceMask`),
`packages/2d/src/layer.ts` and `views.ts` (the query, the switch, the
count), `packages/2d/AGENTS.md`.
