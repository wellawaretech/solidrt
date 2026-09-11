---
title: Three.js feature survey - the inventory behind the roadmap
description: A fine-grained inventory of what Three.js has and @solidrt/3d does not, taken 2026-09-08 across geometry, materials, renderer features, objects, animation, loaders, textures, controls and math, marking each entry as already tracked or untracked, plus the items our model makes unnecessary.
created: 2026-09-08
---

# Three.js feature survey

A feature-by-feature inventory taken 2026-09-08, prompted by
`wireframeGeometry`/`edgesGeometry` landing and the question of what else
of Three's surface is absent.

This note does NOT rank and does not set direction.
[3d-roadmap](3d-roadmap.md) owns the destination and the order, and its
"Not in scope" section owns the deliberate refusals; several entries below
are already roadmap items or backlog files and are linked as such. What
this note adds is granularity: the roadmap thinks in capabilities, and a
capability-sized entry hides which specific call a porter reaches for and
does not find. Entries with no link are untracked anywhere as of the
survey date.

The bar throughout is practical parity, the roadmap's definition: the
common needs of an app covered without writing GLSL, not feature-count
parity with a decade of accumulation.

## Geometry helpers

`wireframeGeometry` and `edgesGeometry` are the first two of a family
Three ships as `LineSegments` subclasses. With `topology: "lines"` on
`Geometry` all of them are pure builders for us:

| Three | us |
| --- | --- |
| `GridHelper`, `PolarGridHelper` | `gridHelper`; polar missing |
| `AxesHelper` | `axesHelper` |
| `BoxHelper`, `Box3Helper` | `box3Helper(bounds)`; the live subtree-bounds half missing |
| `ArrowHelper` | `arrowHelper`, a pyramid-outline head (one lines geometry) |
| `PlaneHelper` | `planeHelper`, no fill quad |
| `CameraHelper` (frustum lines) | missing |
| `Directional/Point/Spot/HemisphereLightHelper` | missing |
| `SkeletonHelper` | missing, though skeletons and `bindSkeleton` exist |
| `VertexNormalsHelper` | missing |

This is the largest single cluster and the one a porter hits first,
because it is what you reach for while debugging the port itself.

The shape decision (settled 2026-09-10, okf/done/3d-debug-helper-builders.md):
Three makes these scene objects, while Godot and Unity keep the
equivalents in the editor rather than the runtime API. Splitting the
difference matches our layering: the static ones (grid, axes, box,
plane, arrow) are geometry builders next to `wireframeGeometry`, drawn by
`unlit` (which took `vertexColors` for the two colored ones, so there is
no line material), while the live ones (camera, light, skeleton gizmos)
have to follow a node every frame and would be components, which is a
much larger commitment for a debug aid; those stay open.

Adjacent: neither Three's native lines nor ours have line width (1px GL
lines, the ES core guarantee). Three answers with the `Line2`/
`LineMaterial` addon, quad-expanded lines in the vertex stage. If
wireframe overlays are meant to be legible on a HiDPI display this is a
real follow-up, not a nicety.

## Geometry operations

- `computeVertexNormals`. We cannot recompute normals at all. Needed
  after `mergeGeometries`, after any deformation, and for any
  hand-authored vertex array. The most glaring gap after the helpers.
- `toNonIndexed`. Flat shading and per-face attributes have no path.
- `mergeVertices` (weld/dedupe). We weld by position inside
  `edgesGeometry` and nowhere else; not exposed.
- `toCreasedNormals` (smoothing-angle normals). The `smooth` flag on
  profile points covers the sweep builders only.
- `center`, `computeBoundingSphere`, `normalizeNormals`. Small; `center`
  is two lines over `geometryBounds` and `transformGeometry`.
- `setDrawRange`. Draw a slice of an index buffer: progressive reveal,
  growing trails.

## Primitives

- `CapsuleGeometry`. `Capsule` already exists as a collision volume in
  `scene.ts` and nothing can draw one, which is an odd asymmetry.
  Roadmap item 10 already carries it as a tube special case.
- The polyhedron family: `Tetrahedron`, `Octahedron`, `Icosahedron`,
  `Dodecahedron` and the generic `PolyhedronGeometry` with `detail`
  subdivision. Gets you an icosphere, which is the better sphere for
  most shading (uniform triangles, no pole pinch) and which our
  lat/long `sphere` cannot express. Landed 2026-09-11 as `tetrahedron`,
  `octahedron`, `icosahedron`, `dodecahedron` and `polyhedron(vertices,
  indices, options)` (okf/done/3d-polyhedron-primitives.md): Three's
  non-indexed form and tables verbatim, radius 0.5 to match `sphere()`,
  which stays the textured default.

## Curves and shapes

Two gaps that feed builders we already have:

