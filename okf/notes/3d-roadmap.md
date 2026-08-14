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
   also built from, so custom materials never become second-class.
3. [ ] **UI as live 3D content.** Engine:
   [snapshot-boundary-texture-id](../backlog/snapshot-boundary-texture-id.md).
   A snapshot boundary's retained texture as an ordinary TextureId that
   updates as the subtree repaints - the load-bearing piece of the
   one-tree differentiator (real UI on 3D geometry), and the capability
   the browser architecturally forbids.
4. [ ] **Picking.** The volume tier is delivered (mesh pointer events over
   `scene.pick`/`scene.raycast`, BVH broadphase maintained from the sync
   walk, library only as staged). Remaining: the triangle-accurate tier
   (`face`/`uv` on hits, correct concave silhouettes) - per the
   differentiators ladder that is CORE work, because per-triangle rays in JS
   are interpreter-hostile, and it should ride the scene-walk descent (item
   19) rather than grow a JS implementation. Routing events into a UI subtree
   mapped onto a mesh stays with item 3.
5. [ ] **Lights and lit materials (lambert/phong).** Engine:
   [gpu-uniform-arrays](../done/gpu-uniform-arrays.md) - light lists without
   baking a cap into shader source (the agreed answer; the fixed-scalar
   workaround was rejected in scene-graph-3d). What remains is library-only:
   light scene nodes plus material classes built per item 2's policy.
6. [ ] **Transparency.** Engine: the blend factor vocabulary,
   [gpu-pipeline-blend-modes](../backlog/gpu-pipeline-blend-modes.md) - the
   old "sorting plus premultiplied" blocker inverted once the library
   existed, because the scene graph is the sorter. Library: back-to-front
   transparent sort over stable DrawIds (recomputed only when the camera
   moves), fix the entry-rebuild-at-end order trap, settle premultiplied
   vs straight.
7. [ ] **Real models: a glTF subset loader.** Library + CLI; the engine entry
   ticket (index buffers) is paid. Direction per the differentiators note:
   run the mature loaders under Bun in the CLI at build/pack time and ship
   pre-interleaved buffers in the exact addDraw layout, so runtime loading is
   a buffer upload. Runtime-fetched user models stay a separate, later
   problem.
8. [ ] **Mipmaps.** Engine: [gpu-mipmaps](../backlog/gpu-mipmaps.md).
   Textured models alias immediately at minification; the one engine item
   staging step 3 (real models) still needs.
9. [ ] **MSAA on pipeline targets.** Engine:
   [gpu-target-antialiasing](../backlog/gpu-target-antialiasing.md).
   Silhouette jaggies are the dominant artifact on filled geometry.
10. [ ] **Geometry breadth and the vertex-layout ceiling.** Library. Still
    demand-gated: closed sweep loops (twist reconciliation around a loop),
    capsule (a tube special case now that sweep exists), and further named
    layouts (tangents, skin weights) when items 7 and 16 force them. The
    direction stays a small set of named layouts, not an open
    BufferGeometry-style model.
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
12. [ ] **Instanced mesh sugar.** Library only; the engine half (per-instance
    attributes) is paid. One draw entry and one params write covering N
    instances is also the standing answer to the churn workloads (particles,
    projectiles) that the FFI boundary punishes.
13. [ ] **Camera and control breadth.** OrthographicCamera is small library
    work. First-person controls are blocked on engine
    [relative-mouse-input](../backlog/relative-mouse-input.md) - pointer
    lock/relative motion - not on anything in the GPU stack.
14. [ ] **Environment tier: skybox, reflection/environment maps.** Engine:
    [gpu-cube-maps](../backlog/gpu-cube-maps.md). Demand-gated. The binding
    side is already paid: a shared target-level sampler (`setTargetTextures`)
    binds an environment map once per scene target; cube maps are the
    remaining engine gap.
15. [ ] **Shadow maps.** Engine: sampleable depth
    ([gpu-sampleable-depth](../backlog/gpu-sampleable-depth.md)) and a
    depth-func option ([gpu-depth-func](../backlog/gpu-depth-func.md)); the
    map itself binds through the shared target-level sampler channel.
16. [ ] **Skinning and morph targets.** Engine: float texture formats (same
    extensions file). Per-vertex JS is ruled out by the interpreter, so bone
    matrices live in textures and are sampled in the vertex shader.
17. [ ] **PBR and the color-space decision.** The furthest tier: physically
    based lighting math forces the sRGB/linear question the pixel contract
    currently answers with "non-linear RGBA8 everywhere".
18. [x] **Scene background.** Deferred within the item: the texture-id form (a
    reserved non-breaking union widening - decide fit semantics when a
    consumer arrives). The boundary with item 14 stands: a camera-linked
    background (skybox) is the cube-map case, this was the screen-space one.
    Note for the proving-ground demo: its ground FADES over the backdrop,
    which needs item 6's blend factors before the two layers can merge.
19. [ ] **Scene scale.** Frustum culling in the library when a consumer needs
    it (scenes under a few thousand nodes will not), and the recorded escape
    hatch behind everything above: the draw-list design deliberately allows
    the scene walk to move into core without an app-facing API change.
    Explicit non-goal until something demands it.

## Not in scope

Deliberately left behind, per the scope section of scene-graph-3d:
multi-backend abstraction (GLSL ES 3.00 is a settled bet), shader node
graphs / TSL, a WebGL-style global-state renderer, and a post-processing
composer (the window shader and `<texture blendMode>` compositing already
cover that tier).
