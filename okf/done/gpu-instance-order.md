---
title: Instance draw order within one entry, produced in core
description: Draw order of one entry's instance records must change without record churn - sprite raise/y-sort reorders by remove-and-re-add today, gaussian splats need back-to-front over 100k+ records per camera move, and particles would need it per frame. One core primitive orders an entry's records by a key (field or view-projected depth); slots stay stable, JS never touches the order. Settled 2026-08-31 - one API, key mode and materialization orthogonal, gather-at-publish default, retained copy opt-in.
created: 2026-08-24
---

# Instance draw order within one entry

## Symptom

Instanced draws consume records in buffer order, and nothing can change
that order except rewriting records. Two consumers hit this today, at
opposite scales, and a third is foreseeable:

- **The sprite layer** ([2d-sprite-sort-key](2d-sprite-sort-key.md)):
  raising a dragged sprite or y-sorting a perspective crowd is expressible
  only as remove-and-re-add, which shifts records and renumbers handles.
  Its note proposes a JS index-gather at flush; workable at sprite
  populations, but still O(population) interpreted work per frame for a
  moving y-sorted crowd - on the layer whose reason to exist is avoiding
  per-element JS.
- **Gaussian splats** ([gaussian-splats](../backlog/gaussian-splats.md)): correct
  rendering IS a strict back-to-front order over the whole cloud
  (100k-1M records) whenever the camera moves meaningfully. At any rung
  above core this is out of reach; browser viewers burn a worker plus a
  full buffer re-upload per sort.
- **Particles** (prospective, no note yet): a particle population
  rewrites every record every frame AND wants back-to-front order every
  frame for alpha blending - a projected key on a
  republish-every-frame population, crossing the other two consumers'
  shapes. Physics-driven crowds are the same profile at sprite scale
  with a field key.

Same primitive, two key sources. This is the third ordering granularity in
the notes: [spatial-core](../backlog/spatial-core.md)'s escalation 3 orders draw
ENTRIES within a target; this orders RECORDS within one entry; the sprite
note was the JS-side draft of it.

## Shape

An opt-in ordering stage on one draw entry, owned by core, between the
app's records and what the draw consumes. Two key modes, fixed at setup:

- **Field key**: the float at a given offset in each record (sprite y, or
  an explicit sortKey field an app writes). Re-orders when records
  holding keys change.
- **Projected key**: dot(position-at-offset, direction) - view depth from
  a camera the consumer updates on camera move (the `SharedSlot`
  projection precedent: core does arithmetic on caller-named data, no
  domain concept enters).

Ascending or descending, stated at setup. Record slots as the app knows
them never move: JS writes slot i forever, order is core's output.
Counting/radix sort over quantized keys, not a comparison sort - the
populations are exactly the shape radix likes, and it must stay cheap
enough to run every frame (a y-sorted crowd or a particle population
sorts per frame, not per camera-settle).

## How the order reaches the GPU (settled 2026-08-31)

GL instancing has no index indirection - records are consumed in buffer
order - so the order must be materialized. Two strategies, and they are
orthogonal to the key mode, not paired with it:

- **Gather at publish** - the default. The sprite layer already
  re-publishes its live prefix through the buffer-write lease every
  flush; core applying the permutation during that copy costs nothing
  extra and retains no copy. Fits every republish-per-frame population:
  sprites (field key) and particles (projected key) alike.
- **Retained copy, permute on demand** - per-entry opt-in for
  write-once populations. Splat records are written once and only the
  ORDER changes; core retains the record data (or just the key column
  plus the full records - decide by measuring), sorts, and re-uploads
  permuted, throttled by camera-settle. The re-upload is the cost
  (1M x 32 B per sort); the escalation if that bandwidth shows up is a
  per-instance index attribute with record data fetched from a texture
  by index - 4 B per record, needs
  [gpu-float-texture-formats](../backlog/gpu-float-texture-formats.md) and a
  different material contract, and is exactly what the web
  gaussian-splat viewers ship.

One API, not two entry points: key mode (field vs projected) and
backing strategy (gather vs retained) are independent choices at setup.
A particle population is projected-key + gather, so bundling a key mode
with a strategy is already violated by the third consumer.

The retained copy collides with the deliberate "raster thread drops its
upload Vec" stance recorded in spatial-core's findings - the retention
here is opt-in per entry, which is what keeps that rejection intact.

## Engine precedent (surveyed 2026-08-31)

