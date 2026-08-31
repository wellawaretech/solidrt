---
title: Sprite draw order is insertion order, so reordering means remove and re-add
description: The sprite layer paints in record order with no sort key, so raising one sprite or depth-sorting a population by y - the ordinary case for a dense 2D scene - costs a record shift and an index fixup per element instead of a sort of an index array.
created: 2026-08-22
---

# Sprite draw order is insertion order, so reordering means remove and re-add

What it looks like when you hit it: two ordinary things stop being ordinary.

- **Raise on interaction.** Dragging a piece over its neighbours, or focusing
  a token, needs it drawn last. The only way to say that is `removeSprite`
  then `addSprite`, which loses the handle, shifts every later record down a
  slot and renumbers every later sprite.
- **Depth sorting.** A scene drawn in perspective paints back to front, which
  in practice means sorted by y every frame as things move. Expressed through
  add/remove that is O(n) record churn per changed element per frame, against
  a layer whose entire reason for existing is that per-element costs are what
  kill you.

The result is that a sprite population that changes its order is the one
population `@solidrt/2d` handles worse than plain elements, where the tree
order is just the JSX order and moving a node is a node move.

## Cause

[packages/2d/src/layer.ts](../../packages/2d/src/layer.ts): record order IS
draw order IS insertion order. `_order[i]` and record slot `i` are the same
index by construction, which is what makes `removeSprite` a `copyWithin` plus
an index fixup over every later sprite, and what makes reordering
expressible only as removal and re-insertion.

The coupling is deliberate and it is what keeps the flush a single bulk
`.set` of one contiguous prefix. The fix has to preserve that.

## Proposed shape

Break the identity between record slot and draw slot: keep records where they
are, and give the flush an order to walk.

- A per-sprite `sortKey` number (default 0), set through `setSprite` like any
  other field. Same key falls back to insertion sequence, so the current
  behaviour is what you get when nobody sets one - and painter's order stays
  the documented model.
- The flush publishes in key order rather than record order. The ordering
  itself should come from [gpu-instance-order](gpu-instance-order.md)
  (field-key mode, gather-at-publish shape): core applies the permutation
  during the lease copy the flush already pays for, so a moving y-sorted
  crowd costs no per-element JS at all. Gaussian splats are the primitive's
  other consumer, which is why it is its own item. The JS fallback (sort an
  index array, gather into the lease buffer during the copy) remains the
  shape if this item is wanted before the core primitive exists - it is
  correct, just O(population) interpreted work per changed frame.
- With records no longer required to be contiguous in draw order, removal can
  become a free-list release rather than a shift - a second win falling out
  of the same change.

Open before implementing: whether the key is a plain float (fine, sorts
naturally, lets an app write `y` straight in) or a layer-owned integer;
whether the sort runs every flush or only when a key changed; and how the
raw-records power path (documented on `SpriteLayer.records`) survives, since
"record i is draw slot i" is currently part of its contract - it likely
becomes "record i is a stable slot, order is separate", which needs the
AGENTS.md note updated with it.

A trap this makes, seen in the wild: a selection marquee drawn as a sprite
had to be added in `onSettled`, AFTER a `<For>` of sprites mounted, so it
took the higher slots and painted on top. A load-bearing ordering
dependency that the API cannot express and the code does not show; moving
the `addSprite` earlier breaks the visuals with no error. The sort key is
what makes that intent statable.

## Findings

The y-sort half landed 2026-08-31 (uncommitted), on BOTH layers, riding
gpu-instance-order (whose stage 3 - multi-buffer entries - was built for
this; findings there):

- Record layer: `orderBy: "y" | { field, descending? }` on
  `createRecordLayer` (landed with gpu-instance-order stage 1). Record
  slots stay stable; removal still shifts (the free-list follow-on was
  not taken - the record layer's contiguous-prefix flush is its identity).
- Node layer: `orderBy: "y"` on `createSpriteLayer` and `<SpriteLayer>`,
  keying on WORLD y in the core-written pose buffer (`instanceOrder
  { field: 1 }` on slot 0). Because the key is core-owned, sprites moved
  by native transitions or any core producer re-sort with ZERO JS per
  frame; the core gathers pose and style under one permutation and
  republishes style itself when a pose move re-orders. Slots, handles,
  picking and growth are untouched. The raw `{ field }` form is record
  layer only - pose record internals are not public vocabulary.
- Both layers document the same pick limitation: overlap resolves by
  slot/record order, not visual order, when a key is set.
- Verified: probes/order-probe.tsx end to end (ORDER-OK + NODE-ORDER-OK:
  scrambled insert draws in y order on both layer kinds, and a pose-only
  `setSprite` move re-orders the node layer with no style write
  anywhere); stats clean (fps 60, missedPresents 1 incl. a reload).
  Requires a rebuilt client (`make client`) - a stale binary REJECTS the
  node layer's multi-buffer instanceOrder at create.

The raise half landed 2026-08-31 (uncommitted), closing the item:

- Core: `InstanceOrder` gained the `slot` key designation (which instance
  slot's records hold the key, default 0, parse-validated) - the additive
  extension stage 3 designed for. The registry keys on it everywhere the
  old slot-0 constant sat; nothing else changed, since a key in a
  lease-written slot with sink-written siblings already worked. Verified
  in draw_ordered.rs: a lease-written key orders the sink-written pose
  sibling, and a key change alone republishes the pose from its mirror -
  plus the declared-but-unconsumed-attribute fact the package relies on
  (record_layout skips attributes the shader does not use; their bytes
  pad the stride), exercised on real GL.
- Node layer: style record grew a 9th float
  (`[uv, tint rgba, sortKey]`), `iSortKey` declared in the pipeline but
  never read by the shader; `orderBy: "sortKey"` maps to
  `instanceOrder { field: 8, slot: 1 }`. `sortKey` on
  SpriteOptions/`<Sprite>` (default 0 - ties keep slot order, and adds
  zero it explicitly so recycled slots cannot leak the previous
  occupant's key), returned by getSprite. On a record-layer sprite it
  throws (13-float record has no key field; `orderBy: { field }` is that
  layer's vocabulary). The raise idiom is in the docs:
  `setSprite(hit, { sortKey: ++top })`, back to 0 to restore.
- Trap fixed in passing: `_write`'s style guard must list EVERY
  style-side option - a sortKey-only setSprite silently published
  nothing until `opts.sortKey !== undefined` joined the guard. The probe
  caught it (KEY-ORDER-FAIL on the raise, initial order fine).
- Verified end to end: probes/order-probe.tsx (ORDER-OK, NODE-ORDER-OK,
  KEY-ORDER-OK - scrambled keys beat insertion order, then a raise flips
  the topmost through the style-slot key with the core republishing the
  pose buffer); stats clean (fps 60, missedPresents 2 incl. reloads);
  alloy 360 tests + 27 draw_ordered assertions; srt check green. The
  flux:gpu surface has its own headless pin, flux/examples/gpu_order.rs
  (SDL_VIDEO_DRIVER=offscreen cargo run -p flux --example gpu_order
  --features gui, GPU-ORDER-OK): field/projected/slot keys through the JS
  create forms, orderDirection via setDraw, the sibling-slot gather and
  mirror republish asserted by pixel pairing, and six guards throwing at
  the JS call site.

Not taken, deliberately: a record-layer sortKey field (the 13-float
layout is a documented raw contract; `orderBy: { field }` covers it),
free-list removal on the record layer (its contiguous-prefix flush is
its identity), and visual-order picking under a key (both layers
document slot-order overlap resolution; revisit on a report).
