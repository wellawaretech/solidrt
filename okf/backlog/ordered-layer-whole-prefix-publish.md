---
title: An ordered or cell-culled layer re-gathers its whole population on every record write
description: A record or sprite layer with orderBy or cells publishes its whole used prefix and re-sorts it core-side on every frame that writes any record, about 13 ms of the JS thread per such frame at 30,703 records on an Adreno 610 tablet's CPU (1 ms without an order); wanted a publish that costs what changed when the order did not move, and only the moved records' share when it did.
created: 2026-11-06
---

# An ordered or cell-culled layer re-gathers its whole population on every record write

## Symptom

A record layer of 30,703 hull records on an Adreno 610 tablet (Galaxy
Tab A7), animating a few hundred records per frame (a reveal's cascade):

| layer | JS thread per frame, p50 | p95 |
|---|---|---|
| no order | 1 ms | 7 ms |
| `orderBy: "y"` | 7 to 14 ms | 19 ms |
| `cells: { size: 512 }` | 14 to 15 ms | 21 ms |

With an order, a write to one record publishes the whole used prefix
through the lease (`stream.publish(true, count)`: a byte range has no
stable position under a permutation), and the core re-sorts and
re-gathers every record of every instance buffer of the entry
(`gather_for_publish`, `order_permutation` or `order_cells`,
`gather_permuted`, `republish_slots`). The cost follows the population,
not the write. Under `cells` it buys back the GPU time of every
off-screen record during a pan (41.7 to 36.0 ms per frame on the same
board, [2d-sprite-view-culling](../done/2d-sprite-view-culling.md)), so on a layer
that animates often the option trades GPU vertex time for CPU gather
time.

## Done looks like

- A write that leaves every key (and every cell) where it was costs a
  publish of the changed records at their gathered positions: the
  inverse permutation maps a slot to its place, and the publish writes
  those records, not the prefix.
- A write that moves a few keys re-sorts incrementally or at least
  republishes only the buffers' changed spans.
- Measured on the same board and tablet: the reveal's JS frame time
  with `orderBy: "y"` and with `cells` near the unordered layer's.

## Involves

`alloy/src/context/order.rs` (the publish hooks, a dirty range in slot
space mapped through the inverse permutation), `alloy/src/gpu/order.rs`
(an incremental sort, or a check that the permutation held),
`packages/core/src/gpu.ts` record streams (a dirty range on an ordered
publish instead of the prefix), `packages/2d/src/records.ts` and
`layer.ts` (their `ordered` publishes).
