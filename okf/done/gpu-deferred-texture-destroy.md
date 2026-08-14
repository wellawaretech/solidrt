---
title: Frame-safe texture destruction
description: destroyTexture used to land before the reactive texture swap flushed; the runtime now defers reclamation until the live render tree no longer references the id.
created: 2026-07-23
completed: 2026-07-30
---

# Frame-safe texture destruction

Shipped 2026-07-23. `destroyTexture(id)` used to take effect immediately,
while the reactive update repointing `<texture src>` at a replacement id rode
the signal flush - so every rebuild-and-swap app had a latent window where the
render tree referenced a destroyed id (at best a one-frame blank + warning,
depending on flush/paint interleaving).

Implemented as the preferred option from the original write-up: the runtime
defers reclamation until the live render tree no longer references the id.

- `Context::destroy_texture` now only queues the id (`pending_destroys`);
  everything stays fully usable until reclamation.
- `Context::reclaim_destroyed(referenced)` performs the actual registry
  removal + raster `DestroyTexture` for every pending id not in `referenced`,
  and keeps still-referenced ids queued - a destroyed-but-still-mounted
  texture keeps drawing instead of glitching to blank.
- `RenderTree::referenced_texture_ids()` collects the ids held by live
  Texture elements (attached and detached; the only element kind holding
  registry ids). Only computed when destroys are pending, so the per-frame
  hot path is untouched.
- Sweep sites: end of `composite::paint_phase` (after `deliver_captures`, so
  capture callbacks that free their texture reclaim in the same frame), and
  lattice's present-only reuse path in draw.rs (a destroy with no other tree
  change lands there). The flux `destroyTexture` binding now requests a frame
  so a destroy on an idle app is not stranded; pixel safety never depends on
  the sweep running (in-flight display lists hold Rc'd Impeller handles).

Semantics note: ids are never reused (`allocate_id` is monotonic), so
deferral cannot collide with a later create. A destroyed id that stays
mounted forever stays allocated - visible behavior (it keeps drawing), not a
silent leak.

Touched: alloy context.rs (pending queue, reclaim_destroyed), rendertree
tree.rs (referenced_texture_ids + unit test), composite.rs (sweep), lattice
draw.rs (reuse-path sweep), flux gui/gpu.rs (request_frame on destroy),
flux-types gpu.d.ts (destroyTexture doc), docs/core.md.

This is what makes [[gpu-reactive-resource-helpers]]' createShaderMemo
disposal trivially safe, and it retires the one-flush caveat recorded against
[[gpu-in-place-resize]]'s original motivation.

Remaining / follow-ups:

- Like resize, no GL-level automated test (the alloy suite has no GL context
  harness); the tree-reference sweep itself is unit-tested.
