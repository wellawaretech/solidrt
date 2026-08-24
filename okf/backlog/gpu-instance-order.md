---
title: Instance draw order within one entry, produced in core
description: Two populations need the draw order of one entry's instance records to change without record churn - sprite raise/y-sort reorders by remove-and-re-add today, and gaussian splats need a back-to-front order over hundreds of thousands of records per camera move, which is O(N) work no rung above core can pay. One core primitive orders an entry's records by a key (a record field, or view-projected depth) so record slots stay stable and JS never touches the order.
created: 2026-08-24
---

# Instance draw order within one entry

## Symptom

Instanced draws consume records in buffer order, and nothing can change
that order except rewriting records. Two consumers hit this, at opposite
scales:

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
populations are exactly the shape radix likes.

## The fork to settle first: how the order reaches the GPU

GL instancing has no index indirection - records are consumed in buffer
order - so the order must be materialized. Two shapes, and the consumers
pull toward different ones:

- **Gather at publish.** The sprite layer already re-publishes its live
  prefix through the buffer-write lease every flush; core applying the
  permutation during that copy costs nothing extra and retains no copy.
  Fits lease-published, frequently-rewritten populations.
- **Retained copy, permute on demand.** Splat records are written once
  and only the ORDER changes; core retains the record data (or just the
  key column plus the full records - decide by measuring), sorts, and
  re-uploads permuted. The re-upload is the cost (1M x 32 B per sort),
  throttled by camera-settle; the alternative - a per-instance index
  attribute with record data fetched from a texture by index - shrinks
  the upload to 4 B per record but needs
  [gpu-float-texture-formats](gpu-float-texture-formats.md) and a
  different material contract. Start with permute-and-upload; the index
  form is the escalation if sort-upload bandwidth shows up.

Whether these are one API with two backing strategies or two entry points
is the main design decision. The retained copy also collides with the
deliberate "raster thread drops its upload Vec" stance recorded in
spatial-core's findings - the retention here is opt-in per entry, which is
what keeps that rejection intact.

## Done looks like

The sprite layer's raise and y-sort work with stable handles and no JS
gather; a splat cloud re-orders on camera move with zero JS and a parked
camera costs nothing. Both consumers named in this note run on it with no
core changes for the second one.

## Not in this item

Ordering draw entries within a target (spatial-core escalation 3, held
for demand), GPU-side sorting (no compute on the GLES 3.0 floor),
multi-key or hierarchical sorts.
