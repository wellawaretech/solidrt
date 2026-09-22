---
title: 3D feature parity - Three, Unity and Godot against @solidrt/3d
description: One row per feature, four columns (Three, Unity, Godot, us), grouped by area, with a count per area of the rows at least two engines ship and how many of those we ship; every gap links its backlog item or says untracked, and each Unity/Godot cell says whether it was verified against the engine's docs or written from memory. Grew out of the Three-only feature survey (2026-09-08, revised 09-11) on 2026-09-22.
created: 2026-09-08
---

# 3D feature parity

**Measure, 2026-09-22: 44 of 114 rows that at least two engines ship, we ship.** Per area: Geometry and helpers 5/8; Materials 7/24; Lights and shadows 7/13; Renderer and post 8/27; Objects, scene graph, spatial 5/13; Animation 5/9; Loaders and assets 3/11; Controls and camera 3/5; Math 1/4.

The measure of `@solidrt/3d` against the three engines it is built from the
practice of. [3d-roadmap](3d-roadmap.md) still owns the destination and the
order at the capability level; this note is the granularity beneath it, the
specific thing a porter reaches for and does or does not find, and the
number that says how far the gap has closed. It began as a Three-only
inventory (2026-09-08, revised 2026-09-11); the Unity and Godot columns and
the counts were added 2026-09-22 when the backlog dropped its "demand-gated"
phrasing for parity: a feature two of the three ship is reason enough to
build it ([packages/3d/CLAUDE.md](../../packages/3d/CLAUDE.md), the
Three/Godot/Unity comparison as the shape gate).

The bar is still practical parity - the common needs of an app covered
without writing GLSL - not feature-count parity with a decade of accumulation.
The count is a measure of that gap, not a target of 100%: several rows are
deliberate non-goals and say so.

## How to read it

Cells: `yes` the engine ships it in the box; `addon` / `pkg` an official
addon (Three's `examples/jsm`) or official package (Unity's Splines,
Cinemachine, glTFast); `partial` a narrower form, said in the row; `editor`
exists in the editor only, not the runtime API (counts as no); `no`. The
`us` column is the same vocabulary. `item` links where the gap is tracked;
`untracked` means nowhere yet.

The count per area is: rows where at least two of the three engines have it
(`yes`, `addon`, `pkg`, `partial`, or Unity's `HDRP only`, which is in the
box) form the denominator, so a single engine's quirk does not weigh; rows
of those where our cell is `yes` form the numerator. `partial` on our side
does not count. The totals at the top are the sum.
`bun scripts/count-3d-parity.ts` derives the line from the tables and
rewrites it in place; run it after every edit to a row, so the number is
never typed by hand.

Verification: the Three column is carried from the 09-11 survey as written
and was not re-checked in this revision. Unity and Godot cells marked `(v)`
were checked against the engine's current documentation on 2026-09-22;
unmarked cells are from memory and are the first thing to verify when a row
becomes work. Unity is URP unless a row says HDRP; Godot is 4.x Forward+.

## Geometry and helpers

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| Primitive generators (box, sphere, cylinder, cone, torus, plane, ring, circle) | yes | partial: cube, sphere, capsule, cylinder, plane, quad only | yes: BoxMesh .. TorusMesh, PrismMesh (v) | yes | |
| Torus knot, lathe, icosphere detail | yes | no | no | yes | |
| Extrude / sweep along a path | yes | no (ProBuilder is editor) | partial: CSGPolygon3D depth, spin and path modes | yes | |
| Polyhedra (tetra, octa, icosa, dodeca) | yes | no | no | yes | |
| Capsule | yes | yes | yes (v) | yes | |
| Wireframe / edges geometry | yes | no | partial: viewport debug draw, not per mesh | yes | |
| Static debug helpers (grid, axes, box, arrow, plane) | yes | editor (Gizmos) | editor | yes | |
| Live helpers (camera frustum, light, skeleton, subtree box) | yes | editor | editor | no | [3d-debug-helper-builders](../done/3d-debug-helper-builders.md) settled the split: Three-only at runtime |
| Polar grid helper | yes | no | no | no | Three-only |
| Line width beyond 1px | addon: Line2 | yes: LineRenderer width | no: 1px, ImmediateMesh | no | untracked |
| Geometry ops: merge, transform, normals, crease, weld, non-indexed | yes | partial: CombineMeshes, RecalculateNormals | partial: SurfaceTool, MeshDataTool | yes | |
| Multi-material groups (submeshes / surfaces) | yes | yes | yes | no | untracked; see below |
| Curves and splines (Catmull-Rom, Bezier, spaced points) | yes | pkg: Splines | yes: Curve3D, Path3D | no: SweepPath is a polyline | untracked |
| Shape with holes into polygon / extrude | yes | no | no | no | Three-only |
| Bounding sphere | yes | no: Bounds box | no: AABB | no | not a gap, see below |

