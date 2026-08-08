---
type: backlog-item
title: 3D roadmap - toward Three.js parity
description: The tracking file for @solidrt/3d - what has landed and a ranked list of everything still needed to make the package practically comparable to Three.js, each item pointing at its engine backlog file and status. Ranked by structural leverage first (per the differentiators note), then by the research staging.
status: open
timestamp: 2026-08-06T00:00:00Z
---

# 3D roadmap

One place to track `@solidrt/3d` against the obvious benchmark. The design
rationale lives in the research notes and stays there:
[scene-graph-3d](../research/scene-graph-3d.md) (architecture and staging),
[3d-differentiators](../research/3d-differentiators.md) (where the native
model can end up ahead, and the ranking philosophy this file inherits),
[declarative-gpu](../research/declarative-gpu.md) (the layering rules).
This file is the scoreboard: what exists, what is next, in what order.
Update it as items land - move them into the landed section with a date,
and keep the bracketed statuses in sync with the backlog files they point
at.

"Parity" here is practical parity - the common 3D needs of an app covered
without writing GLSL - not feature-count parity with a decade of
accumulation. Per the differentiators note, items that compound a
structural advantage rank above items that merely close distance, which is
why the top of the list is not what a feature checklist would pick.

## Landed

v1 shipped 2026-08-05 (staging step 2 of scene-graph-3d), live-verified on
Linux. Usage and traps: `packages/3d/AGENTS.md`; runnable examples:
`packages/3d/examples/`.

- **Retained imperative core**: `createScene`/`createMesh`/`createGroup`,
  `add`/`remove`, `setTransform`/`setVisible`, `setGeometry`/`setMaterial`,
  `setMeshParams`. Plain objects with dirty flags, microtask-batched, one
  depth-buffered draw target, one `setDrawParams` per changed mesh; a
  static scene costs zero passes and zero JS.
- **Component face**: `Scene`/`Group`/`Mesh`/`PerspectiveCamera` +
  `useScene`, syncing props into the core over context; output is an
  ordinary `<texture>` leaf (layout, transforms, blendMode, pointer events).
- **Math module** (`@solidrt/3d/math`): column-major Mat4
  (`perspective` with the y-down clip flip baked in, `lookAt`, `compose`,
  `multiply`), Vec3 ops, zero per-frame allocation.