- Three's `Curve`/`CurvePath` family (`CatmullRomCurve3`, cubic and
  quadratic Bezier, arcs, `getSpacedPoints`) is what `TubeGeometry` and
  `ExtrudeGeometry` consume. Our `SweepPath` is a polyline with `smooth`
  flags, so anyone wanting a spline samples it themselves. A curve
  sampler in front of `tube`/`sweep`/`pathFrames` is the missing piece.
- `Shape.holes`. Our `triangulate` is ear-clip over a single simple
  contour, documented as no holes, so `shape()` and `extrude()` cannot
  produce a washer, a letter O, or a panel with a cutout. Ear-clipping
  with hole bridging is the standard fix.

## Multi-material groups

`BufferGeometry.groups` (index ranges each drawn with its own material)
has no counterpart: one mesh is one geometry and one material, so a port
carrying a grouped geometry splits it into N meshes by hand. Worth
deciding deliberately rather than by omission. Splitting is arguably the
cleaner model for our one-entry-per-mesh draw path, in which case
`mergeGeometries` should grow the documented inverse rather than leaving
callers to slice index arrays themselves. Related but not the same:
merge-by-material in the model loader
([3d-model-loader](../backlog/3d-model-loader.md)).

## Materials

| Three | us |
| --- | --- |
| MeshBasic / Lambert / Phong / Standard | `unlit` / `lit` / `standard` |
| MeshPhysical (clearcoat, sheen, transmission, iridescence, anisotropy) | missing |
| MeshToon | missing |
| MeshMatcap | missing |
| MeshNormal / MeshDepth | missing; hand-rollable through `overrideMaterial` |
| ShadowMaterial (shadow catcher on invisible ground) | missing |
| PointsMaterial | missing, and see point size below |
| LineDashedMaterial | missing |
| SpriteMaterial | `sprite` |

Two items that matter more than the class list:

- **No `onBeforeCompile` equivalent.** Nothing injects into a stock
  material's shader. Wind sway on a `standard` mesh today means
  rewriting PBR inside `shaderMaterial`. Three has `onBeforeCompile`
  (and now TSL), Godot has the `spatial` shader with vertex/fragment
  hooks over the built-in lighting, Unity has Shader Graph over the
  standard lit. All three converge on "extend the stock material", we
  have only "replace it". Note the tension with the roadmap's non-goal:
  shader node graphs are out of scope, but a hook is not a graph.
- **Missing knobs on the stock materials.** `depth`, `depthWrite` and
  `blend` exist only on `shaderMaterial`, so an additive glow or a
  decal drops to a custom shader (the `unlit` half is already a
  tiny.md line). `polygonOffset` is absent from the whole stack: GL
  `POLYGON_OFFSET_FILL` appears in `alloy/src/gl/pass.rs` purely as
  Impeller state save/restore, never as a pipeline option, so z-fighting
  on coplanar geometry has no fix. Also `alphaMap`, `bumpMap`,
  `displacementMap`, `envMapIntensity`, `alphaHash` dithered
  transparency, and `aoMap` (tracked in
  [3d-environment-additive](../backlog/3d-environment-additive.md)).

## Renderer features

- **Clipping planes** (`material.clippingPlanes`, and the renderer-global
  set). Nothing. Section views, CAD-style slicing, sliced reveals.
- **Logarithmic depth buffer.** Planetary and other large-scale scenes.
- **VSM shadows.** We do PCF with a `radius`; Three offers VSM as an
  alternative shadow filter.
- **Post effects.** Bloom, SSAO/GTAO, FXAA/SMAA/TAA, DOF, motion blur,
  outline (the selection highlight every editor tool needs), god rays.
  Read carefully against the roadmap: a post-processing COMPOSER is a
  declared non-goal there (the window shader and `<texture blendMode>`
  cover that tier), so this entry is not an argument for `EffectComposer`.
  It is the observation that the individual effects have no home either,
  and that half of them are blocked on the same thing regardless of where
  they live: we tone map and sRGB-encode in every fragment, so nothing
  downstream can see radiance
  ([3d-hdr-scene-buffer](../backlog/3d-hdr-scene-buffer.md), roadmap
  item 17).
- **Draw and triangle introspection** (`renderer.info`): tracked as
  [3d-scene-draw-introspection](../backlog/3d-scene-draw-introspection.md).
- **WebXR.** Absent entirely. A whole category that deserves an explicit
  yes or no rather than silence.

## Objects and scene graph

- **Morph targets.** Nothing; the glTF loader drops them. Facial
  animation and blend shapes. Roadmap item 16 and
  [3d-model-loader](../backlog/3d-model-loader.md).
- **Points and particles.** `topology: "points"` exists, but
  `gl_PointSize` appears nowhere in the stack, so points draw as 1px
  dots. Three does not ship a particle system either, but it ships the
  primitive; we do not.
- **Level of detail.** Tracked, [3d-lod](../backlog/3d-lod.md), roadmap
  item 22.
