---
title: A 3D scene graph above the pipeline
description: How a Three.js-in-spirit retained scene graph (meshes, materials, cameras, lights) would be built over flux:gpu as a sibling library with a Solid component face. The structural gap it named (ordered draw lists) landed 2026-08-04 with index buffers, cull and per-instance attributes, so library v1 is unblocked; remaining tiers tracked in the backlog.
created: 2026-08-03
---

# A 3D scene graph above the pipeline

The question: how would solidrt grow something similar in spirit to
Three.js - an app says "a red box at this transform, seen by this camera"
instead of owning vertex buffers and matrices. The gap is already named in
../notes/gpu-review.md ("No 3D scaffolding above the pipeline: no math
library, model loading, culling, or scene graph... every game rebuilds the
layer; Doom's mesh.ts is 551 lines"). Assessed 2026-08-03 from a full
audit of the flux:gpu surface, the alloy GL implementation, and the
example corpus.

## Recommendation

Feasible and worth staging, with one honest blocker.

1. **A new sibling package, two layers inside it.** An imperative retained
   core (math, scene tree, materials, a renderer that compiles the scene
   to flux:gpu calls) with a Solid component face on top (`<Scene>`,
   `<Mesh>`, `<PerspectiveCamera>`, ... over context). Peer-dep on
   `@solidrt/core`, source-only, zero engine coupling beyond `flux:gpu` -
   exactly the `packages/components` shape, and consistent with the
   layering rule in [declarative-gpu](declarative-gpu.md) (the primitive
   stays first-class; sugar lives above core). The imperative layer is
   usable without the components, per the primitive-first rule.
2. **One engine feature was structural: several ordered draws into one
   target.** A scene frame is N draws (one per mesh/material) sharing one
   depth buffer, with per-draw uniforms (the model matrix). When this
   note was written a target owned exactly one pipeline, one buffer, one
   draw range - and every workaround failed in a different way (analysis
   below, kept as the rationale record). It landed 2026-08-04 as
   ../backlog/gpu-draw-list.md in the retained shape recommended here,
   alongside index buffers, cull mode and per-instance attributes; see
   the status section for what remains per stage.
3. **The declarative-gpu conclusions carry over unchanged.** Components
   sync a retained scene model; they are not passes (the pass-as-component
   non-goal is upheld - this is a model *above* the GPU surface, not a
   declarativization *of* it). And the react-three-fiber rule is load
   bearing here too: anything moving at frame rate bypasses the reactive
   layer (direct object mutation + invalidate from `onFrame`; signals
   carry structure and slow state).

## Status 2026-08-04

The tier-1 engine work closed one day after this note was written,
headless-verified and live-verified (Linux; index+cull also on the
Android TV; Windows/ANGLE pending):

- **Ordered draw lists** (../backlog/gpu-draw-list.md): createDrawTarget
  + addDraw/removeDraw with stable DrawIds, per-entry
  setDrawParams/setDrawTextures/setDrawRange, ordering via `before` and
  setDrawOrder. Retained list legal on auto targets per this note's
  purity correction, one clear + N entries = one pass. One refinement
  over the sketch below: depth STORAGE is target-owned
  (createDrawTarget `depth: true`), depth BEHAVIOR stays pipeline state
  - WebGPU's split exactly.
- **Index buffers**: per-entry indexBuffer + indexFormat, WebGPU's
  firstIndex/indexCount spelling.
- **Cull mode**: `cull: "none"|"back"|"front"` on the pipeline, with the
  framebuffer-space winding rule (front = counter-clockwise as
  displayed).
- **Per-instance attributes**: instanceAttributes on the pipeline +
  instanceBuffer per entry, instanceCount derived one-per-record.

So library v1 (staging step 2) has zero engine prerequisites left.
Remaining engine items, by the stage that first needs them: mipmaps
(textured models), uniform arrays (lights;
../backlog/gpu-uniform-arrays.md, filed from this note), blend factors
plus the premultiplied question (transparency), MSAA (visual quality),
sampleable depth and float textures (shadows, skinning), cube maps
(environment; ../backlog/gpu-cube-maps.md, filed from this note). Depth
func stays fixed at LESS - deliberate, and the right v1 default anyway.

## Scope: what "in spirit" means

Take from Three.js: the object model (scene tree with transforms, Mesh =
geometry + material, cameras, lights), a small math library, loaders (a
glTF subset, eventually), raycast picking, and the ergonomic promise that
common 3D needs no GLSL. Leave behind: multi-backend abstraction (GLSL ES
3.00 is a settled bet), shader node graphs / TSL (rejected in
declarative-gpu for the same reason), the WebGL global-state renderer (the
pipeline object model here is already better), and a post-processing
composer (the window shader and `<texture blendMode>` tree compositing
already cover that tier).

## The structural problem: one draw per target

What existed at writing (audited 2026-08-03; the status section above
records the 2026-08-04 landings): user vertex+fragment GLSL, one
interleaved float32 vertex buffer per target, name-resolved attributes,
five topologies, typed uniforms up to mat4 (column-major), instancing via
`instanceCount` + `gl_InstanceID`, optional private DEPTH_COMPONENT24
renderbuffer (func hardcoded LESS), blend "none"|"add", `render:
"manual"` + `renderTarget` + `loadOp` + `copyTexture`, labels, limits,
call-site validation. Absent, verified at the GL layer: index buffers
(`glDrawArrays` only), cull control (CULL_FACE force-disabled per pass),
alpha blending, mipmaps, per-instance attributes (no divisor), depth
textures, cube/3D/array textures, MRT, float targets, MSAA on app
targets, uniform arrays (reflect as unsupported).

A generic scene could not be hosted on that surface, and it is worth
keeping the record of why each workaround fails:

- **Tree compositing** (one target per mesh, stacked `<texture>`
  elements): separate depth buffers, so no cross-mesh occlusion. Only
  correct for convex silhouettes, the trick the second-reality field
  report already exhausted.
- **Instancing**: one geometry and one material per target, and without
  per-instance attributes or mat4 uniform arrays, per-object transforms
  reduce to `gl_InstanceID` arithmetic. Grids, not scenes.
- **CPU batching** (concat everything into one buffer, bake transforms
  per frame): the Doom port's shape, and it works there because a level
  is one material family and the world is mostly static. The documented
  QuickJS ceiling (no JIT, per-vertex work must stay on the GPU) rules it
  out as a general mechanism.
- **Stepping one manual target N times** (`renderTarget` per mesh with
  `loadOp: "load"`): load keeps color only - depth always clears per
  render - so occlusion breaks between steps. Also N full passes of
  overhead on hardware where pass *count* is the budget (the TV numbers
  in gpu-review).

So the missing primitive is: **one render = an ordered list of draws into
one framebuffer, each entry naming its pipeline, buffer, draw range, and
params, with depth cleared once at the top**. That is what every 3D API
calls a render pass.

Note a useful correction to the purity framing in gpu-review: an ordered
draw list *retained on the target* does not break the pure-target
invariant. "Render twice = render once" still holds - the order is
explicit data, an input like any other; only cross-render accumulation
breaks purity. So the feature is legal in `render: "auto"` targets too,
and a static scene keeps the demand-driven win (zero passes until an
input changes) with the dirty flush unchanged. The alternative shape - a
draw list as a `renderTarget(id, draws)` argument - fits the manual path
but forfeits that, and retains nothing for introspection. Recommended
shape: a retained list, e.g.

    createShaderTarget(pipeline, w, h, params, opts)   // today: the 1-draw case
    createDrawTarget(w, h, {
      depth: true, clearColor,
      draws: [{ pipeline, buffer?, first?, count?, instances?, params?, textures? }, ...],
    })
    setDraws(id, index, update)   // partial merge per entry, like setDraw today

with each entry honoring its pipeline's blend/depth state, and the
existing single-draw target becoming the degenerate case. The landed
design (../backlog/gpu-draw-list.md) kept this shape with two upgrades:
entries are addressed by stable DrawIds through per-entry verbs
(addDraw/removeDraw, setDrawParams/setDrawTextures/setDrawRange,
`before` + setDrawOrder) rather than list indices, and depth storage
moved onto the target while depth behavior stayed pipeline state.
Per-draw params live on the entry, as required here, and single-draw
targets did become fixed one-entry lists internally.

## Engine gaps ranked for this workload

Statuses refreshed 2026-08-04; the status section records the landings.

Tier 1 - v1 blockers: **all closed 2026-08-04**. Ordered draw lists
(../backlog/gpu-draw-list.md) and cull mode landed; depth func stays
fixed at LESS deliberately (the right v1 default, additive when a
demand signal arrives). Uniform arrays turned out not to gate v1 - they
gate lights - so they moved to tier 2 and are filed.

Tier 2 - real models, lights, visual quality:

- **Index buffers**: DONE 2026-08-04 (per-entry
  `indexBuffer`/`indexFormat`, firstIndex/indexCount spelling) - the
  glTF entry ticket is paid.
- **Per-instance attributes**: DONE 2026-08-04 (instanceAttributes +
  instanceBuffer, derived instanceCount).
- **Mipmaps** (../backlog/gpu-mipmaps.md): textured materials alias
  immediately at minification. Still deferred.
- **Uniform arrays** (../backlog/gpu-uniform-arrays.md, filed
  2026-08-04 from this note): light lists without baking a count into
  shader source.
- **Alpha translucency**: the recorded blocker was "sorting plus
  premultiplied-vs-straight" - and the scene graph *is* the sorter (it
  owns draw order: opaque front-to-back, transparent back-to-front). So
  the blocker inverts: once the library exists the engine only needs the
  blend factor vocabulary (../backlog/gpu-pipeline-blend-modes.md).