Multi-material groups: one mesh is one geometry and one material here, so a
port carrying a grouped geometry splits it into N meshes by hand. Worth
deciding deliberately rather than by omission: splitting is arguably the
cleaner model for our one-entry-per-mesh draw path, in which case
`mergeGeometries` should grow the documented inverse rather than leaving
callers to slice index arrays. Related but distinct: merge-by-material in
[3d-model-loader](../backlog/3d-model-loader.md).

Line width: neither Three's native lines nor ours have it (1px GL lines, the
ES core guarantee). Three answers with the `Line2`/`LineMaterial` addon,
quad-expanded lines in the vertex stage; Unity's LineRenderer is the same
idea as a component. Wireframe overlays on a HiDPI display need it.

## Materials

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| Unlit, Lambert/Phong, PBR metal-rough | yes | yes | yes | yes | |
| Extend the stock material with own GLSL (hook, not a graph) | yes: onBeforeCompile, TSL | yes: Shader Graph | yes: spatial shader vertex/fragment/light (v) | yes: `prelude`/`surface` on phong and standard, custom vertex with the stock fragment | landed 2026-09-22 |
| Clearcoat | yes: MeshPhysical | yes: Complex Lit (v) | yes (v) | no | untracked |
| Anisotropy | yes | HDRP only | yes (v) | no | untracked |
| Subsurface / transmission | yes | HDRP only | yes: subsurf_scatter (v) | no | untracked |
| Refraction | yes: transmission | HDRP only | yes (v) | no | untracked |
| Sheen | yes | HDRP only | no (v) | no | untracked |
| Iridescence | yes | HDRP only | no (v) | no | untracked |
| Toon shading | yes: MeshToon | no: Shader Graph | yes: toon diffuse/specular modes (v) | no | untracked |
| Matcap | yes | no | no | no | Three-only |
| Normal / depth debug materials | yes | no: debug views | partial: viewport debug draw | partial: hand-rolled through `overrideMaterial` | |
| Shadow catcher (invisible ground that receives) | yes: ShadowMaterial | no | yes: `shadow_to_opacity` (v) | no | untracked |
| Points material (size, attenuation) | yes | no | yes: use_point_size (v) | no: `gl_PointSize` works, no stock material | untracked |
| Dashed lines | yes | no | no | no | Three-only |
| Sprite / billboard material | yes | yes: particles, Billboard Renderer | yes: billboard_mode (v) | yes | |
| Radial / procedural sprite falloff | no | no | no | yes | |
| Depth test and depth write on stock materials | yes | yes: ZTest, ZWrite | yes: depth_draw_mode, depth_test (v) | partial: `shaderMaterial` only | untracked |
| Depth compare function | yes: depthFunc, default LEQUAL | yes: ZTest | partial: default or inverted only (v) | no | [gpu-depth-func](../backlog/gpu-depth-func.md) |
| Polygon offset | yes | yes: Offset | no | no | untracked |
| Alpha map, alpha hash, alpha antialiasing | yes | partial: alpha clip, dither by shader | yes: alpha_hash, alpha_antialiasing (v) | partial: `alphaTest` only | untracked |
| Bump / height / parallax map | yes: bumpMap, displacementMap | yes: height map | yes: heightmap parallax (v) | no | untracked |
| Ambient occlusion map | yes | yes | yes (v) | no | [3d-environment-additive](../backlog/3d-environment-additive.md) |
| Normal, emissive, metal-rough, specular, light maps | yes | yes | yes | yes | |
| Map transform (repeat, offset) | yes | yes | yes | yes | |
| Triplanar | no | no: Shader Graph | yes: uv1_triplanar | yes | |
| Per-material environment intensity | yes | partial | no | no | Three-only |
| Rim / backlight | no | no | yes (v) | no | Godot-only |
| Blend modes on stock materials | yes | yes | yes | yes | |
| Vertex colors | yes | yes | yes | yes | |

Missing knobs on the stock materials: `depth` and `depthWrite` exist only on
`shaderMaterial`. `polygonOffset` is absent from the whole stack: GL
`POLYGON_OFFSET_FILL` appears in `alloy/src/gl/pass.rs` purely as Impeller
state save/restore, never as a pipeline option, so z-fighting on coplanar
geometry has no fix and decals are blocked on it.

