---
title: Occlusion culling in the spatial core's cull pass
description: Frustum culling cannot know that a closed shell hides its interior, so a dense scene draws everything behind its walls; the candidate is Godot's software occlusion (authored occluders rasterized to a small CPU depth buffer per target, every node's world box tested against it in the flush's cull pass), which needs an explicit yes or no before it is built.
created: 2026-10-20
---

# Occlusion culling

## Symptom

`packages/3d/AGENTS.md` ("Views and layers") says it plainly: "there is
no occlusion or portal culling". A city block, a building interior, a
dungeon: everything inside the frustum draws, walls included, and the
GPU fill on a phone is spent on what the nearest wall covers. The manual
route today is the sealed-interiors pattern: the interior, the shell and
a doorway plug on their own layers and the scene's mask switched by the
camera's zone in JS. It works for one building; it does not scale to a
street of them, and it is exactly the per-frame-visibility policy the
core exists to own.

Unity ships it (baked PVS with static occluders and occludees, dynamic
objects as occludees); Godot 4 ships it (`OccluderInstance3D`: authored
occluder shapes rasterized on the CPU each frame into a low-resolution
depth buffer, every instance's AABB tested against it); Unreal uses the
GPU (hardware occlusion queries, then HZB). Three has none.

## Why the core

The test is per node per frame against the camera, the same class as the
frustum test that already runs in `cull.rs` and `mod.rs`'s `cull_pass`.
JS cannot do it (no world boxes without a round trip per node, and a
per-frame walk is the cost the core removed). It is a second gate on the
same draw sinks the frustum test switches, and it runs only when the
camera or an occluder moved, which the dirty tracking already knows.

## The candidate

Godot's model, which fits the core's shape and the GLES 3.0 floor:

- **Occluders are authored**: a box, a quad or a low-poly mesh declared
  on a node (`occluder` on a Mesh or Group, Godot's `OccluderInstance3D`
  shapes), moving with its node. Auto-occluders from large static meshes
  by a size threshold (Unity's static flag) are an additive helper over
  the same declaration, not the mechanism.
- **A software depth buffer per target** (Godot's is 256 x 128 or so):
  the occluders' triangles rasterized in the flush with the target's
  view-projection, on the sort thread the splat order already uses so
  the frame never waits; the buffer lags the camera by one frame on a
  fast turn, which Godot accepts (a conservative test: a node wrongly
  culled for one frame is the failure mode, so the test uses the box's
  nearest depth against the buffer's farthest depth over the box's
  screen rect, with a margin).
- **The occludee test** in `cull_pass`: a node that passed the frustum
  is then tested if the target has occluders; the switch is the same
  instance-count switch. Instanced meshes test by their union box;
  record meshes with bounds likewise; skinned parts by their joint-box
  union; a mesh with `frustumCulled: false` is never occlusion-culled
  either; shadow views do not occlusion-cull (a caster behind a wall
  still casts).
- **Cost model** to measure before deciding: Godot reports under a
  millisecond for thousands of occludees at its resolution, on a
  thread. Ours is one rasterization per target per camera change plus a
  rect read per node re-tested; a still camera costs nothing.

Alternatives, and why not first: GPU occlusion queries (GLES 3.0 has
`ANY_SAMPLES_PASSED`) answer a frame late and need a draw per query;
a hierarchical Z over the scene's depth needs a readback or compute
(GLES 3.1, not on the table); portals and zones are the manual pattern
already available through layers.

## Done looks like

- `occluder` declarations on nodes, the per-target software buffer, the
  test in the cull pass, `setOcclusion(scene | view, enabled)` per
  target (Godot's project-level switch, per target here since a minimap
  view wants none).
- `/tree` and the draw-introspection item report "occluded" as a reason
  a mesh did not draw ([3d-scene-draw-introspection](3d-scene-draw-introspection.md)).
- A probe with a wall of occluders and a field of meshes behind it: the
  draw count through `/gpu` falls as the camera moves behind the wall
  and recovers through a doorway, frame-stepped.

## The decision this item waits on

Yes or no on occlusion culling at all. The argument for: dense scenes on
phones are the target hardware, and the manual layer pattern does not
scale. The argument against: it is authored content (occluders) that a
port must add, the win is scene-shaped, and every mobile engine with it
ships it off by default. If yes, this moves to `plans/`; if no, the
sealed-interior pattern is documented as the answer and this becomes a
note.