- **Decals** (`DecalGeometry`). Bullet holes, tyre marks. Missing, and
  blocked by `polygonOffset` above.
- **Reflector / Refractor / Water.** A planar mirror is cheap for us (a
  view with a mirrored camera, which the view machinery already
  supports) and does not exist.
- **3D text.** Three has `FontLoader` plus `TextGeometry`, and
  `CSS2DRenderer`/`CSS3DRenderer` for labels. Our structural answer is
  better in principle (real UI elements in the same tree, roadmap item
  3) but there is no world-space text today in either form; only 2d
  tracks it, in
  [2d-world-space-text](../backlog/2d-world-space-text.md).
- **Graph traversal helpers.** `traverse`, `getObjectByName`,
  `userData`. Minor, and hit constantly by ports.
- **BatchedMesh** (multi-draw). Our record meshes cover most of it.

## Animation

- Clips are glTF-only. There is no way to author a `KeyframeTrack` in
  code, and the mixer is model-bound, where Three's animates any object
  property (a light's intensity, a material uniform, a camera field of
  view).
- **Additive blending** (`AnimationUtils.makeClipAdditive`), subclip and
  trim (`AnimationUtils.subclip`), and per-action weights beyond the
  crossfade we have.
- **IK** (`CCDIKSolver`).

## Loaders

We have the glTF/GLB subset plus the `.srtm` bake. Missing: Draco,
meshopt, KTX2 (all in
[3d-model-loader](../backlog/3d-model-loader.md) and
[gpu-compressed-textures](../backlog/gpu-compressed-textures.md)), EXR
(in [3d-environment-additive](../backlog/3d-environment-additive.md)),
and Three's OBJ/FBX/STL/PLY/USDZ set.

The format zoo is a defensible non-goal given the build-time bake, but
"I have an FBX" is the most common first-five-minutes blocker for
someone arriving from Three, and today the answer is a tool we do not
name.

## Textures

- **2D array textures and `Data3DTexture`.** Terrain splatting, LUT
  color grading, sprite sheets as layers. Nothing in alloy, flux or core
  binds `TEXTURE_2D_ARRAY`.
- **Compressed formats.** Tracked,
  [gpu-compressed-textures](../backlog/gpu-compressed-textures.md).

## Controls

`OrbitControls` and `FirstPersonControls` are covered by
`createOrbitCamera` and `createFirstPersonCamera`. Missing:
**TransformControls**, the translate/rotate/scale gizmo, which is the
first thing anyone building an editor or level tool needs; plus
DragControls, TrackballControls and ArcballControls.

## Math

Ours is functions with out-params against Three's classes, which is the
right call for our allocation profile, so most of the difference is
restyling rather than absence. Genuinely missing: a `Color` type (we
have `srgbToLinear`/`linearColor` conversions only), `MathUtils` (`lerp`,
`clamp`, `damp`, `smoothstep`, `randFloat` - everyone reaches for these
and every app redefines them), the `Curve` family above, and
`Spherical`.

## Not gaps

Worth recording so they are not re-raised as findings:

- `computeTangents`/`computeMikkTSpaceTangents`. Our normal mapping
  derives the tangent frame in the fragment stage, so any UV-mapped
  geometry works with no tangent channel (roadmap item 21 records that
  this is why item 10's tangent layout was not needed).
- `interleaveAttributes`, `deinterleaveGeometry`, `estimateBytesUsed`.
  We are packed and interleaved by construction.
- `toTrianglesDrawMode`. Strips are a native topology.
- `Raycaster` as an object. `scene.raycast`/`pick` run in Rust over the
  retained index instead, which is strictly better than a per-frame JS
  walk.
- Multi-backend abstraction and shader node graphs. Declared non-goals
  in the roadmap.

## The other direction

Holding the list next to what we have that Three does not, so the
inventory is not read as a verdict: spatial queries
(`raycast`/`overlap`/`sweep`/`moveAndSlide`) run in Rust over a retained
index rather than as a JS walk; frustum culling is core-side and free
for a still camera, where Three re-tests every object every frame; one
flush writes every view's target; UI is live content in the same tree
rather than a DOM overlay; the asset pipeline is build-time. Physics
([physics-core](../backlog/physics-core.md)) and Gaussian splats
([gaussian-splats](../backlog/gaussian-splats.md)) are content classes
Three has no answer for at all.

## If this becomes work

Not a decision, a suggested order for turning the untracked entries into
backlog files, by how many ports each one unblocks:

1. Stock-material extension hooks (the `onBeforeCompile` slot).
2. The line-width question (the debug helper builders landed 2026-09-10,
   with no line material needed: `unlit` draws lines).
3. `computeVertexNormals` and `toNonIndexed`.
4. Clipping planes.
5. Point size.
6. Shape holes and a curve sampler.

Each carries the standing shape gate: the Three/Godot/Unity comparison
in the proposal, per `packages/3d/CLAUDE.md`.