Raw instancing is buffer-order in three.js (InstancedMesh), Unity
(RenderMeshInstanced) and Godot (MultiMesh); all three leave
per-instance order to the user, so ordering at the entry level is a
differentiator, not parity. Where these engines DO sort, it is CPU-side
riding a rebuild that happens every frame anyway (Unity Shuriken's
sortMode, Godot's CPUParticles draw_order) - gather-at-publish - and
both expose field keys (age, index) and projected keys (view depth) as
one enum on one surface, never as separate entry points. Their top
scale goes GPU compute (Unity VFX Graph, Godot's RD renderers), off our
GLES 3.0 floor; the proven CPU answer at splat scale is the
index-texture form above. three.js BatchedMesh materializes order as
WEBGL_multi_draw ranges instead of moving data - the only zero-copy
shape, but multi-draw is not core GLES 3.0 and each ordered item
becomes its own sub-draw, forfeiting instancing. Considered and
rejected.

## Findings

Stage 1 (gather-at-publish, single instance buffer) landed 2026-08-31
(uncommitted): `instanceOrder` on the entry creates, field and projected
keys, radix gather in the lease publish, `orderDirection` updates, the
buffer-swap rekey, and the record layer's `orderBy` riding it.

Stage 3 (multi-slot entries - the node layer's shape) landed 2026-08-31
(uncommitted): an entry with several instance buffers orders them all
under ONE permutation.

- The key reads from SLOT 0's records (a `slot` designation on the
  declared order is the additive extension when a consumer needs a key
  elsewhere; none does yet).
- Multi-slot entries retain the permutation plus a slot-order mirror of
  each slot's last published records (`context/order.rs`); single-slot
  entries keep stage 1's zero-retention path bit-for-bit. When the key
  slot's publish changes the permutation, every sibling slot republishes
  from its mirror in the same flush - both buffers always describe the
  same draw order, with no publish from the app anywhere.
- The spatial sink path publishes ordered buffers whole:
  `SinkWriter::write_instances` now carries the dirty range plus the WHOLE
  staging mirror, and Context's writer routes ordered buffers to
  `ordered_instance_publish` (full-extent gather through a pooled lease
  block) instead of the partial `write_gpu_buffer` - which keeps rejecting
  ordered buffers for everyone else, since a byte-offset write has no
  stable position under a permutation. So a core producer moving nodes
  re-orders a whole multi-buffer entry with zero JS.
- The pure half split into `order_permutation` + `gather_permuted`
  (`gpu/order.rs`); the permuted gather reconciles record-count mismatches
  between key and sibling buffers (out-of-range perm entries skip, the
  unpermuted tail appends in slot order).
- Swaps rekey every changed slot in one `setDraw`/`setDrawRange`
  transaction; an ordered entry's instance buffers must be pairwise
  distinct. After a swap the app republishes its own slots (the new buffer
  starts empty); a retargeted pose group republishes itself at the next
  spatial flush. Growth flow: create both buffers, `retargetRecords`,
  swap both slots, destroy the old pair - no flush in between.
- Verified: `cargo test -p alloy` (order permutation/mismatch units, 360
  green) and `SDL_VIDEO_DRIVER=offscreen cargo run -p alloy --example
  draw_ordered` - 23 assertions including the coherence one: a single
  node move republishes pose AND style in key order with no style publish.

The honest cost, by design: an ordered multi-slot entry gives up
dirty-range instance writes - any pose motion republishes the full extent
gathered, and a permutation change republishes the siblings too. That is
the record layer's existing cost model, and unordered entries are
untouched.

The 2d consumption landed with it (same day): `orderBy: "y"` on the node
layer keys on pose world y - see done/2d-sprite-sort-key.md's findings;
probes/order-probe.tsx is the end-to-end check for both layer kinds.

The `slot` key designation landed 2026-08-31 too (uncommitted):
`InstanceOrder.parse` takes `slot` (default 0, bounds-checked), the
registry keys on it everywhere, and draw_ordered.rs covers the
lease-written-key / sink-written-sibling direction plus the
declared-but-unconsumed key attribute. Its consumer is the node layer's
`orderBy: "sortKey"` (style-slot key, the raise case) - which closed
2d-sprite-sort-key.

Stage 2 (retained-copy strategy) landed 2026-08-31 (uncommitted):
`retain: true` on `instanceOrder`, position keys only (a field key
re-orders when its records republish, so retaining buys nothing and
throws). A retained entry keeps the stage-3 mirrors and permutation even
single-slot; an `orderDirection` update re-sorts the retained copy and,
when the permutation actually changed, republishes every slot core-side
in the same call - no publish from the app, and an order-preserving
direction uploads nothing (the parked-camera gate is a perm compare,
after an O(n) re-key). The whole stage rode the stage-3 machinery: one
predicate (`OrderedEntry::retains`) widens mirror retention, and the
re-materialize is the sibling republish applied to all slots. The CPU
copy of the records is the deliberate opt-in cost; camera-settle
throttling stays the consumer's (the splat viewer decides when the
camera moved meaningfully). `writeBuffer` stays rejected on retained
buffers. Verified: parse units, draw_ordered.rs (direction-only re-order
with no publish, pixel proof, no-op equivalent direction),
probes/retain-probe.tsx end to end (RETAIN-OK).

Remaining: nothing - all three stages are in. The consumers now own
their ends: gaussian-splats stage 2 swaps its JS sort for the retained
projected key; the index-attribute + data-texture escalation stays
parked on gpu-float-texture-formats until re-upload bandwidth shows up.

## Done looks like

The sprite layer's raise and y-sort work with stable handles and no JS
gather; a splat cloud re-orders on camera move with zero JS and a parked
camera costs nothing; a future particle population gets per-frame
back-to-front on the gather path with no new core work. All consumers
named in this note run on it with no core changes after the first one.

## Not in this item

Ordering draw entries within a target (spatial-core escalation 3, held
for demand), GPU-side sorting (no compute on the GLES 3.0 floor),
multi-key or hierarchical sorts.
