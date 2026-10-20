---
title: 3d transforms are float32 from the FFI boundary on, so content far from the origin jitters and steps
description: A node position is an f64 JS number until setTransform copies it into a Float32Array; the spatial arena composes world matrices in f32 and the vertex stage multiplies that uModel by a uViewProj carrying the camera translation, so past roughly 10^4 to 10^5 units from the origin vertices jitter and slow motion steps; a snapped per-target render origin plus f64 stored translations would fix it with no API change and keep the one-write camera move.
created: 2026-10-10
completed: 2026-10-20
---

# 3d transforms are float32 from the FFI boundary on

## Symptom

A scene placed far from the origin - a long track or a flight over a map
at metre units, anything planet-scale - degrades with distance: the
vertices of a mesh near the camera jitter against each other as the camera
moves, a slowly moving camera or object advances in visible steps, and
lighting that reads world position (specular, fog, shadow lookups)
shimmers. f32 has a 24-bit significand, so its spacing is about 1 mm at
10^4, 8 mm at 10^5, 6 cm at 10^6 and one unit at 10^7; Unity's inspector
warns past 100000 units for this reason.

Nothing states the limit (`packages/3d/AGENTS.md`, Coordinates and
rotation, says nothing about range), and every layer below the JS API is
f32:

- **The boundary.** `node.position` is plain JS numbers (f64), but
  `setTransform` copies the write into one `Float32Array(10)` carrier
  (`transformScratch` in `packages/3d/src/node.ts`), and world reads come
  back through a `Float32Array(16)`.
- **The arena.** `Mat4 = [f32; 16]` (`alloy/src/spatial/mod.rs`): local
  transforms and composed world matrices are f32 (`spatial/math.rs`), and
  culling, LOD, picking, collision and the BVH all test in that space.
- **The GPU.** The flush writes the node's world matrix as `uModel`; the
  camera is one shared `uViewProj` per target that carries the eye
  translation (`cameraParams` in `packages/3d/src/camera.ts`); the vertex
  stage multiplies the two (`gl_Position = uViewProj * uModel * ...` in
  `material.ts`). Two large translations cancelling inside an f32 product
  is the classic source of the jitter, however precise the stored
  positions are.

That last split is deliberate and worth keeping: `uModel` is
camera-independent, so a camera move is ONE shared write per target, not
a write per entry. A fix has to keep that property or pay O(entries) per
camera move.

## Three, Unity, Godot

- **Three**: builds `modelViewMatrix` (`camera.matrixWorldInverse *
  object.matrixWorld`) per object per frame on the CPU in f64 and lights
  in view space, so its vertex math is camera-relative by construction -
  bought by the per-frame JS walk we moved into Rust. Paths that read
  world position on the GPU (shadow and environment lookups) still lose
  precision, and nothing rebases far positions.
- **Unity**: Transforms are float32 throughout. HDRP renders
  camera-relative by default (shader authors get absolute world back
  through `GetAbsolutePositionWS`); URP does not. A floating origin
  (shift the world when the camera strays) is a community answer, not an
  engine feature.
- **Godot**: large world coordinates are a compile-time engine option
  (`precision=double`), off in the official builds and export templates;
  it makes positions f64 in the engine, the renderer adjusted to match.