## Lights and shadows

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| Directional, point, spot, hemisphere / ambient | yes | yes | yes | yes | |
| Shadow maps for directional, spot, point | yes | yes | yes | yes | |
| Cascaded shadows with configurable splits | addon: CSM | yes | yes: PSSM, split_1..3 (v) | yes | landed 2026-09-22 |
| Soft shadows (PCF radius) | yes | yes | yes (v) | yes | |
| Contact-hardening (PCSS) | no | HDRP only | yes: light size / angular distance (v) | no | untracked |
| VSM | yes | no | no | no | Three-only |
| Light cull mask / layers per light | no | yes: cullingMask | yes: light_cull_mask, shadow_caster_mask (v) | no | [3d-light-layers](../backlog/3d-light-layers.md) |
| Light count | unbounded (recompiles) | 8 per object Forward, 256 per camera Forward+ (v) | clustered, hundreds | 8, fixed | [app-runtime-config](../backlog/app-runtime-config.md) |
| Area / rect lights | yes: RectAreaLight | partial: baked only in URP, realtime in HDRP | no | no | Three-only at runtime |
| Light probes / SH ambient | yes: LightProbe | yes | yes: LightmapGI probes, sky SH | no | [3d-environment-additive](../backlog/3d-environment-additive.md) |
| Reflection probes | partial: CubeCamera by hand | yes | yes | yes | |
| HDR environment, prefiltered specular | yes | yes | yes | yes | |
| Baked light map input | yes | yes, with a baker | yes, with a baker (LightmapGI) | yes: input only, no baker (Three has none either) | |
| Realtime GI (SDFGI, SSGI, SSIL) | no | HDRP only | yes: SDFGI, SSIL (v) | no | untracked |

## Renderer and post

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| HDR scene buffer, exposure, tone mapping | yes | yes | yes | yes | |
| Bloom / glow | addon | yes (v) | yes (v) | yes | |
| SSAO | addon | yes: renderer feature | yes (v) | no | untracked |
| SSR | addon | HDRP only | yes (v) | no | untracked |
| Depth of field | addon | yes (v) | yes: CameraAttributes | no | untracked |
| Motion blur | addon | yes (v) | no | no | untracked |
| MSAA | yes | yes | yes | yes | |
| FXAA / SMAA / TAA | addon | yes | yes: FXAA, TAA | no | untracked |
| Outline / selection highlight | addon | no | no | no | Three-only |
| Fog: linear, exponential, height | yes: linear, exp2 | yes | yes (v) | yes | |
| Volumetric fog | no | HDRP only | yes (v) | no | untracked |
| Background: color, cube, equirect, procedural sky | yes; sky as addon | yes | yes: Sky (v) | yes | |
| Clipping planes | yes | no | no | no | Three-only |
| Logarithmic depth buffer | yes | no | no | no | Three-only |
| Reversed-z | no | yes | yes: 4.3+ | no | [gpu-depth-func](../backlog/gpu-depth-func.md) |
| Multiple render targets | yes | yes | no: compositor sees the color buffer (v) | no | untracked |
| Compute shaders | yes: WebGPU only | yes | yes: RenderingDevice | no | untracked; GLES 3.1 compute is the native option [3d-differentiators](3d-differentiators.md) lists |
| Indirect / multi-draw indirect | yes: WebGPU only | yes | yes: RenderingDevice | no | untracked |
| 2D array and 3D textures | yes | yes | yes | no | untracked |
| Compressed textures (ETC2 / BC / Basis) | yes | yes | yes | no | [gpu-compressed-textures](../backlog/gpu-compressed-textures.md) |
| Render / draw introspection | yes: renderer.info | yes: stats, frame debugger | yes: monitors | no | [3d-scene-draw-introspection](../backlog/3d-scene-draw-introspection.md) |
| Views: multiple cameras, render to texture | yes | yes | yes: SubViewport | yes | |
| Frustum culling | yes | yes | yes | yes | core-side |
| Level of detail | yes | yes | yes | yes | core-side |
| Occlusion culling | no | yes | yes: occluders | no | untracked |
| XR | yes: WebXR | yes | yes | no | untracked; needs an explicit yes or no |
| Decals | addon: DecalGeometry | yes: Decal Projector (v) | yes: Decal node | no | untracked; blocked on polygon offset |
| Planar reflection / water | addon | HDRP only | no: SSR only | no | Three-only at runtime; cheap for us (a view with a mirrored camera) |
| 3D text / world-space labels | addon: TextGeometry, CSS2DRenderer | yes: TextMeshPro | yes: Label3D, TextMesh | no | [2d-world-space-text](../backlog/2d-world-space-text.md) tracks the 2d half; the 3d half is untracked |
| Particle system | no: Points only | yes | yes: GPU and CPU particles | no | untracked |
| Terrain | no | yes | no: addon | no | Unity-only |
| Gaussian splats | no | no | no | no | [gaussian-splats](../backlog/gaussian-splats.md); ahead of all three |

