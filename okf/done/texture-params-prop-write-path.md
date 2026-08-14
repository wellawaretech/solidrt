---
title: texture params prop - one write path through the GPU channel
description: Done 2026-08-10 - the <texture params> prop now writes through set_target_params at the properties layer (no tree damage, reuse path preserved, prop and imperative writes validate identically); the pending_params/set_params machinery in the texture kind is deleted, and a params write with no src throws ("set src before params").
created: 2026-08-10
completed: 2026-08-10
---

# texture params prop - one write path through the GPU channel

DONE 2026-08-10, same session as the analysis below. What landed:

- `apply_jsx` takes a `gpu_params` route (production: `Context::
  set_target_params` via the set_property closure in flux gui tree.rs; a
  stub in tests); the texture properties dispatch decodes and sends
  `params` through it, returning `Damage::None` - target state, not
  element state.
- `pending_params`, `set_params`, and the build-time application are
  deleted from the texture kind (alloy rendertree kinds/texture.rs).
- Ordering settled as dev-throw: a params write with no src errors
  "params needs a target to write to: set src before params" (chosen over
  the set_shader warn-precedent because a dropped params value is LOST -
  nothing re-applies it when src arrives - while shader/repaintBoundary
  reconcile at composite).
- Value errors (bad types) surface before the routing, so their messages
  are unchanged; unknown-uniform/non-target errors now throw at the prop
  write exactly like the imperative call.
- Docs updated: docs/core.md texture section, TextureProps comment in
  packages/core types.d.ts. Tests: flux tests/properties.rs (route,
  ordering error, gpu-error propagation).

Consequences: the double re-bake under snapshots is gone, and
prop-driven shader animation takes the present-only reuse path like
imperative writes.

## The analysis that led here (pre-change state, kept for the record)

`<texture params>` stores `pending_params` on the texture node and
returns `Damage::Paint`; the params are applied at the node's next BUILD
via `set_target_params` (rendertree kinds/texture.rs - the documented
"paced to real frames" design). That Damage::Paint is load-bearing: it
is what causes the build that applies the params. Do NOT remove it while
the pending mechanism exists (a first "leftover" note got this wrong).

With GPU content damage
([snapshot-gpu-content-invalidation](snapshot-gpu-content-invalidation.md))
this path now has two costs:

- Under a snapshot boundary, one prop write re-bakes TWICE: frame N
  (damage -> rebuild -> build applies params -> correct bake), then frame
  N+1 again (the build-time set_target_params noted content, which drains
  next frame). Correct, just redundant.
- Prop-driven shader animation rebuilds the display list every frame,
  while the same animation through imperative `setTargetParams` now takes
  the present-only reuse path.

## Proposal

Route the prop through the GPU channel at the properties layer: the flux
texture property setter calls `atx.set_target_params(texture_id, ...)`
directly, the kind's `pending_params`/`set_params` machinery is deleted,
and the prop write produces no tree damage at all. Content damage covers
snapshot consumers; the raster dirty flush already coalesces any number
of writes into one render per frame, so pacing is unchanged in practice.
Result: one write path (the prop is sugar over the imperative call), the
double re-bake gone, and prop-driven shader animation gains the reuse
path.

To settle before doing it:

- **Ordering:** a `params` write landing before `src` has no target id
  to write to; the pending model silently tolerated any order. Options:
  dev-throw with a clear "set src before params" message (validation
  policy), or apply-on-src-set for the pre-src window only.
- **Error surface:** unknown names/arities then throw at the prop write
  (matching the imperative call) instead of the build-time path.
- Update flux-types/docs comment ("applied at the next repaint") and the
  pending-model trap notes when done.
