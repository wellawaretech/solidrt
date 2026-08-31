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
- **Gaussian splats** ([gaussian-splats](gaussian-splats.md)): correct
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
the notes: [spatial-core](spatial-core.md)'s escalation 3 orders draw
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
  [gpu-float-texture-formats](gpu-float-texture-formats.md) and a
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