Post effects: a post-processing COMPOSER is a declared non-goal in the
roadmap (the window shader and `<texture blendMode>` cover that tier), so
the rows above are not an argument for `EffectComposer`. Each effect is
library work over `scene.hdrTexture` and a custom `resolve`, the way bloom
shipped ([3d-hdr-scene-buffer](../done/3d-hdr-scene-buffer.md)); nothing in
the engine is in the way any more.

## Objects, scene graph, spatial

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| Hierarchy, transforms, visibility, layers | yes | yes | yes | yes | |
| Instanced meshes | yes | yes | yes: MultiMesh | yes | |
| Per-instance custom value (frame, uv, any) | yes: instanced attribute | yes: property block | yes: custom_data | no | [3d-instance-additive](../backlog/3d-instance-additive.md) |
| Instanced billboards / point sprites | yes: Points | yes | yes | no | [3d-instance-additive](../backlog/3d-instance-additive.md) |
| Batched / record meshes | yes: BatchedMesh | yes: SRP batcher, static batching | partial: MultiMesh | yes | |
| Graph traversal by name, userData | yes | yes: Find, name | yes: find_child, metadata | no | untracked |
| Triangle-accurate picking | yes | yes: mesh collider | yes | yes | core-side |
| Overlap / sweep / move-and-slide | no | yes | yes | yes | |
| Rigid-body physics | no | yes | yes | no | [physics-core](../backlog/physics-core.md) |
| Joints, character controller | no | yes | yes | no | physics-core stage 2 |
| Vehicle | no | yes: WheelCollider | yes: VehicleBody3D | no | [physics-vehicle-controller](../backlog/physics-vehicle-controller.md) |
| Navigation mesh / pathfinding | no | yes | yes | no | untracked |
| Spatial audio (emitter, listener) | yes: PositionalAudio | yes | yes: AudioStreamPlayer3D | no | [spatial-audio-emitters](../backlog/spatial-audio-emitters.md) |
| Declarative transitions on nodes | no | no | yes: Tween | yes | |

## Animation

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| Clip playback, crossfade, blending | yes | yes | yes | yes: in core | |
| Author keyframe tracks in code; animate any property | yes | yes: AnimationClip.SetCurve | yes: Animation.add_track | no: clips are glTF-only, model-bound | untracked |
| Additive blending | yes | yes: layers | yes: AnimationNodeAdd2 (v) | no | untracked |
| Subclip / trim | yes | yes: at import | partial | no | untracked |
| Inverse kinematics | addon: CCDIKSolver | yes: Animator IK, Rigging package | yes: SkeletonIK3D (deprecated) (v), modifiers | no | untracked |
| Root motion | no | yes | yes | yes | |
| Skeleton sharing / wardrobe | yes: SkeletonUtils | yes: humanoid retarget | yes: retarget at import | yes: bindSkeleton (sharing; no retarget) | |
| Morph targets | yes | yes | yes | yes | |
| Skinning on GPU | yes | yes | yes | yes: palettes composed in core | |

## Loaders and assets

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| glTF / GLB | addon: GLTFLoader | pkg: glTFast | yes (v) | yes: subset | |
| Draco, meshopt, KTX2 inputs | addon | pkg | no (v) | no | [3d-model-loader](../backlog/3d-model-loader.md) |
| Tangents, second UV set | yes | yes | yes | no | 3d-model-loader |
| Per-material samplers (wrap, filter) | yes | yes | yes | no | 3d-model-loader |
| Merge by material | yes | yes | yes | no | 3d-model-loader |
| Runtime decode of fetched models | yes | yes | yes | partial: uncompressed only, interpreter interleave | 3d-model-loader |
| FBX, OBJ | addon | yes | yes: ufbx, OBJ (v) | no | non-goal: bake first; name the tool in the docs |
| STL, PLY, USDZ | addon | no | no | no | non-goal |
| Radiance .hdr | addon | yes | yes | yes | |
| EXR | addon | yes | yes | no | [3d-environment-additive](../backlog/3d-environment-additive.md) |
| Six-face cube images | yes | yes | yes | no | 3d-environment-additive |
| Build-time asset bake | no | yes: import pipeline | yes: import | yes: `srt tool` | |