- **MSAA on pipeline targets** (../done/gpu-target-antialiasing.md):
  silhouette jaggies are the dominant artifact on filled geometry.

Tier 3 - later features: **sampleable depth** (extensions file; shadow
maps, SSAO), **float texture formats** (extensions file; skinning
matrices, HDR), **cube maps** (../backlog/gpu-cube-maps.md, filed
2026-08-04 from this note; environment/reflection, cube shadow maps),
and the sRGB/linear question the pixel contract currently answers with
"non-linear RGBA8 everywhere" - PBR lighting math would force it. Outside
the GPU stack, first-person controls stay blocked on
../backlog/relative-mouse-input.md, not on any of this.

## What stays in the library (no engine involvement)

- **Math**: Vec3/Quat/Mat4 over Float32Array, column-major to match the
  mat4 uniform contract, no per-frame allocation. The projection bakes in
  the y-down clip flip so users never see it (the gpu-pipeline.tsx gotcha
  becomes a library guarantee).
- **Scene tree**: plain objects with parent/child links, local TRS,
  cached world matrices with dirty propagation. Not signals - the
  reactive-webgpu lesson (reactivity is contagious and wrong at this
  granularity) and the QuickJS ceiling both say the hot path is flat
  imperative code; signals appear only at the component boundary.
