---
title: Three.js feature survey - the inventory behind the roadmap
description: A fine-grained inventory of what Three.js has and @solidrt/3d does not, first taken 2026-09-08 and revised 2026-09-11 once the last roadmap capabilities landed, across geometry, materials, renderer features, objects, animation, loaders, textures, controls and math, plus the items our model makes unnecessary.
created: 2026-09-08
---

# Three.js feature survey

A feature-by-feature inventory, first taken 2026-09-08 when
`wireframeGeometry`/`edgesGeometry` landed and the question came up of what
else of Three's surface is absent. Revised 2026-09-11, after the last
[3d-roadmap](3d-roadmap.md) capabilities landed and invalidated about a third
of it.

This note does NOT rank and does not set direction. [3d-roadmap](3d-roadmap.md)
owns the destination and the order, and its "Not in scope" section owns the
deliberate refusals; several entries below are already roadmap items or backlog
files and are linked as such. What this note adds is granularity: the roadmap
thinks in capabilities, and a capability-sized entry hides which specific call
a porter reaches for and does not find. Entries with no link are untracked
anywhere as of the revision date.

The bar throughout is practical parity, the roadmap's definition: the common
needs of an app covered without writing GLSL, not feature-count parity with a
decade of accumulation.

## Closed since the first pass

Deleted from the inventory below rather than left behind with a "landed"
marker, since a gap list that keeps its resolved entries stops being readable.
Each one's shape decision and its divergences live where they govern -
`packages/3d/AGENTS.md` and the linked done item - not here.

- The static debug helper builders: `gridHelper`, `axesHelper`, `box3Helper`,
  `arrowHelper`, `planeHelper` ([3d-debug-helper-builders](../done/3d-debug-helper-builders.md)).
  `unlit` took `vertexColors`, so no line material was needed.
- `capsule` and `capsuleHelper` ([3d-capsule-primitive](../done/3d-capsule-primitive.md)).
- The polyhedron family: `tetrahedron`, `octahedron`, `icosahedron`,
  `dodecahedron` and generic `polyhedron`
  ([3d-polyhedron-primitives](../done/3d-polyhedron-primitives.md)).
- `setDrawRange`, and the fixed 8-float vertex layout that used to be the
  ceiling under tangents, skin weights and per-vertex custom data
  ([3d-vertex-data-model](../done/3d-vertex-data-model.md)).
- Morph targets end to end
  ([3d-morph-targets](../done/3d-morph-targets.md)): loader, `.srtm` v8, sparse
  GPU packing, weights as a core register written by `setMorphWeights`, by clip
  tracks, by the transition `weights` lane and per instance.
- Level of detail ([3d-lod](../done/3d-lod.md)).
- Bloom, and the scene buffer that unblocked every radiance-consuming effect
  ([3d-hdr-scene-buffer](../done/3d-hdr-scene-buffer.md)).
- `blend` on the stock materials. `UnlitOptions` carries it and `LitOptions` /
  `StandardOptions` extend that type, so an additive glow or a darkening decal
  no longer drops to a custom shader. `depth` and `depthWrite` stay
  `shaderMaterial`-only.
- The geometry operations: `computeVertexNormals` in place,
  `withNormals(geometry, creaseAngle)` (Three's `toCreasedNormals`, welded
  back to an indexed result), `toNonIndexed`, `mergeVertices`, and the
  `normalsHelper` builder (Three's `VertexNormalsHelper`)
  ([3d-compute-vertex-normals](../done/3d-compute-vertex-normals.md)).

One first-pass claim was wrong rather than stale, and is corrected in place
below: `gl_PointSize` works.

## Geometry helpers

The static builders landed. What stays open is exactly the live half, plus one
polar variant:

| Three | us |
| --- | --- |
| `PolarGridHelper` | `gridHelper` only; polar missing |
| `BoxHelper` (follows a subtree) | `box3Helper(bounds)` takes a computed box; nothing tracks a live subtree |
| `CameraHelper` (frustum lines) | missing |
| `Directional/Point/Spot/HemisphereLightHelper` | missing |
| `SkeletonHelper` | missing, though skeletons and `bindSkeleton` exist |

The split is the one settled in
[3d-debug-helper-builders](../done/3d-debug-helper-builders.md): Three makes
these scene objects, while Godot and Unity keep the equivalents in the editor
rather than the runtime API. The static ones became geometry builders next to
`wireframeGeometry`; the live ones have to follow a node every frame and would
be components, which is a much larger commitment for a debug aid.

Adjacent: neither Three's native lines nor ours have line width (1px GL lines,
the ES core guarantee). Three answers with the `Line2`/`LineMaterial` addon,
quad-expanded lines in the vertex stage. If wireframe overlays are meant to be
legible on a HiDPI display this is a real follow-up, not a nicety.

