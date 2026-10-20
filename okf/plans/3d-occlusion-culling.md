---
title: Occlusion culling in the spatial core's cull pass
description: Authored occluders rasterized to a small CPU depth buffer per target on the sort thread, every node's world box tested against it in the flush's cull pass as a second gate beside the frustum test (Godot's OccluderInstance3D model, Unity's occludee semantics), so a dense scene on a phone stops drawing what its nearest walls cover with zero per-frame JS.
created: 2026-10-20
---

# Occlusion culling

## Problem

`packages/3d/AGENTS.md` ("Views and layers") says it plainly: "there is
no occlusion or portal culling". A city block, a building interior, a
dungeon: everything inside the frustum draws, walls included, and the
GPU fill on a phone is spent on what the nearest wall covers. The manual
route today is the sealed-interiors pattern: the interior, the shell and
a doorway plug on their own layers and the scene's mask switched by the
camera's zone in JS. It works for one building; it does not scale to a
street of them, and it is exactly the per-frame visibility policy the
core exists to own.

The test is per node per frame against the camera, the same class as the
frustum test that already runs in `cull.rs` and `mod.rs`'s `cull_pass`.
JS cannot do it (no world boxes without a round trip per node, and a
per-frame walk is the cost the core removed). It is a second gate on the
same draw sinks the frustum test switches, and it runs only when the
camera or an occluder moved, which the dirty tracking already knows.