- **Materials as pipeline factories**: a material = shader pair + state,
  deduped through the raw split layer (compileShader/linkProgram shared
  per material class, createRenderPipeline per state variant, labels from
  material names). v1 ships a fixed set (unlit color, unlit textured,
  vertex color; lambert/phong once lights land) rather than an uber
  shader or user shader graphs - custom materials take user GLSL
  directly, which the raw layer already makes first-class.
- **Draw-list assembly**: walk visible meshes, sort (state-change-major
  for opaque, depth-major for transparent), emit the retained draw list,
  update only entries whose inputs moved.
- **Picking**: raycast against bounding volumes, Three-style, driven from
  the `<texture>` element's pointer events - localX/localY arrive with
  ancestor transforms already undone, so unproject is straightforward.
- **Loaders**: a glTF subset (positions/normals/uvs/indices + baseColor)
  once index buffers exist. Pure JS, demand-gated.
- Frustum culling, later; scenes below a few thousand nodes will not
  need it to hit budget.

## The Solid face

PascalCase components communicating over context - no new intrinsic
elements, no rendertree or renderer changes. Props follow the Solid 2.0
model (reactive values, no destructuring); effects write into the
retained scene objects and call `invalidate()`; onCleanup detaches.
`<Scene>` owns the target and renders as an ordinary `<texture>` leaf, so
the output composes with layout, transforms, blendMode, and pointer
events like any other element. Demand-driven end to end: a static scene
costs zero passes; `invalidate()` marks dirty and the next frame renders
once - which matters on the TV class where pass count is the budget.
Frame-rate motion uses `onFrame` + direct mutation, per the r3f rule the
trails example independently converged on.

    <Scene width={400} height={400} clearColor={[0.08, 0.08, 0.12, 1]}>
      <PerspectiveCamera fov={60} position={[0, 1.5, 4]} lookAt={[0, 0, 0]} />
      <Group rotateY={spin()}>
        <Mesh geometry={box()} material={unlit({ color: "#e33" })} position={[-1, 0, 0]} />
        <Mesh geometry={sphere(24)} material={unlit({ map: tex() })} position={[1, 0, 0]} />
      </Group>
    </Scene>