## Geometry operations

At parity since the normals ops landed (Closed above). `center`,
`normalizeNormals` and `computeBoundingSphere` moved to Not gaps.

## Primitives

At parity. Every generator in Three's set has a counterpart, including
`ShapeGeometry` and the `detail` subdivision that gets you an icosphere. The
one qualifier is `Shape.holes`, under Curves and shapes below.

Our flat fill is `polygon(profile)`: Three's `Shape` name stays free for the
input side (an outline with holes) should we grow one.

## Curves and shapes

Two gaps that feed builders we already have:

- Three's `Curve`/`CurvePath` family (`CatmullRomCurve3`, cubic and quadratic
  Bezier, arcs, `getSpacedPoints`) is what `TubeGeometry` and `ExtrudeGeometry`
  consume. Our `SweepPath` is a polyline with `smooth` flags, so anyone wanting
  a spline samples it themselves. A curve sampler in front of
  `tube`/`sweep`/`pathFrames` is the missing piece.
- `Shape.holes`. Our `triangulate` is ear-clip over a single simple contour, so
  `polygon()` and `extrude()` cannot produce a washer, a letter O, or a panel
  with a cutout. Ear-clipping with hole bridging is the standard fix.

## Multi-material groups

`BufferGeometry.groups` (index ranges each drawn with its own material) has no
counterpart: one mesh is one geometry and one material, so a port carrying a
grouped geometry splits it into N meshes by hand. Worth deciding deliberately
rather than by omission. Splitting is arguably the cleaner model for our
one-entry-per-mesh draw path, in which case `mergeGeometries` should grow the
documented inverse rather than leaving callers to slice index arrays
themselves. Related but not the same: merge-by-material in the model loader
([3d-model-loader](../backlog/3d-model-loader.md)).

## Materials

| Three | us |
| --- | --- |
| MeshBasic / Lambert / Phong / Standard | `unlit` / `phong` / `standard` |
| MeshPhysical (clearcoat, sheen, transmission, iridescence, anisotropy) | missing |
| MeshToon | missing |
| MeshMatcap | missing |
| MeshNormal / MeshDepth | missing; hand-rollable through `overrideMaterial` |
| ShadowMaterial (shadow catcher on invisible ground) | missing |
| PointsMaterial | missing, and see point size below |
| LineDashedMaterial | missing |
| SpriteMaterial | `sprite` |

Two items that matter more than the class list:

- **No `onBeforeCompile` equivalent.** Nothing injects into a stock material's
  shader. Wind sway on a `standard` mesh today means rewriting PBR inside
  `shaderMaterial`. Three has `onBeforeCompile` (and now TSL), Godot has the
  `spatial` shader with vertex/fragment hooks over the built-in lighting, Unity
  has Shader Graph over the standard lit. All three converge on "extend the
  stock material", we have only "replace it". Note the tension with the
  roadmap's non-goal: shader node graphs are out of scope, but a hook is not a
  graph. This is the largest untracked entry in the survey.
- **Missing knobs on the stock materials.** `depth` and `depthWrite` exist only
  on `shaderMaterial` (`blend` landed on all three stock materials).
  `polygonOffset` is absent from the whole stack: GL `POLYGON_OFFSET_FILL`
  appears in `alloy/src/gl/pass.rs` purely as Impeller state save/restore,
  never as a pipeline option, so z-fighting on coplanar geometry has no fix.
  Also `alphaMap`, `bumpMap`, `displacementMap`, `envMapIntensity`, `alphaHash`
  dithered transparency, and `aoMap` (tracked in
  [3d-environment-additive](../backlog/3d-environment-additive.md)).

## Renderer features

- **Clipping planes** (`material.clippingPlanes`, and the renderer-global set).
  Nothing. Section views, CAD-style slicing, sliced reveals.
- **Logarithmic depth buffer.** Planetary and other large-scale scenes.
- **VSM shadows.** We do PCF with a `radius`; Three offers VSM as an
  alternative shadow filter.
- **A fixed light cap.** `MAX_LIGHTS` is 8 with 8 shadow slots
  (`packages/3d/src/glsl.ts`), where Three recompiles per light count and has
  no fixed ceiling. A porter with a dozen point lights hits a wall we have and
  they do not. The cap is an app-level tunable candidate in
  [app-runtime-config](../backlog/app-runtime-config.md).
