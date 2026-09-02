---
title: 3D roadmap - toward Three.js parity
description: A ranked list of what @solidrt/3d still needs to be practically comparable to Three.js, ordered by structural leverage first and then by the research staging, each entry checked off when the capability is delivered.
created: 2026-08-06
---

# 3D roadmap

One place to rank `@solidrt/3d` against the obvious benchmark. This file owns
the **destination**, the **order**, and one checkbox per entry.

The checkbox is deliberate and its scope is narrow. An entry here is a
capability, which usually spans several okf items and library work that has no
okf item at all - so nothing else in the repo records whether that capability is
delivered. What this file must never do is mirror the state of the items it
links to: no `[done]`/`[open]` beside a link, because the directory that item
sits in already says it, and a second copy rots the way a status field rots.
Checking a box also means deleting the paragraph describing what was built -
that belongs in `packages/3d/AGENTS.md`. Keep only what still points forward: a
deliberate non-goal, or a sub-form left for later.

What already exists is documented where it is used, not here:
`packages/3d/AGENTS.md` (usage and traps) and `packages/3d/examples/`.
The design rationale lives in the research notes and stays there:
[scene-graph-3d](../notes/scene-graph-3d.md) (architecture and staging),
[3d-differentiators](../notes/3d-differentiators.md) (where the native model
can end up ahead, and the ranking philosophy this file inherits),
[declarative-gpu](../notes/declarative-gpu.md) (the layering rules).

"Parity" here is practical parity - the common 3D needs of an app covered
without writing GLSL - not feature-count parity with a decade of accumulation.
Per the differentiators note, items that compound a structural advantage rank
above items that merely close distance, which is why the top of the list is not
what a feature checklist would pick.

## The ranked list

Numbers are permanent ids: entries are never renumbered, so the cross-references
between them keep resolving. A checked box means the capability is delivered -
what it delivered is documented in `packages/3d/AGENTS.md`, not here.

1. [x] **Camera off the O(scene) path: per-entry uModel + target-shared
   uViewProj.** Engine:
   [gpu-shared-draw-params](../done/gpu-shared-draw-params.md). Its sampler
   half (`setTargetTextures`, shared target-level bindings) is the binding
   channel items 14 and 15 consume.
2. [x] **The standard uniform set and the exported-GLSL policy.** Engine:
   [gpu-shared-draw-params](../done/gpu-shared-draw-params.md).
   `@solidrt/3d/glsl` is the shared source item 5's lit material classes are
   also built from, so custom materials never become second-class. The
   app-writable channel and the camera basis followed in
   [3d-material-uniform-plumbing](../done/3d-material-uniform-plumbing.md).
3. [x] **UI as live 3D content.** Engine:
   [snapshot-boundary-texture-id](../done/snapshot-boundary-texture-id.md)
   (landed 2026-08-23: `snapshotTexture(ref)`). Routing pointer events back
   through a mesh into the mapped subtree stays with item 4.
4. [x] **Picking.** Delivered 2026-08-23 with stage 2 of
   [spatial-core](../backlog/spatial-core.md): index and triangle
   narrowphase in core, hits carry `face`/`uv`/`normal`. Routing events
   into a UI subtree mapped onto a mesh stays with item 3.
5. [x] **Lights and lit materials (lambert/phong).** Engine:
   [gpu-uniform-arrays](../done/gpu-uniform-arrays.md). Library landed
   2026-08-23 (`lit`, light NODES with transform inheritance, triplanar
   as an option - see `packages/3d/AGENTS.md`). Left for later, on
   demand: `emissive` (landed 2026-08-31 with the surface maps) and a
   fresnel rim option. Spot and point lights landed 2026-09-02: the
   typed light list (`uLightType`/`uLightPos`/`uLightParams`,
   core-driven position slots via the spatial core's Position
   projection), Three's windowed inverse falloff, `MAX_LIGHTS` raised
   to 8 with the shadow-slot budget decoupled (`MAX_SHADOW_MAPS` = 8,
   its own constant). The cap `MAX_LIGHTS`
   is an app-level tunable candidate in
   [app-runtime-config](../backlog/app-runtime-config.md).
