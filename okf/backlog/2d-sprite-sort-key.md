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
- The flush publishes in key order rather than record order. Either sort an
  index array and gather into the lease buffer (one indirection per sprite
  during the copy, no extra allocation once the index array is reused), or
  keep records physically sorted and pay the moves at mutation time. The
  gather is the better default: it makes the sort cost proportional to
  changed keys, not to the population.
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