- **Post effects beyond bloom.** SSAO/GTAO, FXAA/SMAA/TAA, DOF, motion blur,
  outline (the selection highlight every editor tool needs), god rays. Read
  carefully against the roadmap: a post-processing COMPOSER is a declared
  non-goal there (the window shader and `<texture blendMode>` cover that tier),
  so this entry is not an argument for `EffectComposer`. It is the observation
  that the individual effects have no home either. What used to block all of
  them equally is gone: tone mapping and the sRGB encode ran in every fragment,
  so nothing downstream could see radiance, and the scene buffer plus its
  resolve slot ([3d-hdr-scene-buffer](../done/3d-hdr-scene-buffer.md)) fixed
  that. Bloom shipped on it as the worked example; each remaining effect is now
  library work over `scene.hdrTexture` and a custom `resolve`, with no engine
  half in the way.
- **Compute shaders.** Three's WebGPU backend has them through TSL compute
  nodes; nothing in alloy, flux or core dispatches compute, so GPU particles,
  GPU culling and GPU skinning all stay on the CPU side of the boundary.
  [3d-differentiators](3d-differentiators.md) lists GLES 3.1 compute as native
  access we could have and the browser could not; today it is unbuilt
  potential, not an advantage, and the survey should say so.
- **Indirect and multi-draw indirect.** Same status, same note.
- **Multiple render targets.** A draw target is single-color-attachment
  (`depth`, `format`, `samples`, `into`; one color format). Three exposes MRT
  on both backends. Deferred shading, G-buffers and any effect wanting normals
  or velocity alongside color are out of reach.
- **Draw and triangle introspection** (`renderer.info`): tracked as
  [3d-scene-draw-introspection](../backlog/3d-scene-draw-introspection.md).
- **WebXR.** Absent entirely. A whole category that deserves an explicit yes or
  no rather than silence.

## Objects and scene graph

- **Points and particles.** `topology: "points"` exists and `gl_PointSize`
  works - `packages/core/examples/gpu-particles.tsx` writes it in the vertex
  stage and shapes the splat with `gl_PointCoord`. What is missing is a stock
  points material exposing size and attenuation, so a particle field means
  writing the shader pair yourself. Three does not ship a particle system
  either, but it ships `PointsMaterial`.
- **Decals** (`DecalGeometry`). Bullet holes, tyre marks. Missing, and blocked
  by `polygonOffset` above.
- **Reflector / Refractor / Water.** A planar mirror is cheap for us (a view
  with a mirrored camera, which the view machinery already supports) and does
  not exist.
- **3D text.** Three has `FontLoader` plus `TextGeometry`, and
  `CSS2DRenderer`/`CSS3DRenderer` for labels. Our structural answer is better
  in principle (real UI elements in the same tree, roadmap item 3, and
  `snapshotTexture` is the piece that makes it real) but there is no
  world-space text today in either form; only 2d tracks it, in
  [2d-world-space-text](../backlog/2d-world-space-text.md).
- **Graph traversal helpers.** `SceneNode` carries `parent` and `children` as
  public fields, so the graph is walkable by hand, but `traverse`,
  `getObjectByName`, `name` and `userData` have no counterpart. Minor, and hit
  constantly by ports.
- **BatchedMesh** (multi-draw). Our record meshes and instance streams cover
  most of it.

## Animation

The evaluator is not the gap: clip sampling and blending run in core
([animation-core](../done/animation-core.md)), which is strictly better than a
JS mixer. Authoring and generality are:

- Clips are glTF-only. There is no way to author a `KeyframeTrack` in code, and
  `createMixer(model, options)` is model-bound, where Three's mixer animates
  any object property (a light's intensity, a material uniform, a camera field
  of view).
- **Additive blending** (`AnimationUtils.makeClipAdditive`), subclip and trim
  (`AnimationUtils.subclip`), and per-action weights beyond the crossfade we
  have.
- **IK** (`CCDIKSolver`).

## Loaders

We have the glTF/GLB subset plus the `.srtm` bake. Missing: Draco, meshopt,
KTX2 (all in [3d-model-loader](../backlog/3d-model-loader.md) and
[gpu-compressed-textures](../backlog/gpu-compressed-textures.md)), EXR (in
[3d-environment-additive](../backlog/3d-environment-additive.md)), and Three's
OBJ/FBX/STL/PLY/USDZ set.

The format zoo is a defensible non-goal given the build-time bake, but "I have
an FBX" is the most common first-five-minutes blocker for someone arriving from
Three, and today the answer is a tool we do not name.

