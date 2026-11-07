---
title: A second camera view of a splat cloud draws in the scene camera's order
description: An indexed instance order (splats) has one id stream, sorted under the scene's camera, and every other camera view (scene.createView) reads that stream as it is, so its back-to-front order is the scene camera's and blends wrong when the cameras differ; the fix is an id stream per camera view, sorted under that view, as the draw lists already do for app-written populations (okf/design/draw-order-and-culling.md, D3, D4).
created: 2026-11-07
---

# A second camera view of a splat cloud draws in the scene camera's order

## Symptom

A record mesh with an indexed `instanceOrder` (a splat cloud) is sorted
back to front for the scene's own camera. The sort writes one id
stream, and the mesh's entry in every other camera view (a
`scene.createView` inset, a second split-screen pane) binds that same
stream. A view whose camera looks from another direction then blends
the splats in the wrong order: near splats are drawn under far ones.

App-written populations do not have this defect. Since the per-view
draw lists ([per-view-draw-lists](../done/per-view-draw-lists.md)),
every camera view of a field-keyed or position-keyed population sorts
its own list under its own camera. Shadow views stay unordered by
design: a depth-only pass gives the same result in any order.

## Done looks like

- Each camera view of an indexed population has its own id stream,
  sorted under that view's camera, with the same gates as the scene's
  one (the direction gate, the sort thread above `SYNC_SORT_MAX_KEYS`,
  the packed runs and the LOD ramp per view).
- The handed-off key column stays one copy per population. Only the id
  stream and the selection are per view.
- Facts per view: the sorts, the visible runs and the budget actually
  drawn.
- A test with two cameras on opposite sides of a cloud, each view
  blending back to front.

## Involves

`alloy/src/context/order.rs` (the indexed registry: one id stream per
ordering entry instead of one per buffer), `alloy/src/gpu/order_worker.rs`
(jobs per view), `packages/3d/src/scene.ts` (camera views declare the
indexed order), `packages/3d/src/splat.ts`. The cost is a sort and a 4
bytes per record upload for each view on a frame where its camera turns
past the gate; a parked view costs nothing.