- **Geometry**: `box`, `plane`, `sphere`, `torusKnot`; shared 8-float
  layout (aPos/aNormal/aUV), uint16 indices, lazy shared app-lifetime GPU
  buffers, `disposeGeometry`. Breadth added 2026-08-07 (demand-driven,
  item 10's cheap half): `cylinder` (tapered = truncated cone), `cone`,
  `torus`, `circle`, `ring`; same day the 64k-vertex ceiling fell -
  `Geometry.indices` may be a Uint32Array and the draw entry follows the
  array type (the engine took uint32 all along; the cap was library-side).
- **Overlay projection** (2026-08-07): `scene.project(point)` - world
  point to scene pixels (top-left origin; `w` = camera-forward distance,
  null behind the camera) - plus a `scene.viewProj(out?)` copy getter.
  The forward half of picking (item 4); HUD overlays stop rebuilding the
  camera matrices by hand.
- **Profile kit** (2026-08-07, item 10's profile tier): `extrude` (swept
  along z, quarter-round bevels via miter inset), `lathe` (solid of
  revolution about y from a CLOSED profile - watertight by construction,
  flat caps on partial sweeps), flat `shape`, with `fillet`/`roundRect`
  profile helpers and exported ear-clip `triangulate`. Real texture UVs
  throughout; sharp points crease, smooth-tagged points share normals;
  outputs pick uint16/uint32 indices by vertex count. Verified by
  manifold/winding/area checks, not just typecheck.
- **The "colored" vertex layout** (2026-08-08, item 10's first named
  layout): `withColors(geometry, fill)` derives a 12-float geometry
  appending `aColor` vec4 - the per-vertex data channel (tint, baked
  ambient occlusion, any four scalars) - from any standard-layout
  geometry; a `shaderMaterial` vertex stage reading `aColor` opts into
  the layout automatically, and a geometry/material layout mismatch
  throws at add() (layout is stride - a mismatch renders garbage, so it
  is an error, not a skip). `@solidrt/3d/glsl` exports
  `LIT_VERTEX_COLORED` - LIT_VERTEX plus aColor forwarded raw as vColor -
  so colored geometry with standard lighting rewrites nothing.
  `fillColors(vertices, fill, first?, count?)` is the in-place primitive
  under withColors (implemented on top of it): a merging builder bakes
  colors over its packed buffer without hardcoding layout offsets, and
  because the callback reads pos/normal/uv from the buffer itself, a
  transform-baking packer hands the baker world-space vertices.
  Demand evidence: with no spare channel, an
  app baking per-vertex occlusion and tint had to hijack the uv slot,
  which cost it real UVs and forced re-emitting library geometry by
  hand - the layout channel, not more generators, was the blocker to
  consuming library geometry directly.
- **Standard uniform set + exported lighting GLSL** (2026-08-07, item 2 -
  see the ranked entry below for the contract). Engine prerequisite the
  same day: shared target params and bindings tolerate ZERO coverage
  (stored and skipped until a declaring entry arrives; arity still
  validated wherever declared, single-program targets stay strict), so
  the scene publishes uCamPos beside uViewProj whatever materials are
  attached - pixel-asserted in `alloy/examples/draw_list.rs`.
- **Materials**: `unlit({ color?, map? })`, and `shaderMaterial` - user
  GLSL as a first-class material (uMVP contract, params/textures,
  depth/blend/cull/topology options).
- **Camera control**: `createOrbitCamera` - drag-to-rotate, wheel-to-zoom,
  auto-orbit, plain-state pose get/set, reactive `orbiting()`.
- **Engine prerequisites**, all landed 2026-08-04: ordered draw lists
  ([gpu-draw-list](gpu-draw-list.md) [partial] - cross-device verification
  pending), index buffers, cull mode, per-instance attributes (recorded in
  [gpu-pipeline-extensions](gpu-pipeline-extensions.md)).

Known v1 limits, documented as traps in `packages/3d/AGENTS.md`: opaque
only, Euler-only rotation, two vertex layouts only (no tangents or skin
weights yet), entry rebuilds append at the list end.

## The ranked list

1. **Camera off the O(scene) path: per-entry uModel + target-shared
   uViewProj.** DONE 2026-08-06. Engine:
   [gpu-shared-draw-params](gpu-shared-draw-params.md) [done]
   (`setTargetParams` + `createDrawTarget` positional params). Library:
   uMVP split into per-mesh `uModel` + shared `uViewProj`; a camera move is
   one write, and the `shaderMaterial` vertex contract now requires both
   matrices (changed before any app shipped against uMVP). The engine
   item's sampler half (`setTargetTextures`, shared target-level bindings)
   landed the same day - the binding channel items 14 and 15 consume.
2. **The standard uniform set and the exported-GLSL policy.** DONE
   2026-08-07 (differentiators note, implications item 4). The contract:
   per-mesh `uModel` plus opt-in `uNormal` (world inverse-transpose,
   written beside uModel for materials that declare it - correct normals
   under non-uniform scale), shared `uViewProj` plus `uCamPos` (one
   target write per camera move, whatever materials are attached - the
   engine's zero-coverage relaxation, recorded in
   [gpu-shared-draw-params](gpu-shared-draw-params.md)). Missing
   uModel/uViewProj throws at shaderMaterial() creation.
   `@solidrt/3d/glsl` exports `LIT_VERTEX` (pins the vWorldPos/vNormal/
   vUv varying interface) and pure `HEMISPHERE`/`LAMBERT`/
   `BLINN_SPECULAR`/`FRESNEL` functions - the pieces item 5's lit
   material classes will also be built from, so custom materials never
   become second-class.
3. **UI as live 3D content.** Engine:
   [snapshot-boundary-texture-id](snapshot-boundary-texture-id.md) [open].
   A snapshot boundary's retained texture as an ordinary TextureId that
   updates as the subtree repaints - the load-bearing piece of the
   one-tree differentiator (real UI on 3D geometry), and the capability
   the browser architecturally forbids.
4. **Picking.** Library only, no backlog file (staged in scene-graph-3d,
   deliberately deferred past v1). Raycast against bounding volumes,
   driven from the `<texture>` element's pointer events (localX/localY
   arrive with ancestor transforms undone). Three's Raycaster equivalent,
   and the interaction half of item 3. The forward direction landed
   2026-08-07 (`scene.project`, see landed); what remains is the inverse:
   pixel to ray to hit.