6. [x] **Transparency.** Done 2026-08-17: engine `blend: "multiply"` and
   `"alpha"` ([gpu-pipeline-blend-modes](../backlog/gpu-pipeline-blend-modes.md)),
   library `transparent: true` materials + scene-owned back-to-front sort +
   `renderOrder` ([gpu-alpha-translucency](../done/gpu-alpha-translucency.md)).
   Premultiplied settled by the pixel contract.
7. [x] **Real models: a glTF subset loader.** Shipped 2026-08-26 as two
   layers, both in the package (the CLI gained only the generic `srt tool`
   runner): the runtime primitive `parseGltf` + `createModel`, and the bake
   `srt tool 3d/model` writing the same parse as a `.srtm` for `loadModel`.
   The bake exists because of a measurement, not a principle: 124 ms of
   interpreter time per 32k vertices against 40 ms for the whole baked load.
   Left, demand-gated, in [3d-model-loader](../backlog/3d-model-loader.md):
   Draco/meshopt and KTX2 decoding in the bake, merge-by-material,
   vertex colors, runtime-fetched content. KTX2 has an engine half the bake
   cannot supply on its own -
   [gpu-compressed-textures](../backlog/gpu-compressed-textures.md), the
   ETC2 upload path `createTexture` does not have.
8. [x] **Mipmaps.** Engine: [gpu-mipmaps](../done/gpu-mipmaps.md) (landed 2026-08-23: `mipmap: true` on texture creation).
   Textured models alias immediately at minification; the one engine item
   staging step 3 (real models) still needs.
9. [x] **MSAA on pipeline targets.** Engine (landed 2026-08-23, `samples` on the target):
   [gpu-target-antialiasing](../done/gpu-target-antialiasing.md).
   Silhouette jaggies are the dominant artifact on filled geometry.
10. [ ] **Geometry breadth and the vertex-layout ceiling.** Library. Still
    demand-gated: closed sweep loops (twist reconciliation around a loop),
    capsule (a tube special case now that sweep exists), and further named
    layouts (tangents, skin weights) when items 7 and 16 force them. The
    direction stays a small set of named layouts, not an open
    BufferGeometry-style model.
20. [x] **Geometry as data: transform, merge, public bounds.** Library:
    [3d-geometry-ops](../done/3d-geometry-ops.md) (shipped 2026-08-19). Sits here rather than
    at the end of the list because it ranks with the other geometry work and
    ids are permanent, not positional. The generators build geometry and
    nothing can move or combine it, so a static scene costs one node, one
    draw entry and one `uModel` write per part - the per-frame walk this
    whole ranking is organised around. Baking placement into vertices
    collapses a static scene to one mesh per material, which is structural
    leverage on the interpreter, not a micro-optimisation. Pure array math,
    no engine call. Carries the two already-written-but-unexported helpers
    (`geometryBounds`, `rayBoxDistance`) along with it.
11. [x] **Rotation: aiming and quaternions.** Library. Only on-demand sugar is
    left (`rotateOnAxis`-style wrappers are one-liners over `quatMultiply`;
    name each against Unity, glam and Godot alongside Three, per the
    `quatFromTo` rename). Two constraints that outlive the work: `lookAt` is a
    node MUTATOR, not a function returning a rotation - one that returned a
    rotation would be representation-coupled, so do not add one. And
    `setTransform(node, { matrix })` is deliberately NOT in this item: a raw
    local matrix bypasses `compose()`, so it needs either a lossy decompose or
    a second matrix-mode node path beside the position/rotation/scale
    dirty-flag model.
