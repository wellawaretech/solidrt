---
title: 3d transforms are float32 from the FFI boundary on, so content far from the origin jitters and steps
description: A node position is an f64 JS number until setTransform copies it into a Float32Array; the spatial arena composes world matrices in f32 and the vertex stage multiplies that uModel by a uViewProj carrying the camera translation, so past roughly 10^4 to 10^5 units from the origin vertices jitter and slow motion steps; a snapped per-target render origin plus f64 stored translations would fix it with no API change and keep the one-write camera move.
created: 2026-10-10
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
- The 2d side: [2d-baked-layers](2d-baked-layers.md) stage B2 part 4
  fixes the same limit for tiles with chunk-local record coordinates. If
  the render origin lands in the spatial core, the 2d sprite layer's
  camera can take it too.

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