5. **Lights and lit materials (lambert/phong).** Engine:
   [gpu-uniform-arrays](gpu-uniform-arrays.md) [open] - light lists
   without baking a cap into shader source (the agreed answer; the
   fixed-scalar workaround was rejected in scene-graph-3d). Library: light
   scene nodes plus material classes built per item 2's policy.
6. **Transparency.** Engine: the blend factor vocabulary,
   [gpu-pipeline-blend-modes](gpu-pipeline-blend-modes.md) [deferred] -
   the old "sorting plus premultiplied" blocker inverted once the library
   existed, because the scene graph is the sorter. Library: back-to-front
   transparent sort over stable DrawIds (recomputed only when the camera
   moves), fix the entry-rebuild-at-end order trap, settle premultiplied
   vs straight.
7. **Real models: a glTF subset loader.** Library + CLI; the engine entry
   ticket (index buffers) was paid 2026-08-04. Direction per the
   differentiators note: run the mature loaders under Bun in the CLI at
   build/pack time and ship pre-interleaved buffers in the exact addDraw
   layout, so runtime loading is a buffer upload. Runtime-fetched user
   models stay a separate, later problem.
8. **Mipmaps.** Engine: [gpu-mipmaps](gpu-mipmaps.md) [deferred].
   Textured models alias immediately at minification; the one engine item
   staging step 3 (real models) still needs.
9. **MSAA on pipeline targets.** Engine:
   [gpu-target-antialiasing](gpu-target-antialiasing.md) [open].
   Silhouette jaggies are the dominant artifact on filled geometry.
10. **Geometry breadth and the vertex-layout ceiling.** Library. The
    cheap primitives landed 2026-08-07 (cylinder, cone, torus, circle,
    ring), the profile tier the same day (extrude/lathe/shape with fillet
    and triangulation), and the first named layout 2026-08-08: "colored"
    appends `aColor` vec4 via `withColors` (see landed) - vertex colors
    and per-vertex baked data are covered. Still demand-gated: capsule,
    and further named layouts (tangents, skin weights) when items 7 and
    16 force them; the direction stays a small set of named layouts, not
    an open BufferGeometry-style model.
11. **Quaternions.** Library. Rotation is Euler x-y-z only; quats unlock
    slerp, gimbal-free tumbling, and glTF node transforms (glTF stores
    rotation as a quaternion, so item 7 will force the representation
    question anyway).
12. **Instanced mesh sugar.** Library only; the engine half
    (per-instance attributes) landed 2026-08-04. One draw entry and one
    params write covering N instances is also the standing answer to the
    churn workloads (particles, projectiles) that the FFI boundary
    punishes.
13. **Camera and control breadth.** OrthographicCamera is small library
    work. First-person controls are blocked on engine
    [relative-mouse-input](relative-mouse-input.md) [open] - pointer
    lock/relative motion - not on anything in the GPU stack.
14. **Environment tier: skybox, reflection/environment maps.** Engine:
    [gpu-cube-maps](gpu-cube-maps.md) [open]. Demand-gated. The binding
    side is already paid: a shared target-level sampler
    (`setTargetTextures`, landed 2026-08-06) binds an environment map once
    per scene target; cube maps are the remaining engine gap.
15. **Shadow maps.** Engine: sampleable depth and a depth-func option,
    both deferred in [gpu-pipeline-extensions](gpu-pipeline-extensions.md);
    the map itself binds through the shared target-level sampler channel
    (landed 2026-08-06).
16. **Skinning and morph targets.** Engine: float texture formats (same
    extensions file). Per-vertex JS is ruled out by the interpreter, so
    bone matrices live in textures and are sampled in the vertex shader.
17. **PBR and the color-space decision.** The furthest tier: physically
    based lighting math forces the sRGB/linear question the pixel
    contract currently answers with "non-linear RGBA8 everywhere".
18. **Scene scale.** Frustum culling in the library when a consumer
    needs it (scenes under a few thousand nodes will not), and the
    recorded escape hatch behind everything above: the draw-list design
    deliberately allows the scene walk to move into core without an
    app-facing API change. Explicit non-goal until something demands it.

## Not in scope

Deliberately left behind, per the scope section of scene-graph-3d:
multi-backend abstraction (GLSL ES 3.00 is a settled bet), shader node
graphs / TSL, a WebGL-style global-state renderer, and a post-processing
composer (the window shader and `<texture blendMode>` compositing already
cover that tier).