12. [x] **Instanced mesh sugar, and billboards.** The instancing half
    shipped 2026-08-19: `shaderMaterialClass({ instanceAttributes })` makes
    an instanced material, `createInstancedMesh(geometry, material,
    records, count?, { bounds? })` carries the record buffer (plus
    `setInstances` / `setInstanceCount` / `disposeInstances` and the
    `<InstancedMesh>` component), the visibility switch restores the record
    count instead of writing 1, and picking takes explicit population
    `bounds` (records are opaque, so without them the mesh simply has no
    leaf). One draw entry and one uModel covering N instances is the
    standing answer to the churn workloads (particles, projectiles,
    streamed scatter - a chunk-streamed forest at hundreds of instances
    per chunk was the demand evidence, previously only possible by
    shimming the entry swap by hand). The billboard half shipped
    2026-08-26 as Three's vocabulary, not drei's: `sprite()` material +
    `createSprite` / `<Sprite>`, the facing done in the vertex stage off
    the shared camera basis (`billboard: "full"` default, or `"fixed-y"` -
    yaw only, the upright tree/character sprite every engine but Three
    has). `transparent` defaults to true on sprites (Three's SpriteMaterial
    default). Deferred to demand: `rotation`/`center`, constant screen
    size (`sizeAttenuation: false`), and an instanced sprite fleet -
    `shaderMaterialClass({ instanceAttributes })` plus the AGENTS.md
    billboard recipe already covers the last.
13. [ ] **Camera and control breadth.** The orthographic camera landed
    (`ortho: { left, right, top, bottom }` on any camera, scene and view
    alike - see `packages/3d/AGENTS.md`). First-person controls were
    blocked on engine
    [relative-mouse-input](../done/relative-mouse-input.md) - pointer
    lock/relative motion - which landed, so they are library-only now.
    A chase/follow camera rig is app code over `setTransition`.
14. [ ] **Environment tier: skybox, reflection/environment maps.** Shaped
    2026-09-02 with the Three/Godot/Unity comparison in
    [3d-environment](../backlog/3d-environment.md) (scene-level, mip-chained
    cube map, equirect + six-face sources, rgba16f for HDR). Engine:
    [gpu-cube-maps](../done/gpu-cube-maps.md) landed the same day
    (`createCubeTexture` + `samplerCube`), so the tier is library-only
    from stage 1; render-to-face
    ([gpu-cube-render-targets](../backlog/gpu-cube-render-targets.md))
    and [rgba16f](../backlog/gpu-half-float-format.md) wait for stages 3
    and 4. The binding side is already paid: a shared target-level
    sampler (`setTargetTextures`) binds an environment map once per scene
    target.
15. [x] **Shadow maps.** Landed 2026-08-26 (uncommitted) through stage 3
    of [3d-shadow-maps](../done/3d-shadow-maps.md): `castShadow` on
    `DirectionalLight` and `Mesh`, `lit` receiving by default, the
    `SHADOW` GLSL; every directional light may cast since 2026-08-27
    (stage 4a, slot = light index); the multi-view shape below became
    `scene.createView` on the way (split-screen, minimaps,
    override-material passes). Comparison sampling landed 2026-09-02
    ([gpu-depth-compare-sampling](../done/gpu-depth-compare-sampling.md):
    uShadowAtlas is a sampler2DShadow, one hardware 2x2-PCF tap). The
    Spot casters landed 2026-09-02 (the same machinery with a
    perspective camera, one slot) and point casters with them
    ([3d-point-light-shadows](../done/3d-point-light-shadows.md): six
    face tiles in the atlas, dominant-axis select - no cube map); still
    demand-gated: cascades
    ([3d-shadow-cascades](../backlog/3d-shadow-cascades.md)). Instanced
    casters landed 2026-09-02
    ([3d-instanced-shadow-casters](../done/3d-instanced-shadow-casters.md):
    per-class `shadowVertex`; skinned casters cast their pose since
    2026-09-01, riding the float-texture palettes).
    Shaped 2026-08-26 as engine sampleable
    depth ([gpu-sampleable-depth](../done/gpu-sampleable-depth.md)),
    per-target draw sinks in the spatial core, then scene VIEWS, then the
    shadow itself; the depth-func option
    ([gpu-depth-func](../backlog/gpu-depth-func.md)) turned out not to be
    a dependency. The
    map itself binds through the shared target-level sampler channel.
    **Library prerequisite, and it is the bigger half: a `Scene` is
    hardwired to one camera and one target.** There is no way to render the
    same scene twice from a different viewpoint, so even with sampleable
    depth in hand an app cannot produce the depth pass. The pieces below it
    all exist (`createDrawTarget` with `depth: true`, a second camera,
    `setTargetParams`); what is missing is a scene-graph shape for "render
    this scene into that target from this camera". Settle that shape before
    the engine work, because the same constraint is what rules out
    split-screen, reflections, minimaps and portals - shadows are just the
    first consumer to hit it. Until then the achievable tier is a projected
    blob, which also wants item 6's blend factors to avoid dithering.