## Scale and the interpreter

The obvious objection: QuickJS has no JIT, and scene graphs are CPU
machinery. The answer is that the retained draw list exists precisely
because of the interpreter, and it inverts the cost model the browser
libraries were built for. A WebGL scene graph re-issues every draw from
JS every frame - the JS engine sits in the hot loop for the whole scene,
every frame, which is why it needs a JIT. Here the per-frame walk (bind
pipeline, bind buffer, draw, repeat) runs in Rust on the raster thread;
JS pays for what changed, not for what exists. A static scene costs zero
JS and zero passes; one spinning group costs one matrix chain update and
one params write.

What remains in JS is object-count work, never vertex-count work: world
matrix updates for animated nodes, the occasional transparent-geometry
sort when the camera moves, and the FFI writes (a 16-float params entry
per moved mesh). Vertices transform in the vertex shader - the
documented "per-vertex work stays on the GPU" rule. The calibration
point is the Doom port at jsMs ~0.3 for a real game frame of geometry
bookkeeping in lean typed-array JS. Interpreted numeric code runs
roughly 10-30x behind a JIT, and browser scene graphs on a JIT handle
tens of thousands of CPU-animated objects - divide through and the
comfortable ceiling here is several hundred to ~1k independently
animating objects, far more when most of the scene is static or
instanced (one draw entry and one params write cover N instances).

What the interpreter genuinely rules out: per-vertex JS of any kind -
skinning and morph targets want bone data on the GPU (float textures,
tier 3), CPU particle systems want instancing plus vertex-shader math
(which already works today). Physics and large AI populations hit the
same platform-wide ceiling gpu-review already records, independent of
this library. And 10k+ dynamic objects are out of reach - which is also
not this platform's app class.

It also dictates the hot-path style already stated above: plain objects
with dirty flags, signals only at the component boundary (per-prop
effects would multiply interpreter overhead per node), zero per-frame
allocation in the math layer, coarse whole-props sync first. flux:wasm
is not a shortcut - wasmi is an interpreter too, a small constant factor
at best. If a real consumer outgrows the JS side, the escape hatch is
moving the scene walk into the engine; the draw-list design is
deliberately the shape that makes that migration possible without
changing the app-facing API. That stays an explicit non-goal for now -
it would be a new alloy concern and nothing demands it.

## Staging

1. **Engine: ordered draw lists** (+ cull). DONE 2026-08-04 - see the
   status section. Index buffers and per-instance attributes arrived
   with it, pulling the engine half of stages 3 and 4 forward.
2. **Library v1** (the current step): new package with math, scene tree,
   unlit materials, PerspectiveCamera, the draw-list renderer, the Solid
   components, and one gallery example (spinning primitives with
   occlusion). Bare minimum, complete of its kind. Zero engine
   prerequisites left.
3. **Real models**: the glTF subset loader (library) and mipmaps
   (engine; the one item still missing at this stage).
4. **Light and depth of field of use**: lambert/phong lights (needs
   ../backlog/gpu-uniform-arrays.md), transparency (blend factors +
   library sorting), instanced meshes (engine half landed; library
   sugar only), MSAA.
5. **Later, each demand-gated**: shadow maps (sampleable depth),
   environment maps (cube maps, filed), PBR + the color-space decision.

## Open questions

- Package name and altitude: `@solidrt/scene` vs something 3D-explicit;
  whether the math module is its own subpath export (apps will want it
  without the scene graph).
- Light model before uniform arrays exist: a fixed handful of scalar
  uniforms would work but bakes a cap into shader source; wait for
  ../backlog/gpu-uniform-arrays.md rather than ship the workaround. (The
  draw-list API question this list used to carry is resolved -
  ../backlog/gpu-draw-list.md decided it.)
- Component granularity: one effect syncing whole props per node vs
  per-prop effects; start coarse, measure.
- Whether picking ships in v1 or with the first interactive consumer.
- Whether `<Scene>` sizes from layout (onLayout on the texture element)
  or from explicit props; explicit first, layout-driven when asked.

Cross-references: [declarative-gpu](declarative-gpu.md) (the layer below,
and the non-goals this note upholds), ../notes/gpu-review.md (the
capability audit and workload tiers this note extends),
../backlog/gpu-draw-list.md (the landed draw-list design), and
../backlog/gpu-pipeline-extensions.md (the landed-extensions record; its
remaining opens are split into their own backlog items, linked from it).