The GPU half is solved by camera-relative rendering wherever it is
solved; the storage half by f64 positions (Godot's double build) or a
floating origin.

## Done looks like

A scene 10^6 units or more from the origin, the camera close to its
content, draws with no vertex jitter and no stepping, and picking,
collision, culling and shadows agree with what is drawn. Positions are
already f64 numbers in the API, so the fix is internal: no signature
changes.

The shape to explore, in two halves:

1. **A render origin per target, snapped.** A node already carries one
   draw sink per target (`DrawSink` in `spatial/mod.rs`), so the flush can
   write `uModel` relative to a per-target origin, with that target's
   `uViewProj` built relative to the same origin. The origin moves in
   coarse steps (a power-of-two grid, re-snapped when the camera leaves
   its cell), so between snaps a camera move is still one shared write; a
   re-snap rewrites that target's entries once, rare by construction. An
   origin of zero is today's behaviour exactly, so a scene that stays in
   one cell renders as now.
2. **Precise stored translations.** A relative upload fixes the GPU
   product but not storage: a top-level node at 10^6 still sits on a 6 cm
   grid in the arena. The arena's translations (local and world) go f64,
   rotation and scale staying f32, with an f64 boundary carrier; the
   origin subtraction then happens in f64 before the f32 upload, and
   culling, picking and collision run origin-relative. The alternative, a
   floating origin that rewrites every root (and every world-space value
   the app, a transition target or a physics body holds), leaks into app
   code, which is why the engines that solved storage went f64.

Design points to settle:

- **The shader contract.** `vWorldPos` and `uCamPos`, in the stock
  materials and in every `shaderMaterial` that declares them, become
  origin-relative. Effects that need absolute world coordinates
  (triplanar mapping, world-space noise) get the origin as a uniform to
  add back, HDRP's `GetAbsolutePositionWS` in our spelling. Within the
  first cell nothing changes, so this matters only to scenes that leave
  it. Lights, probes, fog and shadow cameras upload origin-relative too.
- Instance records, splat centres and record meshes are local to their
  mesh node, so they ride the node's precision unchanged; a population
  spread over 10^6 units of one mesh's local space does not, and the docs
  say so.
- Depth range is the other half of large worlds and a separate item:
  [gpu-depth-func](gpu-depth-func.md) (reversed-z).
- The 2d side, half done. The tile layer is exact at any distance:
  records are chunk-local ([2d-baked-layers](../done/2d-baked-layers.md)
  stage B2) and a tile view's stage takes the camera SPLIT by chunk
  (`uOrigin`, the chunk the camera point lies in, plus the remainder in
  world px; `tileCameraParams` in packages/2d/src/tile-pipeline.ts) so it
  subtracts small exact integers before scaling to pixels - a render
  origin that re-snaps on every camera write at no cost, because a
  chunk entry carries only its indices. The SPRITE and record layers
  still write the camera as raw world floats (`cameraParams` in
  packages/2d/src/camera.ts: `uCamera.xy` is the camera point, the stage
  computes `world - uCamera.xy` in f32), so a sprite population 10^7 px
  out shimmers exactly as a 3d scene does. Its fix is this item's shape
  one dimension down: the pose record (`iPos`, core-written from the
  spatial arena) and the camera both relative to a per-view origin, the
  origin snapped to a coarse grid so a camera move stays one shared
  write; the tile view's split is the spelling to copy. If the render
  origin lands in the spatial core, the sprite layer's pose sink takes it
  for free.

Additive: an app that stays near the origin sees no change, and nothing
in the public API changes shape.

## Involves

`alloy/src/spatial/` (translation storage in `mod.rs` and `math.rs`, the
origin and the `uModel` write in the flush, the cull, pick and collide
paths made origin-relative), the flux spatial plugin's transform
marshalling, `packages/3d` (the `node.ts` carriers, `camera.ts` and every
place that builds a target's camera params, the light and probe uploads,
the origin uniform in the GLSL) and AGENTS.md (the range under
Coordinates and rotation, the shader contract under Custom looks).

## Landed

2026-10-20, in the shape agreed after the proposal: f64 INSIDE the arena,
f32 only at the GPU edge and in geometry data, and a render origin per
target where the f32 is made.

- **The arena is f64.** `alloy/src/spatial/`: positions, matrices
  (`Mat4 = [f64; 16]`), boxes, frustums, the BVH, viewers, grids,
  springs, IK, the rigs' pose math and the transition tracks. Geometry
  stays f32 where it is local to a node: shape vertices (`pick::Shape`,
  the picking copy of a geometry), occluder meshes in, clip keys
  (`ClipChannel` storage, sampled out in f64), the morph weights'
  staging. The shared motion curves (`alloy/src/motion.rs`, the element
  lanes' too) stay f32; a position track steps its f32 displacement
  around an f64 target.
- **The GPU edge is relative.** `SinkWriter` is f32 (`Mat4f`,
  `math::narrow`); a draw sink's `uModel` and a target's Position slots
  are written relative to the target's RENDER ORIGIN (`set_origin`,
  carried by `set_view`), the frustum, LOD view, occlusion raster and
  residency viewer derive in origin space (`relative_view`,
  `Frustum::from_view_proj(vp, origin)`, boxes shifted in f64 on test),
  and an origin change costs ONE write per target: `SinkWriter::
  shift_params(target, draws, delta)` -> `RasterCmd::ShiftDrawParams`,
  the raster adding the delta to each listed entry's uModel translation
  in one pass (`gl/target.rs shift_entry_params`), the Position slots
  restaged. Sinks still owing their first write are left out and written
  under the new origin. Instance records and palettes relativize at
  STAGING time in f64 (anchor inverse cached per flush per group), and a
  buffer without an anchor takes a record origin (`set_instance_origin`).
- **The policy is the consumer's.** `packages/core/src/origin.ts`
  (`originCell`, `snapOrigin`): a power-of-two cell, re-snapped when the
  eye leaves a one-cell band around its cell. The 3d camera snaps on 64
  near planes (`camera.ts`, `ORIGIN_CELL_PER_NEAR`), keeps absolute f64
  matrices for JS and relative ones for the GPU (`viewRel`,
  `viewProjRel`, `invViewProjRel`), and `cameraParams` writes
  `uViewProj`, `uInvViewProj`, `uCamPos` relative plus `uOrigin`. The
  shadow matrices are composed PER receiving target against its origin
  (`scene-shadows.ts receiverMatrices`: the map's relative view-projection
  translated by the origins' difference in f64), rewritten when a map
  camera or a receiver's origin moved; a probe face re-bases its own on
  render. The 2d sprite layer snaps its one origin from the primary view
  on 2^16 screen pixels (`views.ts`, `ORIGIN_CELL_SCREEN_PX`), restages
  the pose buffer through `setInstanceOrigin` and writes every view's
  `uCamera` relative; the gate viewers (2d views, the streaming source)
  pass their camera point as the origin.
- **Shader contract.** `uModel`, `uCamPos`, `uLightPos`, `vWorldPos` are
  origin-relative; `uOrigin` is a standard target uniform (declared by
  `sceneSource`, so every stock and tier-3 fragment has it); the fog
  height and the stock triplanar add it back. No compatibility shim: a
  custom shader reading `vWorldPos` as absolute adds `uOrigin` itself.
- **Carriers.** Every position or matrix argument of `flux:spatial` is a
  `Float64Array` (transform, worldMatrix, readTransform, setRest, rays,
  volumes, motion, view/proj/origin); geometry stays `Float32Array`
  (bounds, shapes, channels, weights, clips, springs, slot vectors, post
  matrices). `setView(target, view, proj, root?, origin?)` and
  `setInstanceOrigin(buffer, origin)` are the new surface
  (`packages/flux-types/gui/spatial.d.ts`).
- **Tests.** `alloy/src/tests/spatial.rs` (Render origin: relative
  params and slots, the one-write shift with summed deltas and the fresh
  exclusion, the frustum and LOD eye in origin space, raycast translation
  invariance at 10^6), `packages/core/tests/origin.test.ts`,
  `packages/3d/tests/large-world.test.tsx` (a lit, shadowed scene 2^20
  units out renders byte for byte like the one at zero, picks agree to a
  micron), `packages/2d/tests/large-world.test.tsx` (a sprite layer 2^24
  px out, past f32's whole numbers, likewise).
- **Docs.** `packages/3d/AGENTS.md` (Coordinates and rotation: RANGE;
  shaderMaterial; Custom looks tier 3; Fog; Views and scene params),
  `packages/2d/AGENTS.md` (The camera; Cameras and rotation trap), the
  flux-types header, `material.ts` and `camera.ts` doc comments.

Not done, on purpose: the 2d sprite layer serves views far apart from
each other with ONE origin (okf/backlog/2d-sprite-layer-chunked-records.md
is the chunked-records follow-up); the record layer (JS-written world
f32) keeps the origin at zero and says so; instance populations stay
local to their mesh. The "sprite layer takes the origin for free" claim
in the Symptom above was wrong: a per-target origin cannot re-base a
buffer shared by every view, which is why the 2d origin is per buffer.

## Findings

- A shared-buffer edge cannot take a per-view origin. The 3d fix hangs on
  `uModel` being per entry PER TARGET (a node carries one draw sink per
  target), so each target's origin applies to its own copy; a 2d pose
  buffer is one copy for every view, so its origin is per buffer, and
  the first live view decides it.
- An origin change must not rewrite entries. The raster's params update
  is per entry (a String, a Vec and a channel message each): 13 us per
  node on the tablet, 32 on the TV, so a re-snap over a few thousand
  shown entries would drop a frame. The shift command is one message
  with the draw ids (8 bytes each) and a single pass on the raster; the
  core's origin pass runs BEFORE the walk so a node that moves in the
  same flush is written fresh under the new origin, never shifted twice.
- Relativizing at staging rather than publish changed one observable:
  moving a palette's ANCHOR no longer republishes the palette (the
  anchor-local rows are unchanged), where it used to. The test that
  asserted the republish now asserts its absence; no consumer depended
  on the write, the texture content was identical.
- `Frustum::reaches_sides` needed a tolerance once the math went f64:
  the grid test said "touching a pushed-out plane at a face does not
  count", read as a strict zero, and in f64 the unprojected viewport edge
  on a grid line lands 7e-15 inside it, where f32's rounding had fallen
  the other way. The rule now holds with `TOUCH_EPS` (1e-9 of the box's
  size); the residency test that pinned the touching cell found it.
- Shapes stay f32 on purpose: the picking copy of a merged static scene
  is the one place a memory doubling would show, and a ray is in the
  node's local space by the time it meets a triangle, where f32 is
  exact enough; the transform in is f64.
- The 3d test's offset is 2^20 and the 2d's 2^24 so that every origin in
  play (the camera's cell, the shadow map's cell, the layer's) snaps to
  the near scene's origin plus the offset exactly; any other offset
  gives a different relative picture (the shadow map's texel grid lands
  elsewhere) and a few edge pixels differ, which is correct and not what
  the test is for.
- The walk's f64 cost, measured with `probes/scene-walk-whole-bench.tsx`
  (3000 nodes under one group rotated every frame: the whole tree
  recomputed per frame, the worst case for the arena's float width;
  release clients, cadence hold off on the devices), median flush ms per
  frame, f32 arena -> f64 arena:

  | device | f32 (before) | f64 (after) |
  |---|---|---|
  | TV, Philips TPM171E, armeabi-v7a | 96 - 99 | 103 - 115 |
  | tablet, SM-T500, arm64 | 23 (first window), then 39 | 26 (steady) |
  | desktop, Intel RPL-P | 6.0 - 7.5 (idle machine) | 8.3 - 8.9 (machine still loaded) |

  The ARMv7 TV pays 7 - 16% on this all-nodes walk (VFP double is slower
  than single there); on arm64 and x86-64 the change is inside the run
  to run noise (the tablet's own two readings differ more than the
  versions do; the desktop after-figure was read while builds and other
  processes ran and is not comparable). The raycast column did not move
  on any device. A real scene moves subtrees, not the whole tree, so the
  per-frame share is a fraction of this.
