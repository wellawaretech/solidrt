---
title: Tile world bounds - bounded contract or additive path to unbounded
description: Decide whether the TileLayer's creation-fixed cols x rows grid is the contract, or sketch the additive route to an unbounded world before something depends on the bound
created: 2026-08-30
---

# Tile world bounds

The tile layer's grid is creation-fixed (`cols`/`rows`, "recreate to
resize"), but chunks allocate lazily, so the bound is bookkeeping, not
architecture. Godot's TileMap is unbounded (cells at any coordinates);
Unity's Tilemap likewise grows on write. If we ever want that, making
`cols`/`rows` optional is additive on its face - which is why this sits in
the backlog instead of forcing work now.

The catch, and the reason to decide deliberately rather than discover it
later: `TileLayer.width`/`height` and the world `<view>` sized to them are
load-bearing. The view's laid-out box is the auto-oversample measurement
basis (pinned by `flexShrink={0}`), the rotation divide-out divides by the
world size, and the camera transform hangs off the world-sized view. An
unbounded world has no width/height, so those all need a different anchor -
likely a camera-anchored composite (chunks positioned relative to the
camera, oversample measured from the clipping container instead of the
world view). That is a new composite mode, not a prop default.

Options:

1. Bounded is the contract: document that a tile world is finite and sized
   at creation, full stop. Zero work; an app with a huge sparse world still
   pays nothing for empty chunks, it just has to pick a big number.
2. Additive unbounded mode: keep the bounded form as-is and add an
   unbounded creation form later whose composite anchors on the camera.
   Intersects with camera-driven chunk residency (2d-baked-layers.md, the
   bake-on-approach/evict item) - an unbounded world without eviction is a
   memory leak with a scenic route, so the two land together.

Either way the current API survives; the decision only fixes which
sentences the docs get to promise.
