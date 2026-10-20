---
title: Occlusion culling in the spatial core's cull pass
description: Authored occluders rasterized to a small CPU depth buffer per target inside the flush, every node's cull box tested against it in the cull pass as a second gate beside the frustum test (Godot's OccluderInstance3D model, Unity's occludee semantics), so a dense scene on a phone stops drawing what its nearest walls cover with zero per-frame JS; landed 2026-10-20 with the package's occluder shapes, cullState introspection and /stats figures.
created: 2026-10-20
completed: 2026-10-20
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
  (`setOccluder`, the prop) as `"geometry"` (the mesh's own triangles),
  `"bounds"` (its box as a solid), `{ box }`, `{ quad: [width, height] }`
  (Godot's QuadOccluder3D), `{ sphere: { center, radius } }` or
  `{ geometry }` (a proxy, Godot's ArrayOccluder3D); it moves with its
  node and a hidden node occludes nothing, so a door that closes
  occludes when closed. `occludersFromMeshes(scene, { minSize })` marks
  the large plain meshes by their geometry (Unity's static flag by size)
  and returns them.
- **A software depth buffer per target** (`spatial/occlusion.rs`): the
  shown occluders' triangles rasterized with the target's
  view-projection into a buffer `OCCLUSION_BUFFER_WIDTH` (128) texels
  wide at the target's aspect (read off the projection), rebuilt inside
  the flush only when the camera or an occluder moved, and not at all
  while no occluder is shown or every occluder is behind the camera;
  conservative to the texel (below).
- **The occludee test** in `cull_pass`, after the frustum test: the same
  cull box (own, instance union, joint union) and `cull_margin`, its
  nearest depth against the buffer's farthest depth over its screen
  rectangle, read exactly through a max pyramid walked as a quadtree. A
  mesh with `frustumCulled: false` is never occlusion-culled either. A
  node never occludes itself; an occluder can be occluded.
- **Per target**: `occlusion` on createScene and createView (default
  true), `setOcclusion` live; never a shadow tile nor a probe face. With
  no occluder shown nothing is built and nothing tested.
- **Introspection**: `scene.cullState(mesh)` and `view.cullState(mesh)`
  report "on" or the first of "hidden", "frustum", "occluded", "lod";
  `/stats` carries `occludedDraws`, `occluders`, `occluderTriangles`,
  `occluderTrianglesDrawn` (the ones that reached the raster),
  `occluderTrianglesSkipped`, `occlusionBuilds`, `occlusionBuildMs` and
  `occlusionBuildMaxMs` (the flux:spatial `occlusionStats()`).
- Tests: `alloy/src/tests/spatial_occlusion.rs` (the box behind a wall,
  the edge and near-plane straddlers, a moving or hidden occluder, the
  unions, every opt-out, self-occlusion, a perspective camera, the
  sphere, a mesh occluder, the budget, clearing and destroying);
  `packages/3d/tests/occlusion.test.tsx` (the same through the package,
  views, occludersFromMeshes, the components). AGENTS.md "Culling"
  grew the paragraph and the sealed-interiors text points to it.

## Decisions

**Software occlusion, Godot's model.** GPU occlusion queries (GLES 3.0's
`ANY_SAMPLES_PASSED`) answer a frame late AND cost a draw per query; a
hierarchical Z over the scene's depth needs a readback or compute
(GLES 3.1 is not on the table); Unity's baked PVS needs a bake step and
a static world. A CPU raster of authored occluders is engine-independent,
runs where the cull pass already runs, works for moving occluders, and is
what the one engine in our class that ships it at runtime chose.

**Inside the flush, not on the sort thread.** The plan had the build on
the sort thread with one frame of lag; that is not conservative: a box
visible from the new camera but hidden from the old one is culled for a
frame, a pop-in on every fast turn. Godot builds inside the frame too.
The build is bounded by a per-target triangle budget
(`OCCLUDER_TRIANGLE_BUDGET`, 4096: the occluders past it are skipped
that build, which is always safe) and measured below at well under a
millisecond for a wall; the sort-thread lane stays additive if a scene
ever asks for a larger occluder set.

**Conservative to the texel.** A texel takes an occluder's depth where
the triangle covers its center (two triangles sharing an edge leave no
seam; a whole-texel coverage rule did, and the max pyramid spread the
seam over every block it crossed), at the farthest depth the triangle
reaches inside the texel; the occludee's rectangle is rounded outward
and grown by one texel, which reaches past any silhouette texel the
center test over-covered. What the grid cannot resolve is lost: a crack
narrower than a texel between two occluders occludes as if closed.

**Mesh occluders are the exact shape.** A triangle occludes exactly what
lies behind it, so `"geometry"` and a proxy are correct for any mesh,
holes included; box and sphere stand for solids. A mesh whose triangles
move in the vertex stage (skinned, morphing, a sprite, an instanced
population) refuses `"geometry"`.

**No quality-row knob.** Occlusion helps most on the fill-bound low
row, and a row field equal in every row is noise. The switch is per
target, default on; shadow tiles and probe faces never.

**Introspection as a query, not a `/tree` field.** The `/tree` has no
scene introspection to put "occluded" beside; the core records why each
sink is off and `cullState` reads it, the first answer for
[3d-scene-draw-introspection](../backlog/3d-scene-draw-introspection.md).

**Occluders are authored, not inferred.** Godot and Unity both require
the author to mark occluders; inferring them from every mesh would
rasterize the whole scene on the CPU, the thing the GPU is for. The
size-threshold helper applies the exact geometry shape, so it is safe
by construction.

**Shadow views do not occlusion-cull.** A caster hidden from the camera
still casts a shadow the camera sees.

**No portals or zones.** The occluder buffer covers the doorway case (a
wall with a gap where the door is) without them; the layer pattern
remains for the deliberate sealed-interior switch and for the lights.

**Resolution is a constant, not a knob, and it is 128.** Godot's 1/8 of
1080p is 240; the probe on the tablet (below) culled the same share of
the field at 128 as at 256 while the build cost fell to a third, and a
texel at 128 is under one percent of the view, so the one-texel test
margin costs a silhouette nothing visible. A per-target override is
additive if a measurement ever asks for it.

**The occluder set is the audio blocker set.** Spatial audio occlusion
(a later item under [spatial-audio-emitters](../backlog/spatial-audio-emitters.md))
is a listener-to-emitter ray, which a screen-space buffer cannot answer
(an emitter behind the camera); it is a ray query over the same
node-space triangles, a core method beside the raster, with per-occluder
audio properties as additive fields on the shape. One authoring, two
consumers; a hidden occluder blocks neither.

**2d.** Nothing: 2d targets set no view, so nothing there is an
occludee; a 3d sprite is a mesh with a unit box and tests like any
other.

## Findings

Probe `probes/3d-occlusion-probe.tsx`: a wall of ten box occluders (120
triangles) with a doorway and 1600 solids behind it, the camera walking
through the doorway and back; this machine (performance profile),
release client, 720 px target, medians per 120 frames:

- Outside the wall, facing it: 846 to 978 of the 1600 solids occluded;
  the buffer build 0.3 to 0.85 ms (slowest frame 3.6 ms); the scene
  sync, the flush with the build and every occludee test inside, 2.0 to
  2.3 ms against 0.7 to 1.4 ms with the same wall drawn but not
  occluding (the frustum re-test of 1600 sinks a camera move pays
  anyway). The 1600 occludee tests are the larger share, about 0.5 us
  each: eight corner projections and the quadtree walk.
- Through the doorway and beyond, the wall behind the camera: 0
  occluded, the build 0.06 to 0.3 ms (the triangles clip away), the
  sync 1.0 to 1.5 ms, the coverage early-out skipping every test.
- With occlusion on and no occluder shown, nothing is cleared, built or
  tested: the first version cleared and pyramided an empty buffer per
  camera move, 0.1 to 0.2 ms for nothing, and the figures caught it.
Tablet (SM-T500, the low row's Mali device; the release player built
from this checkout, the same probe over the LAN):

- Baseline, the wall drawn but not occluding: sync 3.5 to 6.6 ms, the
  frustum re-test of 1600 sinks per camera move.
- Width 256: facing the wall 832 to 990 occluded, the build 1.9 to 2.7
  ms (slowest 6.5 ms), the sync 8 to 10 ms; the wall behind the camera
  still 0.5 ms of clear and pyramid for nothing, since "empty" was
  judged by triangles submitted, not texels covered (fixed: an empty
  buffer is one no texel was written to, and the pass skips the tests).
- Width 128, the final build: facing the wall 546 to 976 occluded at the
  same camera positions as 256's 700 to 990 (the same share within the
  walk's jitter), the build 0.6 to 1.0 ms (slowest 1.9 ms), 0.2 ms with
  the wall behind the camera; the sync 7.2 to 7.8 ms facing the wall
  and the same with the wall behind, against the baseline's 5.6 to 6.6
  ms at the same steady state (the tablet runs every session's first
  seconds faster, 4.4 ms, then settles; compare steady lines only). The
  occludee tests do not show above that noise at 128: the sink loop and
  the frustum re-test are the bulk, and a SIMD projection of the corners
  is the lever if a population ever asks.
- The one-texel rule and the quadtree query were both found by the core
  tests: the first version culled nothing behind a quad (its diagonal
  seam), the second culled nothing small behind something slightly
  larger (a coarse pyramid block reaching past the rectangle).