16. [ ] **Skinning and morph targets.** Skinning shipped 2026-08-31 with
    the model loader's skins and the JS mixer, and moved to float-texture
    palettes 2026-09-01 (`uBones` an rgba32f texture sized to the rig,
    texelFetched in the vertex stage - built on
    [gpu-float-texture-formats](../done/gpu-float-texture-formats.md) -
    so the MAX_JOINTS uniform-array cap is retired; see
    `packages/3d/AGENTS.md`). Palette composition moved into the spatial
    core 2026-09-02 (the `TextureSlot` sink: joints bound row by row with
    their inverse bind, palettes composed and uploaded at the flush,
    `updateSkins` deleted, identical skins deduped); what keeps the box
    open: morph targets, since per-vertex JS is ruled out by the
    interpreter. The crowd-scale evaluator is
    [animation-core](../done/animation-core.md), not this item
    (clip evaluator DELIVERED 2026-09-03; the crowd tier is open until an
    app pushes it).
17. [ ] **PBR and the color-space decision.** The furthest tier: physically
    based lighting math forces the sRGB/linear question the pixel contract
    currently answers with "non-linear RGBA8 everywhere".
18. [x] **Scene background.** Deferred within the item: the texture-id form (a
    reserved non-breaking union widening - decide fit semantics when a
    consumer arrives). The boundary with item 14 stands: a camera-linked
    background (skybox) is the cube-map case, this was the screen-space one.
    Note for the proving-ground demo: its ground FADES over the backdrop,
    which needs item 6's blend factors before the two layers can merge.
    Settled 2026-09-02 with the environment tier's stage 1
    ([3d-environment](../backlog/3d-environment.md)): the directional
    background is SANCTIONED. The vertex stage hands the fragment `vRay`
    (a world-space view ray rebuilt from the new shared `uInvViewProj`),
    the background may declare `uCamPos` and any `scene.setParams` name,
    and the docs say so; a skybox `{ cube, intensity?, rotation? }` is
    the object form of the same slot. The 2D image form stays reserved.
19. [ ] **Scene scale.** The walk itself goes to core as stage 1 of
    [spatial-core](../backlog/spatial-core.md) - the JS sync recurses the
    whole tree on every change, so one moved node is O(scene) today, and
    the known triggers (hundreds of non-instanceable moving nodes; the
    O(vertices) picking tier of item 4) are not hypothetical. Frustum
    culling follows as a small query over the same core index when a
    GPU-bound scene needs it; a JS form is ruled out, not deferred, because
    testing every node per frame is exactly the O(scene) loop the design
    avoids. Both land with no app-facing API change, the reason the draw
    list was shaped as it was.
21. [x] **Surface maps on `lit`: normal, emissive, specular, light maps,
    UV transform.** Library:
    [3d-surface-maps](../done/3d-surface-maps.md) (shipped 2026-08-31).
    Normal mapping is derivative-based, so item 10's tangent layout was
    NOT needed and returns only as a quality option if mirrored-UV seams
    show up in a real model; item 17's lighting model and color-space
    decision stay untouched.
22. [ ] **Level of detail.** Core, a sink beside item 19's culling: a
    LOD group's level is picked by distance from a reference node after
    the flush and drives the variants' visibility switches, so a
    thousand-tree scene costs no per-frame JS; a JS distance loop is the
    O(scene) walk item 19 rules out. Shaped in
    [3d-lod](../backlog/3d-lod.md). Mesh simplification stays a bake-tool
    job.

## Not in scope

Deliberately left behind, per the scope section of scene-graph-3d:
multi-backend abstraction (GLSL ES 3.00 is a settled bet), shader node
graphs / TSL, a WebGL-style global-state renderer, and a post-processing
composer (the window shader and `<texture blendMode>` compositing already
cover that tier).
