---
title: Shadow maps and their dependencies
description: Directional shadow maps for @solidrt/3d, staged over the three things they need - a sampleable depth id in the engine, per-target draw sinks in the spatial core, and a scene VIEW (render this scene into that target from this camera) in the library - with the view settled first because split-screen, minimaps and reflections hit the same wall.
created: 2026-08-26
---

# Shadow maps and their dependencies

Roadmap item 15 in [3d-roadmap](../notes/3d-roadmap.md). This file owns the
shape; the two engine items it consumes stay their own files:
[gpu-sampleable-depth](gpu-sampleable-depth.md) (the blocker) and
[gpu-depth-func](../backlog/gpu-depth-func.md) (named alongside, but NOT a dependency:
`LESS` is the right comparison for both the depth pass and the main pass,
so nothing here waits on it).

## Symptom

A lit scene has no contact shadows: a mesh floating over a ground plane
reads as pasted on. Three's `castShadow`/`receiveShadow` is the first thing
an app ports and the first thing that has no answer here. The achievable
tier today is a projected blob quad.

## What is in the way (measured against the code, 2026-08-26)

1. **Engine: depth is unsampleable.** A draw target's depth is a private
   `DEPTH_COMPONENT24` renderbuffer (`alloy/src/gpu/target.rs`,
   `create_mesh_storage`/`attach_storage`). Nothing can bind it. ES 3.0 has
   depth textures in core, so the storage swap is small; the design
   question is that a target's id names its COLOR, so its depth needs an id
   of its own to appear in a `textures` list.
2. **Spatial core: one draw sink per node.** `Node.sink: Option<DrawSink>`
   (`alloy/src/spatial/mod.rs`) - the flush writes uModel to exactly one
   entry in exactly one target. A shadow pass is a second entry for every
   caster in a second target, and it must receive the same world matrix
   from the same flush, or the library is back to writing uModel from JS
   per mesh per move (the O(scene) loop the whole design avoids). Same
   for `Node.slot: Option<SharedSlotSink>` (a light direction lands in one
   target only).
3. **Library: a Scene IS one target and one camera.** `createScene`
   creates the draw target, every `_attach` adds one entry to it, the
   camera writes `uViewProj` to it. There is no way to render the same
   scene twice from another viewpoint. This is the bigger half, and it is
   the same constraint that rules out split-screen, minimaps, reflections
   and portals - shadows are only the first consumer to hit it. So the
   view shape is settled first and shadows are built on it, not the other
   way round.

Already in hand, no work: `createDrawTarget` with `depth: true`, the
shared target-level sampler channel (`setTargetTextures`; the map binds
once per scene target), shared params (`uShadowMatrix` is one write per
light move), the dependency graph (a target sampling another target's
output re-renders in order, cycles throw), `moved` bookkeeping in the
scene (which nodes moved since the last sync, lights included),
`cull: "front"` on pipelines (the classic acne reducer for the depth
pass), `shaderMaterialClass` building one pipeline per vertex layout (a
depth-only material class serves every layout for free).

## Stages

Each stage lands alone, is verifiable alone, and is additive - nothing in
a later stage changes an earlier stage's surface.

### Stage 1 - engine: sampleable depth (gpu-sampleable-depth)

Landed 2026-08-26 (uncommitted): `DepthStorage` in alloy, `depth: true |
"texture"` and `depthTexture(target)` in `flux:gpu` and `@solidrt/core/gpu`,
`alloy/examples/depth_texture.rs` as the assertion. The shape below is what
shipped; the display probe's answer is in Findings.

The shape:

- `createDrawTarget(w, h, params, { depth: "texture" })`. The existing
  `depth: true` keeps the renderbuffer (unchanged cost, unchanged
  behavior); `"texture"` allocates a `DEPTH_COMPONENT24` texture instead
  and attaches it as `DEPTH_ATTACHMENT`. Storage kind is creation state,
  like `samples`. Throws with `samples >= 2`: a multisampled depth texture
  is not sampleable, and a resolve would be a blit (see
  msrtt-resolve-sampling: never a blit). A shadow map does not want MSAA
  anyway.
- `depthTexture(target): TextureId` - the depth's own id, allocated at
  creation with the target and stable for its life (resize follows the
  target like the color does). Throws for a target without `"texture"`
  depth. The id is sampler-only: it appears in any `textures` list
  (`setTargetTextures`, entry bindings, fragment targets) and is sampled
  as `sampler2D` reading `.r` in 0..1 window depth. Its sampler state is
  fixed `nearest`/`clamp`, no mipmap: ES 3.0 depth textures are not
  filterable without a comparison mode (a LINEAR depth sampler is
  texture-incomplete and samples zero), and the comparison mode is stage
  4.