## Controls and camera

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| Orbit / turntable | addon | pkg: Cinemachine | no: addon | yes | |
| First person, fly | addon | no: script over CharacterController | no: script | yes | |
| Boost / sprint action | no | script | script | yes | landed 2026-09-22 |
| Reference frame (ride a moving node) | no | yes: parenting | yes: parenting | no | [3d-first-person-reference-frame](../backlog/3d-first-person-reference-frame.md) |
| Zoom to cursor, dynamic pivot | addon: zoomToCursor | pkg | no | partial: hooks, no built-in | [camera-and-controls-extensions](../backlog/camera-and-controls-extensions.md) |
| Push / fly-through the model | addon: camera-controls | no | no | no | camera-and-controls-extensions |
| Fit to bounds | addon | pkg | no | yes: fit | |
| Map, trackball, arcball | addon | no | no | no | Three-only; camera-and-controls-extensions |
| Runtime transform gizmo | addon: TransformControls | editor | editor | no | Three-only at runtime |
| Projection, unproject, screen ray | yes | yes | yes | yes | |

## Math

| Feature | Three | Unity | Godot | us | item |
| --- | --- | --- | --- | --- | --- |
| Vectors, quaternions, matrices | yes: classes | yes | yes | yes: functions with out-params | |
| Color type | yes | yes | yes | no: conversions only | untracked |
| MathUtils (lerp, clamp, damp, smoothstep, rand) | yes | yes: Mathf | yes: @GlobalScope | no | untracked |
| Spherical coordinates | yes | no | no | no | Three-only |
| Curves | yes | pkg | yes | no | see Geometry |

## Not gaps

Recorded so they are not re-raised as findings:

- `computeTangents` / MikkTSpace. Our normal mapping derives the tangent
  frame in the fragment stage, so any UV-mapped geometry works with no
  tangent channel. The loader's tangent row above is about carrying an
  authored channel through, not about needing one.
- `interleaveAttributes`, `deinterleaveGeometry`, `estimateBytesUsed`. We are
  packed and interleaved by construction.
- `toTrianglesDrawMode`. Strips are a native topology.
- `center`, `normalizeNormals`, `computeBoundingSphere`. `center` is one
  `transformGeometry` over `geometryBounds` (the recipe is in
  `packages/3d/AGENTS.md`), computed and transformed normals are unit
  already, and Unity and Godot stop at the box, which every query and the
  LOD read here. Three names for one-liners.
- `Raycaster` as an object. `scene.raycast`/`pick` run in Rust over the
  retained index instead.
- Multi-backend abstraction and shader node graphs. Declared non-goals in the
  roadmap, and the axis Three's own current investment (WebGPURenderer +
  TSL) runs along, so the distance here grows on purpose.
- Editor-only features (gizmos, helpers, transform handles in Unity and
  Godot). We ship no editor; where Three has the runtime form it counts as
  Three-only above.

## The other direction

What we have that the engines do not, so the tables are not read as a
verdict:

- Spatial queries (`raycast`/`overlap`/`sweep`/`moveAndSlide`) run in Rust
  over a retained index rather than as a JS walk; Three has no collision
  tier at all.
- Frustum culling, LOD selection, the transparent sort, instance records and
  skinning palettes are core-side and cost nothing for a still camera, where
  Three re-tests and re-uploads per frame.
- Camera motion is one `setTargetParams` for the whole target, not a
  per-object matrix upload.
- Motion is a first-class lane shared with `@solidrt/2d`: `setTransition`,
  exit transitions, staggered groups, the `weights` lane. Only Godot has a
  tween in the box; neither has it as a property of the node.
- One flush writes every view's target; UI is live content in the same tree
  rather than a DOM overlay or a separate UI system; the asset pipeline is
  build-time; the frame clock is ours, so pixel-exact 3D regression tests are
  possible.
- Gaussian splats as a core content class, which none of the three ship.

## Untracked rows, in a suggested order

Not a decision, an order for turning untracked rows into backlog files by how
many ports each one unblocks. Each carries the standing shape gate: the
Three/Godot/Unity comparison in the proposal, per `packages/3d/CLAUDE.md`.

1. Author keyframe tracks in code and animate any property; additive
   blending and subclips beside it.
2. Depth test / depth write on the stock materials, polygon offset, and
   decals on top of them.
3. Multi-material groups, decided deliberately.
4. A stock points material; a particle tier after it.
5. Curves and splines in front of `tube`/`sweep`.
6. Graph traversal helpers, a `Color` type and `MathUtils`.
7. Texture arrays and 3D textures.
8. Clearcoat first among the physical extras; toon; shadow catcher.
9. SSAO, then the rest of the post effects over the scene buffer.
10. XR, occlusion culling, navigation, MRT, compute: each wants an explicit
    yes or no before it is a backlog file.