Unity ships it (baked PVS with static occluders and occludees, dynamic
objects as occludees); Godot 4 ships it (`OccluderInstance3D`: authored
occluder shapes rasterized on the CPU each frame into a low-resolution
depth buffer, every instance's AABB tested against it); Unreal uses the
GPU (hardware occlusion queries, then HZB). Three has none. Decided yes
on 2026-10-20.

## Done looks like

- **Occluders are authored** on nodes: `occluder` on a Mesh or Group
  (`setOccluder`, the `occluder` prop) as a box (Godot's `BoxOccluder3D`),
  a quad (`QuadOccluder3D`), a sphere, or a low-poly mesh (the node's own
  geometry or a simpler one, `ArrayOccluder3D`); it moves with its node,
  so a door that closes occludes when closed. Auto-occluders from large
  static meshes by a size threshold (Unity's static-occluder flag) are
  an additive helper over the same declaration.
- **A software depth buffer per target** on the sort thread: the
  occluders' triangles rasterized with the target's view-projection into
  a small buffer (Godot's default is the viewport at 1/8 to 1/16 scale;
  ours a fixed OCCLUSION_BUFFER width with the aspect of the target),
  rebuilt only when the camera or an occluder moved. The frame never
  waits: the buffer is taken as it stands, one frame behind on a fast
  turn, and the test is conservative (a node is culled only when its
  box's NEAREST depth is behind the buffer's FARTHEST depth over the
  box's screen rectangle, with a margin), so the failure mode of the lag
  is a node drawn one frame too long, never one missing.
- **The occludee test** in `cull_pass`, after the frustum test, only on
  targets that have occluders and `occlusion` on: the same instance-count
  switch. An instanced mesh tests by its union box, a record mesh with
  `bounds` likewise, a skinned part by its joint-box union; a mesh with
  `frustumCulled: false` is never occlusion-culled either, and
  `cullMargin` widens the test as it widens the frustum's. Shadow views
  do not occlusion-cull (a caster behind a wall still casts); probe
  faces neither. A node that is itself an occluder can be an occludee
  (a wall hidden by a nearer wall), as in Godot.
- **Per target**: `occlusion: boolean` on createScene and createView,
  live through `setOcclusion` (Godot's project switch, per target here:
  a minimap view wants none), default off until a target has at least
  one occluder, then on; the quality tier can turn it off on the low
  row where the raster thread is the bottleneck.
- **Introspection**: `/tree` reports "occluded" beside "culled" as the
  reason a mesh did not draw, feeding
  [3d-scene-draw-introspection](../backlog/3d-scene-draw-introspection.md);
  `scene.stats()` (or the control API's `/stats`) carries the occluded
  count per target.
- A still camera and still occluders cost nothing; a probe shows the
  draw count through `/gpu` falling behind a wall and recovering through
  a doorway, frame-stepped.
- AGENTS.md "Culling" grows an occlusion paragraph and the
  sealed-interiors text points to it as the scalable route.

## Decisions

**Software occlusion, Godot's model.** GPU occlusion queries (GLES 3.0's
`ANY_SAMPLES_PASSED`) answer a frame late AND cost a draw per query; a
hierarchical Z over the scene's depth needs a readback or compute
(GLES 3.1 is not on the table); Unity's baked PVS needs a bake step and
a static world. A CPU raster of authored occluders is engine-independent,
runs where the cull pass already runs, works for moving occluders, and is
what the one engine in our class that ships it at runtime chose.

**On the sort thread, one frame of lag, conservative.** The buffer is
built off the main thread as the splat order is; the cull pass reads the
latest finished buffer. Godot accepts the same lag. The conservative
test (nearest box depth against farthest buffer depth over the rect, a
margin of a texel) makes the lag's error one frame of overdraw, never a
pop-in. A box that straddles the near plane is never culled.

**Occluders are authored, not inferred.** Godot and Unity both require
the author to mark occluders; inferring them from every mesh would
rasterize the whole scene on the CPU, the thing the GPU is for. The
size-threshold helper over static meshes is a convenience for a port,
additive, and declares the same thing.

**Shadow views do not occlusion-cull.** A caster hidden from the camera
still casts a shadow the camera sees. Godot and Unity exclude shadow
passes likewise; the shadow tiles keep their frustum test only.

**No portals or zones.** Portal culling is a second mechanism with its
own authoring (zones, portals, their links); the occluder buffer covers
the doorway case (a wall occluder with a hole where the door is) without
it, and the layer pattern remains for the deliberate sealed-interior
switch. If a port needs portals they are additive beside this.

**Resolution is a constant, not a knob.** One OCCLUSION_BUFFER width,
chosen by measurement in the Work below (Godot's 1/8 of 1080p is 240
wide); a per-target override is additive if a measurement ever asks for
it.

## Work

1. Core: `alloy/src/spatial/occlusion.rs` - occluder shapes per node
   (box, quad, sphere, triangle list) in node space, the per-target
   buffer (triangle raster with a conservative depth, the rect max-depth
   query over a mip chain or a summed-area pass), rebuilt on camera or
   occluder change; `cull_pass` takes the second gate; `Spatial::
   set_occluder/clear_occluder/set_occlusion`; tests in
   `alloy/src/tests/spatial_occlusion.rs`: a box behind a wall culls, a
   box straddling the wall's edge does not, a near-plane straddler never
   culls, a moving occluder uncovers, instanced and skinned unions.
2. Threading: the buffer build on the sort thread beside the splat sort
   (`sort-thread-core-placement` is the precedent), the cull pass reading
   the latest completed buffer.
3. Plugin: `flux:spatial` `setOccluder/clearOccluder/setOcclusion`,
   `/tree` "occluded" reason, `/stats` count;
   `packages/flux-types/gui/spatial.d.ts`.
4. Package: `occluder` on Mesh and Group (`setOccluder`, the prop),
   `occlusion` on createScene/createView and `setOcclusion`, the quality
   tier row, the static-threshold helper `occludersFromStatic(scene, {
   minSize })`; AGENTS.md "Culling".
5. Probe `probes/occlusion-probe.tsx`: a wall of occluders with a
   doorway and a field of meshes behind it; draw count through `/gpu`
   and the occluded count through `/stats`, frame-stepped; the buffer
   width chosen by measuring cull rate against raster cost on the Pixel
   and the TV.
6. Example `packages/3d/examples/occlusion.tsx`; the parity row
   re-linked and recounted.

## Findings

