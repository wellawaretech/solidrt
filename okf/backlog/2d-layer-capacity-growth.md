---
title: A sprite layer's capacity is fixed and overflow throws
description: createSpriteLayer reserves a record buffer for the layer's life, so an app whose sprite count is data-driven has to guess a maximum up front and crashes when it guesses low; growth needs a way to swap a target's instance buffer.
created: 2026-08-22
---

# A sprite layer's capacity is fixed and overflow throws

What it looks like when you hit it: everything works through development,
then a bigger level, a longer session or a busier document adds one sprite
too many and `addSprite` throws

```
addSprite: layer is at capacity (1024 sprites)
```

with no way to recover in app code. Every other constraint in
`@solidrt/2d` is a performance tradeoff you can measure and live with. This
one is a hard failure at runtime, and it pushes a guess onto the app author
at the one moment they know least - layer creation.

The workaround is to reserve generously (records are 52 bytes, so 100k
sprites is 5MB) and hope. That works for a game with a known entity budget
and does not work for anything data-driven, which is most of what a 2D-heavy
application does.

## Cause

[packages/2d/src/layer.ts](../../packages/2d/src/layer.ts) allocates both
sides of the record store once, at creation:

- the JS canonical array, `new Float32Array(capacity * FLOATS_PER_SPRITE)`
- the GPU instance buffer, `createBuffer(capacity * FLOATS_PER_SPRITE * 4)`,
  handed to `createPipelineTexture` as `instanceBuffer`

The JS side could grow trivially. The GPU side cannot: core buffers are
fixed-size by contract, and the pipeline target holds the buffer id it was
created with. There is no `setInstanceBuffer`-shaped call to point a live
target at a replacement.

## Proposed shape

Grow on demand inside `addSprite`, doubling like any dynamic array:

1. allocate a new, larger buffer
2. copy the live prefix into it (a write lease publish covers this - the
   canonical JS array is the source of truth, so the copy is just the normal
   flush against the new buffer)
3. re-point the target at the new instance buffer
4. destroy the old one

Step 3 is the missing piece and the only part that touches core. Options to
weigh: a `setDraw`-adjacent call that swaps the instance buffer on an
existing target, versus rebuilding the pipeline target (cheap in theory,
but the target's texture id is what the app composited into the tree, so it
must survive - which argues for the swap).

Open before implementing: whether the growth policy should be doubling or
caller-hinted, and whether `capacity` stays as a reservation hint once
growth exists (it should - reserving up front still avoids the copies).
