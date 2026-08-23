---
title: A sprite layer's capacity is fixed and overflow throws
description: createSpriteLayer reserved a record buffer for the layer's life, so a data-driven sprite count had to guess a maximum up front and crashed when it guessed low; resolved by a draw-entry buffer swap in core (setDraw buffer keys / setDrawBuffers) and doubling growth in the layer.
created: 2026-08-22
completed: 2026-08-23
---

# A sprite layer's capacity is fixed and overflow throws

What it looked like: everything works through development, then a bigger
level, a longer session or a busier document adds one sprite too many and
`addSprite` throws

```
addSprite: layer is at capacity (1024 sprites)
```

with no way to recover in app code. Every other constraint in
`@solidrt/2d` is a performance tradeoff you can measure and live with. This
one was a hard failure at runtime, pushed onto the app author at the one
moment they know least - layer creation.

## Cause

[packages/2d/src/layer.ts](../../packages/2d/src/layer.ts) allocated both
sides of the record store once: the JS canonical array and the GPU instance
buffer handed to `createPipelineTexture`. The JS side could grow trivially;
the GPU side could not, because core buffers are fixed-size by contract and
a target's entry held the buffer id it was created with. Nothing could point
a live entry at a replacement.

## What shipped

A draw entry's buffers are now mutable state, the way its params, textures
and range already were:

- `setDraw(id, { buffer?, indexBuffer? + indexFormat?, instanceBuffer? })`
  on single-draw targets, alongside the range keys. Both halves are one
  transaction in alloy (`Context::update_draw`): the merged range is
  validated against the swapped buffers before anything commits, so one
  call grows a buffer and extends the range into it, and a rejected call
  changes nothing.
- `setDrawBuffers(target, draw, update)` on draw-list entries, sibling of
  `setDrawRange`.

Replace-only: a role the entry fills gets a new buffer; roles never change
(they are pipeline layout state, and indexing is the entry's draw
vocabulary). The entry's draw range is kept and rechecked against the new
sizes, so a swap to a buffer too small for the live range throws at the call
site; a larger one never does. Raster-side the VAO is rebuilt against the
new buffers and the replaced ones released, so `destroyBuffer(old)` right
after the swap is safe in either order.

`@solidrt/2d` grows on demand: `addSprite` past the reservation doubles the
canonical array, and the next publish creates a larger GPU buffer, writes
the live prefix, swaps it in and destroys the old one. `capacity` stays as
the initial reservation (a hint that avoids the copies, not a limit).

Verified with a probe reserving 4 records and adding 1500 sprites while
rendering: the GPU inventory ends with one records buffer of 2048 x 52 bytes
as the pipeline's instance buffer, the intermediate buffers freed, and the
snapshot shows the full grid.

## Why a core verb and not a draw-list workaround

The alternative was to move the layer onto `createDrawTarget` and express
growth as add-entry-then-remove-entry. That works without touching core,
but it encodes "change this entry's buffer" as list churn and forces the
layer off the single-draw primitive for a reason unrelated to draw lists.
Textures were already swappable on a live entry while buffers were not; the
verb removes that asymmetry, and `@solidrt/3d`'s instanced meshes (capacity
fixed at creation today) get the same growth path for free.

Deliberately not done: shrinking (nothing asks for it; a swap to a smaller
buffer works once the range fits) and growth-policy hints beyond the
reservation.
