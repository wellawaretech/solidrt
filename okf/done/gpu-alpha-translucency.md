---
title: Alpha translucency (sorted blending)
description: DONE 2026-08-17 both halves - engine blend "alpha" (premultiplied over, order-dependent) and the @solidrt/3d library half (transparent materials, back-to-front per-mesh sort owned by the scene, renderOrder). Split from gpu-pipeline-extensions 2026-08-11.
created: 2026-08-11
---

# Alpha translucency

Done 2026-08-17, both halves in one day:

- Engine: `blend: "alpha"` (`ONE, ONE_MINUS_SRC_ALPHA`, premultiplied
  output, order-dependent, the list owner sorts) - see
  [gpu-pipeline-blend-modes](gpu-pipeline-blend-modes.md).
- Library (`@solidrt/3d`): `transparent: true` on `unlit` and
  `shaderMaterial` (explicit, Three's rule; alpha alone stays opaque) builds
  the pipeline with `blend: "alpha"` + `depthWrite: false` - NOT Three's
  depthWrite-on default, the known artifact source. `renderOrder` on
  `Mesh` (`setRenderOrder`, and a `Mesh` component prop). The scene owns
  the order: one `setDrawOrder` from `sync()` when dirty - background,
  opaque by renderOrder then add order, transparent by renderOrder then
  back-to-front by the world-bounds center in view space (not Three's
  origin: off-origin geometry sorts by where it is; not the nearest bounds
  point: a big translucent ground plane would cover the small translucents
  on it). Dirty on attach/detach/rebuild
  (which closes the re-append-at-end trap), renderOrder change, and, only
  with two or more transparent meshes, camera or transparent-mesh moves;
  opaque-only scenes never resort on camera moves, and a resort that lands
  on the already-issued permutation sends nothing. `shaderMaterial` infers
  `transparent` from any non-"none" `blend` (explicit false wins). Verified via three probe
  scenes read back through the control API: correct add order untouched,
  wrong add order reversed, `setGeometry` rebuild keeps place, moving a
  transparent mesh behind another resorts.

Deliberate non-goals: per-mesh sort only (no per-triangle, no OIT); no
opaque front-to-back sort for early-Z (a separate perf decision, no visual
difference); no separate `opacity` property (alpha lives in `color`).

The record below is the history as it stood before.

Symptom (as recorded 2026-08-11): transparent geometry has no general path. Blending within one draw
is additive-only (`blend: "add"`, landed 2026-07-29); classic alpha
translucency (`SRC_ALPHA, ONE_MINUS_SRC_ALPHA` or its premultiplied form)
is not offered. The known workaround is convex-only: front and back faces
split into two targets composited with `<texture blendMode="plus">`, which
works only because a convex object has exactly one front and one back face
per pixel. Non-convex transparent meshes and many-particle accumulation
with per-particle colour still have no path (demand recorded 2026-07-29 in
[gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md)).

Adding the blend mode is trivial; what defers it is correctness, per
[gpu-pipeline-blend-modes](gpu-pipeline-blend-modes.md) (the fuller design
note for the blend vocabulary): it needs sorted geometry - which the draw
list's ordering verbs (`before`, `setDrawOrder`) now make expressible, but
no sorting story owns - and an answer to straight-vs-premultiplied against
how Impeller composites the target. Do not add the mode without deciding
those two.

First step regardless, and it costs nothing: document the target pixel
contract (premultiplied, non-linear RGBA8 -
[gpu-pixel-contract-docs](../done/gpu-pixel-contract-docs.md)), which answers the
straight-vs-premultiplied half by declaring it.

## Who owns the sort (recorded 2026-08-17)

Second demand for the non-convex case, and with it the missing half of the
answer: the scene graph is the sorter, and nothing yet says so.

`setDrawOrder` exists at the GPU layer and is documented as the sorting verb.
`@solidrt/3d` does not surface it at all, and `setGeometry`/`setMaterial`
re-append the entry at the list end - harmless while everything is opaque,
and exactly the thing that breaks the moment this item lands. Entry order is
add order, which is why an inverted-sphere sky dome has to be added first;
that works but it is implicit.

Two decisions to take together, and they are cheap to take now:

- **Sorting policy.** A `transparent: true` flag on a material, with the
  scene graph keeping the transparent set back-to-front over stable DrawIds
  and recomputing only when the camera moves (roadmap item 6's shape), or the
  app driving `setDrawOrder` itself. Deciding early is the point: the flag
  changes what a material is, and retrofitting it after transparency ships is
  a breaking change.
- **Explicit ordering.** A `renderOrder` on `Mesh`, Three's name for the same
  quantity, so add-order dependence becomes intentional rather than
  discovered.

History: the remaining half of the blending bullet in
[gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md) (additive half done
2026-07-29); split out 2026-08-11.
