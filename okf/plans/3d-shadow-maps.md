---
title: Shadow maps and their dependencies
description: Directional shadow maps for @solidrt/3d, staged over the three things they need - a sampleable depth id in the engine, per-target draw sinks in the spatial core, and a scene VIEW (render this scene into that target from this camera) in the library - with the view settled first because split-screen, minimaps and reflections hit the same wall.
created: 2026-08-26
---

# Shadow maps and their dependencies

Roadmap item 15 in [3d-roadmap](../notes/3d-roadmap.md). This file owns the
shape; the two engine items it consumes stay their own files:
[gpu-sampleable-depth](gpu-sampleable-depth.md) (the blocker) and
[gpu-depth-func](gpu-depth-func.md) (named alongside, but NOT a dependency:
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
`## Findings` here.

### Stage 2 - core + library: scene views

Landed 2026-08-26 (uncommitted): per-target draw and slot sinks in the
spatial core (`bind_sink`/`unbind_sink`, `bind_shared_slot`/
`unbind_shared_slot`; `unbindDraw(node, target?)` / `unbindSlot(node,
target?)` in `flux:spatial`), `scene.createView(opts)` with
`overrideMaterial` and `depth: "texture"`, `ortho` on `CameraUpdate`
(`orthographic()` in math.ts), `packages/3d/examples/split-screen.tsx`
as the demonstration. Deviations from the shape below, all in Findings:
the per-entry flush state is a private `BoundSink` wrapper rather than
fields on `DrawSink`; an overridden view is not sorted; `ortho: null`
returns to perspective; the scene's merged `setParams` names replay on a
new view. Verification through the JS surface (snapshots of the three
leaves) waits for a client rebuild.

**Spatial core.** `Node.sink` becomes `sinks: Vec<DrawSink>` and
`Node.slot` becomes `slots: Vec<SharedSlotSink>`, both keyed by target
(`bind_draw` replaces the sink with the same target, else appends;
`unbind_draw(node, target: Option<u64>)` removes one or all). `entry_on`
and `fresh` move into `DrawSink` (they are per entry, not per node). The
flush writes params and count to every sink; `set_count` applies to all
of them. Plugin: `bindDraw` keeps its signature (it already carries the
target), `unbindDraw(node, target?)` gains the optional target,
`bindDirectionSlot` keeps its signature, `unbindSlot(node, target?)`
likewise. Tests in `alloy/src/spatial/tests/` cover two sinks receiving
one move.

**Library.** A view is "render this scene into that target from this
camera": its own draw target, its own camera, one entry per mesh,
sharing the scene's geometry buffers and (unless overridden) materials.

```ts
type ViewOptions = {
  width: number
  height: number
  /** Every mesh draws with this material instead of its own (Three's
   * scene.overrideMaterial, scoped to the view). The view carries no
   * bindings or params of the meshes' own materials then. */
  overrideMaterial?: Material
  /** Target storage; `depth` defaults to true, "texture" for stage 1's
   * sampleable depth. */
  depth?: true | "texture"
  clearColor?, samples?, filter?, wrap?, label?
}
type View = {
  texture: TextureId
  depthTexture: TextureId | null      // when depth: "texture"
  setCamera(update: CameraUpdate): void
  setSize(width, height): void
  setParams(params: ShaderParams): void   // view-owned shared params
  dispose(): void
}
scene.createView(opts): View
```

- `CameraUpdate` gains `ortho?: { left, right, top, bottom }`: with it,
  `perspective()` is swapped for a new `orthographic()` in `math.ts`
  (same y-down clip flip, the one trap in that file) and `fov` is ignored.
  This is roadmap item 13's `OrthographicCamera` arriving as a camera
  option rather than a component - the scene's own camera takes it too,
  and `<PerspectiveCamera>` stays as the component face; an
  `<OrthographicCamera>` component is a follow-up when a declarative
  consumer asks.
- Entry membership mirrors the scene: `_attach`/`_detach`/`setGeometry`/
  `setMaterial` add, remove and rebuild the mesh's entry in every view;
  `setVisible` goes through the core's count write (all sinks); the
  transparent sort issues `setDrawOrder` per view target (the sort key
  is view-space, so a view with its own camera re-sorts from its own
  view matrix - orderEntries already takes the view matrix as an
  argument).
- Shared params fan out: the camera goes to the view's target only; the
  light set (`uLightColor`, `uLightCount`, hemisphere) and
  `scene.setParams` names go to every target; a light's direction slot is
  bound once per target (the core fan-out above). A view with an
  `overrideMaterial` still receives them (zero-coverage shared params
  are stored and skipped, the existing rule), so a depth-only override
  costs nothing there.
- Background: the scene's background entry is not mirrored (a view has
  no background in stage 2; `clearColor` is the view's backdrop).
- Instanced meshes under `overrideMaterial`: the override's vertex stage
  cannot know the instance record layout, so instanced meshes are
  skipped in an overridden view, documented. The additive follow-up is
  a per-class `shadowVertex` (a class declaring `instanceAttributes`
  supplies the depth-pass vertex stage that reads them).
- Not in stage 2, additive later: `view.pick`/`view.project`/
  `view.handlers` (a picking view needs its camera in the ray; the code
  is a parameterization of the scene's), a `<View>` component.

Verification: `examples/split-screen.tsx` - one scene, the built-in leaf
plus a `createView` leaf with a second camera (one orthographic top-down),
a moving group visible in both from ONE `setTransform` per frame, checked
through snapshots of the two leaves. This also proves the view shape on a
consumer that is not shadows.

### Stage 3 - library: directional shadow maps

Landed 2026-08-26 (uncommitted), library only as designed: `castShadow` +
`shadow` on `DirectionalLight` (options, `setLight`, the component
props), `castShadow` on `Mesh` (`setCastShadow`, `<Mesh castShadow>`),
`lit` receiving by default (`receiveShadow: false` opts out), `SHADOW` in `@solidrt/3d/glsl`,
`shadowDepthMaterial()` (internal, material.ts), `MAX_SHADOWS = 1`, and
`packages/3d/examples/shadows.tsx`. Verified on screen (Linux GL): the
shadows land on the far side of the casters and sweep with the sun, the
knot self-shadows the cube, no log entries. Deviations in Findings.

Built entirely on stages 1 and 2; no engine change.

- **Casting.** `DirectionalLight` gains `castShadow` (default false,
  Three's default) and `shadow` options, Three's vocabulary:
  `{ mapSize?: number (1024), bias?: number (0), normalBias?: number (0),
  camera?: { left, right, top, bottom, near, far } }` (the orthographic
  light frustum, defaults +-5 / 0.5..500 like Three's
  `DirectionalLightShadow`). A casting light owns an internal
  `scene.createView({ width: mapSize, height: mapSize, depth: "texture",
  overrideMaterial: DEPTH_MATERIAL })` where `DEPTH_MATERIAL` is a
  module-level `shaderMaterialClass` (vertex `uViewProj * uModel * aPos`,
  empty fragment, `cull: "front"`). The view's camera sits at the light's
  WORLD position looking along its world direction (Three's rule: a
  shadow camera is placed by the light's position, so a casting light's
  node position starts to matter - a trap to document; a light at the
  origin with a downward direction shadows nothing above it). Light moves
  are core-driven for the direction slot, but the shadow matrix is JS:
  the scene's `moved` list already names the light node, so a moved
  casting light recomputes its view camera and rewrites `uShadowMatrix`
  in the same sync - one light, one mat4, no per-mesh cost.
- **Mesh side.** `castShadow` on nodes (plain field + `<Mesh castShadow>`
  prop, default false): the shadow view's entry set is the casting
  meshes, and toggling it adds/removes that entry (`setCastShadow` verb).
  This is the general view feature "entry filter" applied by the shadow
  view, not a shadow-only path.
- **Receiving.** A material option, not a node flag: `lit({ receiveShadow:
  true })` selects a lit class variant whose fragment declares
  `uniform sampler2D uShadowMap; uniform mat4 uShadowMatrix; uniform
  float uShadowBias; uniform int uShadowLight;` and multiplies the
  directional term of light `uShadowLight` by the shadow factor. Deliberate
  divergence from Three (where `receiveShadow` is on the object): here
  the material picks the program, exactly as `vertexColors` and
  `triplanar` already do, and a per-object flag would need a per-object
  program swap. A scene with no casting light writes `uShadowLight: -1`,
  so a receiving material in an unshadowed scene draws normally. The
  map binds through `setTargetTextures(scene.texture, { uShadowMap })`
  - the shared target-level channel item 15 was designed to use - and
  the shadow coordinate is `uShadowMatrix * vec4(vWorldPos, 1.0)`
  computed in the fragment (one mat4 multiply, no new varying, so the
  `LIT_VERTEX` interface does not change and custom vertex stages keep
  working).
- **Exported GLSL.** `@solidrt/3d/glsl` gains `SHADOW`: `float
  shadow(sampler2D map, vec4 coord, float bias)` - perspective divide,
  0..1 remap, out-of-frustum returns 1 (lit), 3x3 PCF over texel
  neighbours reading `.r` with a manual comparison (stage 1's depth id is
  nearest-sampled, so the softness is the loop, not the sampler). Custom
  materials compose it the way they compose `LAMBERT`.
- Precedence and cost: `MAX_SHADOWS = 1` in stage 3 (one casting
  directional light per scene; a second `castShadow` throws at attach,
  like the `MAX_LIGHTS` check). The array form (`uShadowMatrix[N]`,
  `uShadowMap0..N`) is additive; it is NOT done now because each map is
  a sampler unit and a full extra pass, and the demand evidence is one sun.
- Orientation check: the lookup uses the very matrix that rendered the
  map, so it is self-consistent under the y-down clip flip. The one thing
  to verify on the first run is the y orientation of target-to-target
  sampling on the ANGLE path
  ([impeller-texture-inversion](../notes/impeller-texture-inversion.md)
  territory); `scene-post-effect.tsx` already samples a target from a
  pass on every platform, so the convention exists and is followed.

Verification: `examples/shadows.tsx` - a ground plane with `lit({
receiveShadow: true })`, a few casting meshes turning inside a group, a
sun swinging through its arc via a `setTransform` on the light node, the
depth id shown in a corner `<texture>` (when stage 1's probe allows) -
snapshots of the ground where the shadow is expected dark and light.
`AGENTS.md` documents the casting-light-position trap, the
receiveShadow-on-material divergence, the acne/bias knobs and the
instanced-caster gap.

### Stage 4 - deferred, each demand-gated

- Comparison sampling: `compare: "less"` as sampler state on the depth
  id -> `sampler2DShadow`, which unlocks LINEAR filtering (hardware 2x2
  PCF) at one tap instead of nine. Pure quality, no API change for apps
  using `lit`.
- Several casting lights (the array form above); spot lights (a
  perspective shadow camera, otherwise identical); point lights (cube
  maps, [gpu-cube-maps](gpu-cube-maps.md)).
- Cascaded shadow maps for large outdoor scenes; a `shadow.camera` box
  is the honest tier until a scene outgrows it.
- Instanced casters via a per-class `shadowVertex`.
- `depthCompare` on pipelines ([gpu-depth-func](gpu-depth-func.md)):
  reversed-z or equal-depth tricks; nothing in stages 1-3 needs it.

## Done looks like

`examples/shadows.tsx` renders a contact shadow under a mesh from one
`castShadow` on a light, one `castShadow` per caster and one
`receiveShadow` on the ground material; the sun moves with one
`setTransform` per frame; nothing else in the scene pays for it (a
non-casting mesh has no shadow-view entry). Split-screen works from the
same view primitive. The roadmap checks item 15 and its item 13 half
(orthographic).

## Findings

(appended during the work; cut to notes/ on completion)

- Impeller adopts a `DEPTH_COMPONENT24` GL texture through
  `adopt_opengl_texture` without complaint (the descriptor claims RGBA8;
  Impeller only binds and samples), and a display-list
  `draw_texture_rect` of it comes out as `(d, 0, 0, 1)` on the Linux GL
  path - ES 3.0's depth-texture sampling rule, red channel only. So a
  depth id is displayable via `<texture src>` for free, as a red-tinted
  depth view, provided the draw samples NEAREST: the id's fixed
  `SamplerState::DEPTH` makes the paint walk pick nearest, which keeps the
  texture complete. The registry therefore needs no optional Impeller
  handle and the paint walk no special case.
- A depth id is an alias, not a texture, everywhere a graph question is
  asked: the UI-side sampler mirror records the OWNER target for a binding
  to it (`Context::source_of`), and the raster flush graph maps binding
  sources through `depth_owners`. Recording the raw depth id instead would
  have left content propagation, cycle detection and reclamation blind to
  the edge, since none of them index by anything but target ids.
- Ownership follows the color exactly: adopted name, Impeller deletes it
  on handle drop, a resize allocates a FRESH depth name and re-adopts at
  the same id (respecifying the old name would race in-flight display
  lists and, worse, the old handle's drop would delete the live texture).
- The three fused creates (`create_shader_texture`,
  `create_pipeline_texture`, `create_shader_target`) validate their
  initial bindings on their own (unit budget only) and never pass through
  `validate_new_bindings`; a per-binding rule added there alone
  (the linear-override-on-depth rejection) silently missed them. Any
  future binding rule must be applied at both places - or the creates
  should be routed through the shared validator, which is a small refactor
  left for when a second rule arrives.
- An alloy example panicking inside `app.run`'s closure (the srt-ui
  thread) leaves the main thread pumping the SDL window: a black window
  that never closes. `depth_texture.rs` installs a panic hook that exits;
  the other examples do not (ideas.md).
- Stage 2, core: `entry_on`/`fresh` stayed OFF the public `DrawSink`
  (which is the caller's bind spec, Copy, compared in tests) and live in
  a private `BoundSink { sink, entry_on, fresh }` on the node - the
  flush state is the core's, not the binder's. Sinks and slots are keyed
  by target only (one entry per mesh per target is the 3d package's
  invariant; a (target, name) key for slots is the additive widening if
  a node ever feeds two arrays of one target). `uNormal` is computed once
  per node per flush however many sinks ask.
- Stage 2, core: a rebind (bind on a target the node already draws into)
  re-queues the node, and a queued node recomputes as changed (the
  reparent rule), so the node's OTHER sinks get a params rewrite in that
  flush. Pre-existing for the single sink (set_bounds does the same);
  rebinds are rebuildEntry-rare, so it stays. Splitting "queued for
  structure" from "queued for bookkeeping" is the fix if it ever shows in
  a profile.
- Stage 2, library: an `overrideMaterial` view draws in add order - no
  renderOrder, no transparent sort. The sort reads `mesh._transparent`
  (the mesh's own material), which says nothing about the override; for
  the depth pass order is irrelevant, and a transparent override is a
  visualizer's problem, not a shadow's. `orderEntries` grew an `entry`
  accessor so a non-overridden view sorts with its own camera's view
  matrix and its own per-mesh entries; the world-space centers refresh
  once per sync for every sort.
- Stage 2, library: `pick()` handles `ortho` (rays along the camera
  forward axis from a point on the camera plane), since the scene's own
  camera takes the option; `project()` under ortho has w = 1 everywhere,
  so its behind-the-camera null never fires there. A view has no pick.
- Stage 2, library: the light set is rewritten for EVERY target whenever
  it changes or a view is created (`writeLights` rebinds each light's
  direction slot per target - the slot re-seeds at the flush); the merged
  `scene.setParams` names are kept in `sceneParams` and replayed on a new
  view, because a view created after a `setParams({ uTime })` would
  otherwise never see the name. `view.setParams` is the view's own
  channel and is not replayed anywhere.
- Stage 3: the y orientation of target-to-target depth sampling needed
  no correction on the GL path: the receiver looks up with the very
  matrix that rendered the map and raw texture coordinates
  (`ndc * 0.5 + 0.5`), and GL stores the map in the same convention it
  rasterized it in, so the flip cancels. The ANGLE/D3D path is still
  unverified (impeller-texture-inversion territory).
- Stage 3: the view "entry filter" stayed INTERNAL (`ViewRecord.filter`,
  re-evaluated per mesh by `_setCast`); a public `ViewOptions.filter`
  would need a re-evaluation trigger the app controls, so it waits for a
  consumer.
- Stage 3: every receiving target carries a `uShadowMap` binding at all
  times - a 1x1 white placeholder (depth 1, never shadowed) when no light
  casts, the shadow view's depth id otherwise - so the no-shadow state is
  deterministic rather than "whatever unit 0 holds" (the engine does
  accept a declared-but-unbound sampler at entry creation). The shadow
  view itself is excluded from the binding (same-pass feedback).
- Stage 3: the shadow camera is re-placed by comparing the light's
  world matrix against the one it was last placed from (16 compares per
  sync while a shadow exists), not by scanning the `moved` list for the
  light or its ancestors; a core-driven transition on the light is
  followed only when some sync runs, since a transition alone schedules
  none - the same limit picking's sort keys have.
- Stage 3: `receiveShadow` DEFAULTS TO TRUE (flipped after landing). The
  plan had it opt-in; Godot and Three both receive by default, and the
  opt-in shape meant an app had to know the flag existed before any
  shadow showed on its ground. The cost of the default is one sampler
  bind and a per-light branch in every lit fragment, paid even in
  unshadowed scenes; `receiveShadow: false` opts a material out and
  drops the sample. The placement (material, not object) is Godot's
  `disable_receive_shadows` / URP's material toggle, not Three's object
  flag - the program is the material's.
- Stage 3: `_shadowChanged` on a size change resizes the shadow view in
  place (`setTargetSize`; the depth id survives, stage 1's rule), so
  `setLight(light, { shadow: { mapSize } })` never rebinds the map.