## Textures

- **2D array textures and `Data3DTexture`.** Terrain splatting, LUT color
  grading, sprite sheets as layers. Nothing in alloy, flux or core binds
  `TEXTURE_2D_ARRAY`, and Three has both on WebGL2 as well as WebGPU.
- **Compressed formats.** Tracked,
  [gpu-compressed-textures](../backlog/gpu-compressed-textures.md).

## Controls

`OrbitControls` and `FirstPersonControls` are covered by `createOrbitCamera`
and `createFirstPersonCamera`. Missing: **TransformControls**, the
translate/rotate/scale gizmo, which is the first thing anyone building an
editor or level tool needs; plus DragControls, TrackballControls and
ArcballControls.

## Math

Ours is functions with out-params against Three's classes, which is the right
call for our allocation profile, so most of the difference is restyling rather
than absence, and the vector and quaternion set ships on the `@solidrt/3d/math`
subpath. Genuinely missing: a `Color` type (we have `srgbToLinear`/
`linearColor` conversions only), `MathUtils` (`lerp`, `clamp`, `damp`,
`smoothstep`, `randFloat` - everyone reaches for these and every app redefines
them), the `Curve` family above, and `Spherical`.

## Not gaps

Worth recording so they are not re-raised as findings:

- `computeTangents`/`computeMikkTSpaceTangents`. Our normal mapping derives the
  tangent frame in the fragment stage, so any UV-mapped geometry works with no
  tangent channel (roadmap item 21 records that this is why item 10's tangent
  layout was not needed).
- `interleaveAttributes`, `deinterleaveGeometry`, `estimateBytesUsed`. We are
  packed and interleaved by construction.
- `toTrianglesDrawMode`. Strips are a native topology.
- `center`, `normalizeNormals`, `computeBoundingSphere`. `center` is one
  `transformGeometry` over `geometryBounds` (the recipe is in
  `packages/3d/AGENTS.md`), computed and transformed normals are unit already,
  and Unity and Godot stop at the box, which every query and the LOD read
  here. Three names for one-liners.
- `Raycaster` as an object. `scene.raycast`/`pick` run in Rust over the
  retained index instead, which is strictly better than a per-frame JS walk.
- Multi-backend abstraction and shader node graphs. Declared non-goals in the
  roadmap, and the axis Three's own current investment (WebGPURenderer + TSL)
  runs along, so the distance here grows on purpose.

## The other direction

Holding the list next to what we have that Three does not, so the inventory is
not read as a verdict:

- Spatial queries (`raycast`/`overlap`/`sweep`/`moveAndSlide`) run in Rust over
  a retained index rather than as a JS walk, and `moveAndSlide` means a
  character controller exists at all.
- Frustum culling and LOD selection are core-side and free for a still camera,
  where Three re-tests every object every frame.
- Camera motion is one `setTargetParams` for the whole target, not a per-object
  matrix upload (roadmap item 1). This was the one place the O(delta) advantage
  did not apply, and it closed.
- Skinning palettes and morph weights compose in core through the
  `TextureSlot` sink, so the per-vertex and per-frame work never enters the
  interpreter.
- Motion is a first-class lane shared with `@solidrt/2d`: `setTransition`, exit
  transitions, staggered groups, the `weights` lane. Three has a clip mixer and
  nothing declarative.
- One flush writes every view's target; UI is live content in the same tree
  rather than a DOM overlay; the asset pipeline is build-time; the frame clock
  is ours, so pixel-exact 3D regression tests are possible.

Physics ([physics-core](../backlog/physics-core.md)) and Gaussian splats
([gaussian-splats](../backlog/gaussian-splats.md)) are content classes Three has
no answer for at all, and both are core work rather than wasm-bridge work here.

## If this becomes work

Not a decision, a suggested order for turning the untracked entries into
backlog files, by how many ports each one unblocks:

1. Stock-material extension hooks (the `onBeforeCompile` slot).
2. Clipping planes.
3. A stock points material (size and attenuation over the working
   `gl_PointSize`).
4. Shape holes and a curve sampler.
5. Graph traversal helpers (`traverse`, `getObjectByName`, `name`, `userData`).
6. The line-width question (`Line2`-style quad expansion).
7. A `packGeometry` form over planar position, uv and index arrays, the
   shape a port holds them in (the normals half of that friction is gone).

Each carries the standing shape gate: the Three/Godot/Unity comparison in the
proposal, per `packages/3d/CLAUDE.md`.