- Lifetime: the depth id dies with its target. `destroyTexture(depthId)`
  throws ("owned by target N"); `destroyTexture(target)` removes both.
- Dependency graph: the UI-side sampler-graph mirror and the raster
  flush graph read `binding_sources()` as target ids. A depth id resolves
  to its owner target for propagation and cycle detection - one alias map
  `depth id -> target id` in `alloy/src/context/`, consulted where
  sources are resolved. A target sampling its own depth is a cycle and
  throws like any other.
- Registry: the raster side keeps `textures: HashMap<u64, GpuTexture>` for
  binding resolution (`alloy/src/raster/mod.rs`, the `gl_texture` lookup),
  so the depth GL name registers there under the new id with a new
  `TextureFormat::Depth24` (sample-only: not uploadable, `createTexture`
  rejects it). The UI-side `TextureEntry` needs an Impeller handle for
  display; whether Impeller adopts a `DEPTH_COMPONENT24` GL texture is the
  one unknown here. **Probe first** (a 20-line check through
  `gl::adopt_texture`): if it adopts, `<texture src={depthId}>` shows the
  depth as grey for free - the debug view every shadow implementation
  wants; if not, the entry carries no display handle and the paint walk
  skips the leaf with a warning ("depth ids are sampler-only"). Either
  way the sampling path is unaffected.
- Plugin/type mirror: `flux/src/alloy_plugins/gpu.rs`
  (`collect_draw_target_spec` parses `depth: true | "texture"`, a new
  `depthTexture` function), `packages/flux-types/gui/gpu.d.ts` and
  `packages/core/src/gpu.ts` doc/type updates (flux-types parity).
  Resource introspection (`GpuTextureInfo`) reports the format so the
  MCP inventory tells a depth id from a color id.

Not in this stage: `sampler2DShadow` / comparison sampling (stage 4),
depth on `createShaderTarget`/`createPipelineTexture` (draw targets only;
the single-draw creates derive depth from the pipeline and nobody needs
their depth yet), `DEPTH_COMPONENT32F`, stencil.

Verification: a draw target with `depth: "texture"` rendered through a
fragment target that samples `depthTexture(id)` and writes `.r` to grey,
compared against a snapshot; the display probe result recorded in
`## Findings

The durable findings - what is true of depth ids, per-target sinks,
scene views and the shadow set whether or not this plan had existed -
were cut to [depth-ids-views-shadows](../notes/depth-ids-views-shadows.md)
on close. What stays here is the archaeology:

- Stage 3: the view "entry filter" stayed INTERNAL (`ViewRecord.filter`,
  re-evaluated per mesh by `_setCast`); a public `ViewOptions.filter`
  would need a re-evaluation trigger the app controls, so it waits for a
  consumer.
- Stage 3: `receiveShadow` DEFAULTS TO TRUE (flipped after landing). The
  plan had it opt-in; Godot and Three both receive by default, and the
  opt-in shape meant an app had to know the flag existed before any
  shadow showed on its ground. The cost of the default is the sampler
  binds and a per-light branch in every lit fragment, paid even in
  unshadowed scenes; `receiveShadow: false` opts a material out and
  drops the sample. The placement (material, not object) is Godot's
  `disable_receive_shadows` / URP's material toggle, not Three's object
  flag - the program is the material's.
- Stage 3 verification: `examples/shadows.tsx` on the Linux GL path,
  shadows landing on the far side of the casters and sweeping with the
  sun between snapshots, the knot self-shadowing the cube.
- Stage 4a (several casters, 2026-08-27): the slot-per-light design and
  the `SHADOW_LOOKUP` export (which was a backlog item,
  [3d-glsl-shadow-lookup](3d-glsl-shadow-lookup.md), for a few hours
  between the demo copying `lit`'s chain and the export) are in the
  note; `MAX_SHADOWS = MAX_LIGHTS`, and the attach-time cap went with
  it. The moving-sun cost is one 64-float write per frame instead of one
  mat4, cheap next to the pass it drives. Verified on the Linux GL path
  with sun + fill + rim: three crossing shadows per caster, the sun's
  pair moving between snapshots while the fixed two hold, no shader or
  binding warnings.
- ANGLE/D3D11 verified 2026-08-27 (Windows box, RTX 3070, ES 3.0 via
  ANGLE 2.1): the same example through a LAN dev server, snapshotted
  through the control API with `?client=`. Shadows sit at the casters'
  feet in the same directions as on the Linux GL path - no y flip of
  the map lookup, no mirrored placement - and the client log is empty.
  The plan's one open verification is closed; the Mesa/ANGLE
  self-consistency argument (the receiver looks up with the matrix that
  rendered the map) holds on both.
