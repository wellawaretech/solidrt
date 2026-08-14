---
title: Shader effects on a subtree
description: A snapshot boundary already rasterizes a subtree into a texture; a shader prop runs one pass over that texture and composites the result. Plan decided 2026-08-03; all three stages done and verified 2026-08-04. Android sanity run for exact-size storage pending.
created: 2026-07-27
completed: 2026-08-04
---

# Shader effects on a subtree

Split out of okf/plans/root-layer-effects.md (2026-07-27), where it had been
appended as a stage. It is a separate feature: that plan is about the window's
contents as a whole, this is about a region, and the two differ in semantics
and in what they can see. Its prerequisites from that plan have landed: the
program/target split (compileShader/linkProgram) and the declaration shape
(the window `shader` prop), plus the snapshot rig churn fix.

## Mechanism

`repaintBoundary="snapshot"` already rasterizes a subtree into a texture and
composites it as one quad (alloy/src/rendertree/composite.rs, `snapshot_node`).
A `shader` prop on such a view means: do that, run the program over the
resulting texture into a second texture, and composite that instead. Ordering
is correct by construction, since it happens at the boundary's position in
the paint walk, before the parent composites the quad.

Cost is region-sized rather than window-sized, so it is far cheaper than a
window effect, and it can later render at reduced resolution where the
effect tolerates softening.

## Plan (decided 2026-08-03)

Same descriptor and uniform contract as the window shader: `uSource` is the
boundary's texture, `iResolution` its physical size.

1. **Core pass** [done, verified 2026-08-03 on desktop Linux: warp
   upright, one ~0.13ms pass per frame, params-only frames leave jsMs
   flat, identity toggle clean].
   `shader={{ program, params?, textures? }}` on views, valid only with
   `repaintBoundary="snapshot"` (warn otherwise; see decisions). One
   attributeless pass snapshot -> per-node output texture, composite the
   output quad. Split validity: content damage re-rasterizes the snapshot
   then re-runs the pass (`RasterizeDlShaded`, one trip); a params-only
   change re-runs only the pass (`RerunNodeShader`, fire-and-forget,
   driven by a dirty flag on the View). All GL lives on the raster
   thread's one ordered channel, so pass-before-frame ordering is by
   construction; no fences needed. The program contract matches shader
   targets, not the window pass: vUV needs no flip.
   Example: packages/core/examples/view-shader.tsx; docs: "Boundary
   shader" in docs/core.md.

   Reworked 2026-08-04 (no-backward-compat cleanups, user-approved): ALL
   snapshot storage is exact-size now - the 64px tile round-up was
   deleted outright (its corruption theory was a red herring for the
   cross-context bug, itself dead since the raster thread owns all GL;
   the exactly-sized window layer had long proven the path on Android).
   One allocation contract means a shader toggle at unchanged dimensions
   reuses the source storage instead of dropping the cache (an outset
   still reallocates - different canvas). PaintCache::Snapshot became a
   struct (SnapshotCache with an optional ShadedCache { output, outset,
   history }), and Damage::Transform was renamed Damage::Compose (it
   names the class: composite-time state - matrix, opacity,
   recording-scroll, shader declarations). Pending: an Android sanity
   run for the exact-size storage.
2. **`outset`** [done, verified 2026-08-03: pixel probe showed bleed in
   all four margin bands and zero content beyond the outset]. Logical px
   on the descriptor: grows the raster area and the composited quad. The
   margin exists for the effect to write into (glow, shadow, bleeding
   blur); the subtree's own paint stays cropped at the layout box (an
   explicit clip - on the plain path the crop is the texture viewport),
   so the margin starts transparent. Content overflow into the margin is
   a different feature. Demo lesson: bleed is only visible when content
   reaches the box edge (the example gained a filling background rect).
3. **`previous: true`** [done, verified 2026-08-04 on desktop Linux:
   click cross-dissolve correct, passes burst only during the mix sweep
   and stop at rest, warp example unregressed on the exact-storage
   base at the same ~0.1ms/pass]: the prior rasterization bound as
   `uPrevious`, rotated on
   content re-raster by role swap (the new frame renders into the old
   history's storage and the old source becomes uPrevious - no copy);
   history starts transparent and resets to transparent on a canvas
   resize. Example: packages/core/examples/view-shader-history.tsx
   (click-to-cross-dissolve).

## Decisions

- **The boundary is required, not implied.** The prop's real cost is
  snapshot semantics (retained pixels, crop at the layout box, smear
  under ancestor scale animation); requiring `repaintBoundary="snapshot"`
  keeps that visible and keeps one invalidation story. Enforced as a
  composite-time warning rather than a set-time throw: prop application
  order would make a throw misfire on elements that set `shader` before
  `repaintBoundary`.
- **Named `shader`, not `effect`,** matching the window prop and the
  reservation of `filter` for Impeller backdrop filters.
- **Intermediate target is per-node and element-held,** like the snapshot
  texture itself; dies with the node. Pooling is an optimization with no
  measured need.
- **Transform/opacity hoisting stays.** The effect runs in the boundary's
  own coordinate space on the pose-free texture; rotation and opacity
  apply to the result.
- **`previous` is source history, window-consistent.** Previous *output*
  would be self-referential feedback, which the purity decision
  (gpu-purity-decision.md) confines to manual targets. Caveat to
  document: for a static panel with animated params, `uPrevious` equals
  `uSource` until content actually changes; rotation happens exactly when
  content changes, which is when transitions want it.
- **Hit-testing stays on layout geometry.** A distortion shader moves
  pixels, not hit targets; visual-only, documented.

## The limit that defines it

The effect samples the subtree's own pixels, so warping, dissolving or
grading a panel works, and anything that needs what is *behind* the panel
does not: those pixels are not in the texture. That case needs either an
effect declared at a point in the tree (everything painted before it goes
through the effect, everything after draws on top) or Impeller's own filters
(okf/backlog/impeller-backdrop-filters.md). Neither is a variant of this
item. Worth restating in docs, because frosted-glass-over-background is the
first thing people will try.
