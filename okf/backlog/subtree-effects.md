---
type: backlog-item
title: Shader effects on a subtree
description: A snapshot boundary already rasterizes a subtree into a texture, so running a shader over it and compositing the result is one extra pass; region-sized rather than window-sized, but it can only see the subtree's own pixels.
status: open
timestamp: 2026-07-27T00:00:00Z
---

# Shader effects on a subtree

Split out of okf/plans/root-layer-effects.md (2026-07-27), where it had been
appended as a stage. It is a separate feature: that plan is about the window's
contents as a whole, this is about a region, and the two differ in semantics
and in what they can see. It does depend on that plan landing first, since it
reuses the program/target split and the effect declaration shape.

## Mechanism

`repaintBoundary="snapshot"` already rasterizes a subtree into a texture and
composites it as one quad (alloy/src/rendertree/composite.rs, `snapshot_node`).
An `effect` prop on a view means: do that, run the effect program over the
resulting texture into a second texture, and composite that instead. Ordering
is correct by construction, since it happens at the boundary's position in
the paint walk, before the parent composites the quad.

Cost is region-sized rather than window-sized, so it is far cheaper than a
window effect, and it can render at reduced resolution where the effect
tolerates softening.

## The limit that defines it

The effect samples the subtree's own pixels, so warping, dissolving or
grading a panel works, and anything that needs what is *behind* the panel
does not: those pixels are not in the texture. That case needs either an
effect declared at a point in the tree (everything painted before it goes
through the effect, everything after draws on top) or Impeller's own filters
(okf/backlog/impeller-backdrop-filters.md). Neither is a variant of this
item.

## To work out

- Whether the prop implies a snapshot boundary or requires one to be
  declared already. Implying it hides a real cost; requiring it makes the
  common case verbose.
- Whether the intermediate target is runtime-owned per node (simple,
  allocates per effect-bearing node) or pooled by size.
- Interaction with the boundary's existing transform and opacity hoisting:
  the effect runs on the pose-free texture, which is the property that makes
  the boundary a good effect source in the first place, but it means the
  effect sees unrotated content and the rotation applies afterwards.
