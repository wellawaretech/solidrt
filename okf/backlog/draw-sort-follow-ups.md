---
title: Draw sort follow-ups
description: The core-side draw sort (2026-09-22) sorts opaques and cutouts front-to-back and transparents back-to-front per target with no JS per mesh, but it has no state-change grouping inside a bucket, a two-call key contract on bindDraw, an implicit background-first rule, and the scene still writes the camera to each target three times; each is a bounded change, listed here with the reasoning so a fresh session can pick any one up.
created: 2026-09-22
---

# Draw sort follow-ups

Context: `Spatial::set_draw_sort` (alloy/src/spatial/mod.rs, `order_pass`
and `draw_key`) keys every bound entry of a sorted target from its world
box center against the target's LOD view - three queues, opaque, cutout
(alpha-tested) and transparent - sorts natively, and writes the
permutation through `SinkWriter::write_order` when it changed; the
context composes unbound entries first. The 3d scene enables it on its
target and every non-override view and keys each mesh's node with
`setDrawKey`. Measured on the bench probe
(probes/3d-opaque-order-bench.tsx): 80 ms GPU pass in add order against
2 ms sorted, and a spinning parent of 300 meshes at the pre-sort JS
baseline. What follows is what a comparison with Unity, Godot and
Three.js still shows missing, plus two simplifications the work exposed.

## 1. No state-change grouping inside a bucket

Unity and Godot sort by shader and material inside each depth bucket;
Three puts material id above depth. The core's bucket tiebreak is bind
order, which groups materials only by accident of attach order. Whether
this costs anything depends on whether alloy's draw list skips a
redundant pipeline bind between consecutive entries - unmeasured. Start
with that measurement (two materials alternating vs grouped, GPU pass
time and a bind count); if binds are not skipped, a pipeline id on the
sink as the in-bucket tiebreak is the fix. Do not add the grouping on
assumption.

## 2. The key is a second call after every bind

`bindDraw` keeps six arguments because rquickjs caps function arity, so
the scene calls `setDrawKey` after each bind and a node's sinks share
the key. Works, but the contract is easy to forget (a bind without a key
sorts opaque, renderOrder 0). An options object on `bindDraw` carrying
normal/count/fade/transparent/renderOrder would make the key part of the
bind and drop the second crossing; the plugin stays thin (one object
read).

## 3. Background-first is implicit

Entries the core does not bind draw first, and enabling a sorted target
again re-issues the order - the call the scene makes after attaching a
background. Correct, but it says nothing about what it means. A pinned
class in `DrawOrder` (draws before every opaque) and binding the
background to the scene root would say it; what blocks that today is
`write_params` on a program without `uModel`, which would release the
binding - a `params: false` flag on the sink fixes it.

## 4. Three camera writes per target per move

On every camera move the scene writes target params (`cameraParams`),
the frustum (`setFrustum`, a view-proj matrix) and the LOD view
(`setLodView`, eye, forward, focal, ortho, bias) to each target - three
crossings carrying overlapping data derived from one camera. Now that
the core owns the per-target view, one `setView(target, view, proj)`
could derive the frustum, the LOD view and the sort view itself, and the
same view could feed the record-level projected-key re-sort
(okf/backlog/gaussian-splats.md, stage 2). A refactor across scene,
plugin and core that deserves its own plan; the payoff is one crossing
and one source of truth for what a target sees.

## Not in this item

- Depth pre-pass (okf/ideas.md): waits for a measured victim.
- Per-triangle or order-independent transparency: the engine contract
  is per-mesh, unchanged.
