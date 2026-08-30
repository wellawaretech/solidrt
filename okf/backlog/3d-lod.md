---
title: Level of detail - distance-selected mesh variants as a core sink
description: A large scene ships every object at one triangle count; a track with a thousand trees either draws full-detail foliage at the horizon or nothing. A per-frame JS distance test over every LOD group is the O(scene) loop roadmap item 19 rules out, so the level select is a spatial-core sink - distance from a camera node picks which variant's visibility switch is on.
created: 2026-08-30
---

# Level of detail - distance-selected mesh variants as a core sink

## Symptom

Nothing in `@solidrt/3d` swaps a mesh by distance. Three's `LOD` object
(`addLevel(mesh, distance)`), Godot's visibility ranges and every game
engine's LOD group exist because a large outdoor scene cannot afford its
near-field triangle count at the horizon - and the reverse: a lone
billboard card looks wrong at arm's length. A track with a thousand trees,
fences and karts is exactly that scene, and on the low-end GPUs
[3d-low-end-gpu-performance](3d-low-end-gpu-performance.md) targets the
fill and vertex budget is the binding one.

The app-side answer is a per-frame loop: for each LOD group, distance to
the camera, pick the level, `setVisible` the winner. That is a test per
group per frame in the interpreter - the same O(scene) walk
[spatial-core](spatial-core.md) moved into Rust, and the reason roadmap
item 19 says frustum culling "in JS is ruled out, not deferred". LOD is
the same query over the same index and belongs beside it.

## Shape

A LOD group is a spatial node whose children are the variants, each with
a switch distance, plus a camera node to measure from:

- `createLod([{ node, distance }, ...])` / `<Lod levels>` - levels sorted
  by distance, the last one open-ended (or `null` for "draw nothing past
  here", the culling case for small props). Three's shape, one node.
- The core keeps, per LOD group, the level distances and the reference
  node (the scene camera by default; a view may name its own, since a
  minimap and the main view want different levels of the same tree).
  After the flush it computes the group-to-reference distance from the
  fresh world matrices, picks the level, and drives the variants'
  visibility switches (the existing `instanceCount` 0/N sink) - only
  when the level CHANGED, so a still scene writes nothing.
- Hysteresis (`LOD_HYSTERESIS`, a fraction of the switch distance) so a
  variant boundary does not flicker across frames; a per-level
  `transition` is deliberately later (a cross-fade needs the two variants
  drawn at once with a dither or alpha, a material concern).
- Picking, shadows and views see the group like any node: the hidden
  variants skip picking already (invisible meshes are skipped), and a
  shadow view casting from a nearer level than the camera draws is the
  usual engine simplification, acceptable.

Instanced populations (a forest as one `InstancedMesh` per level) LOD by
the same switch on the whole population; per-instance LOD is a later
item on the instancing side, not this one.

Placement: `alloy/src/spatial/`, a sink kind beside the visibility switch,
engine-independent Rust; marshalled through `flux:spatial`; `flux-types`
parity.

## Done looks like

A scene of a thousand trees, three levels each (mesh, simplified mesh,
sprite card), holds its frame time as the camera flies over, with the
level swaps invisible at the hysteresis band and zero per-frame JS,
measured by the spatial-core bench pattern. `examples/` gains one.

## Not in this item

Automatic mesh simplification (an authoring/bake-tool job, listed under
the interpreter losses in
[3d-differentiators](../notes/3d-differentiators.md)), screen-space-error
metrics instead of distance, cross-fade transitions, per-instance LOD.
