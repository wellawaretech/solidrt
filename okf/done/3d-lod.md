---
title: Level of detail - screen-size selected variants as a core gate
description: A large scene ships every object at one triangle count; a track with a thousand trees either draws full-detail foliage at the horizon or nothing. The level select is a spatial-core gate beside frustum culling - projected size per target picks a level for node groups, per instance for populations - with a dithered cross-fade, so a thousand trees cost no per-frame JS.
created: 2026-08-30
completed: 2026-09-11
---

# Level of detail - screen-size selected variants as a core gate

## Symptom

Nothing in `@solidrt/3d` swaps a mesh by distance. Three's `LOD` object
(`addLevel(mesh, distance)`), Godot's visibility ranges and every game
engine's LOD group exist because a large outdoor scene cannot afford its
near-field triangle count at the horizon - and the reverse: a lone
billboard card looks wrong at arm's length. A track with a thousand trees,
fences and karts is exactly that scene, and on the low-end GPUs
[3d-low-end-gpu-performance](../backlog/3d-low-end-gpu-performance.md)
targets the fill and vertex budget is the binding one.

The app-side answer is a per-frame loop: for each LOD group, distance to
the camera, pick the level, `setVisible` the winner. That is a test per
group per frame in the interpreter - the same O(scene) walk
[spatial-core](../backlog/spatial-core.md) moved into Rust, and the
reason roadmap item 19 says frustum culling "in JS is ruled out, not
deferred". LOD is the same query over the same index and belongs beside
it.

## Decisions

Three shapes were weighed: Three's distance-keyed group, Godot's per-node
visibility range, Unity's screen-relative LODGroup. What ships is Unity's
model over Three's vocabulary, for these reasons.

**The metric is projected size, not distance.** A level threshold is the
object's projected height as a fraction of the viewport (Unity's "screen
relative transition height"): `size = bias * radius * focal / distance`
for a perspective target (`focal = 1 / tan(fov / 2)`, `radius` the half
diagonal of the group's world box), `bias * radius * focal` for an
orthographic one. Distance is Three's rule and is wrong wherever the
camera is not a fixed-FOV perspective one: an orthographic minimap, a
zoomed-in camera, a scaled-up tree. Coverage costs one extra float per
target and a division; it also makes the quality knob natural - `lodBias`
multiplies coverage, Unity's `QualitySettings.lodBias`.

**A group picks exactly one level; per-node ranges do not.** Godot's
per-node begin/end ranges are the more general primitive but a partition
needs one decision per boundary: two siblings each carrying their own
hysteresis leave a hole or an overlap on a fast crossing. The core keeps
a level list per group and picks one index, so no combination of sizes
can draw two levels except during a fade.

**Selection is per target, exactly like frustums.** Every camera write
already sets that target's frustum in the core; it now sets its LOD view
(eye, focal, ortho flag, bias) too. A group stores its level per target
and the gate composes in the existing cull pass: `want = shown &&
frustum_allows && lod_allows`. The scene target and each view use their
own camera (a minimap sees the far level of the same tree); shadow tiles
take the scene camera's view, so a caster draws the level the camera
sees and its shadow matches (Unity's and Godot's rule); probe faces set
none, like their frustum.

**Cross-fade is a dither, driven by the core.** With `fade > 0` the
boundary at size `s` widens to a band `[s, s * (1 + fade))` in which both
levels draw, the near one keeping the pixels whose screen hash is below
the band position and the far one the rest (a partition, no double
draw). The core writes one `uLodFade` vec2 (threshold, side) per entry as
the band position moves; the stock fragments discard by it, a custom
class opts in by declaring the uniform. Without a fade the switch is
hard with a one-sided hysteresis band (`LOD_HYSTERESIS`, a coverage
fraction; Three's rule), so a boundary never flickers. Shadow entries
never fade: their depth pass switches hard at the band midpoint, which
for two variants of one shape is invisible.

**Populations pick per instance.** A whole-population switch on an
InstancedMesh is worthless for a spread forest (every tree flips at
once), and a thousand tree NODES with three meshes each is a thousand
draws per target, which loses to draw-call cost before LOD helps. So an
instance node can carry one record sink PER LEVEL - one matrix buffer per
level mesh, the levels sharing the instance set - and the core stages an
instance's record into the level its own projected size picks, hiding it
in the others. Records are target-agnostic, so instanced levels pick
against ONE named reference target (the scene's) and every target draws
that choice; instanced levels switch hard with hysteresis, no fade (a
per-instance fade would need a per-record value, a stream, later if
ever).

**Queries follow a target.** Picking and collision live in the index, not
in the draw sinks, so without a change every level of a tree would be
pickable at once. `QueryFilter` gains a `target`: a query sees the levels
that target draws (the scene's from `scene.pick`/overlap/sweep/
moveAndSlide, the view's from `view.pick`); no target means every level.
Collision against a LOD group follows the drawn level; a collider that
must not change with the camera is its own undrawn mesh on a collision
layer, the physics-collider pattern AGENTS.md already documents.

**Names.** Three's `LOD` becomes `createLod(levels, { fade })` /
`setLod` and `<Lod fade>`; a level is `{ node, size }` where `size` is
the coverage BELOW which the level hands over to the next, the last
level's size the cull threshold (0 = never culled): Unity's threshold
list, no null level. In JSX the level is declared on the child
(`<Mesh lodSize={0.3}>`), never by position: drei's `<Detailed
distances>` maps children positionally and attach order is not JSX order
here. The instanced form is `createInstancedLod(levels, opts)` /
`<InstancedLod levels capacity>` with `{ geometry, material, size }`
levels as data and `<Instance>` children.

Not changed by the wider scope: mesh simplification stays a bake-tool job
(a simplifier in Rust and an .srtm format change, its own item; a model
part carrying levels should load as an LOD group without authoring),
impostor baking and HLOD stay out, screen-space ERROR metrics (Godot's
automatic mesh LOD) stay out - size is the input, not a projected edge
error.

## Placement

- Core: `alloy/src/spatial/` - `LodLevel`/`LodView`, `set_lod`,
  `set_lod_view`, the per-target state on the group, the level tag the
  walk propagates like `shown`, `lod_pass` between the walk and the cull
  pass, `DrawSink.fade` + `SinkWriter::write_fade`, record sinks as a
  per-level list, `QueryFilter.target`. Tests in
  `alloy/src/tests/spatial.rs`.
- Plugin: `flux/src/alloy_plugins/spatial.rs` (`setLod`, `setLodView`,
  `bindDraw` fade flag, `bindMatrixRecord` buffer list, the filter's
  target); `packages/flux-types/gui/spatial.d.ts` parity.
- Library: `packages/3d/src/lod.ts` (createLod/setLod/createInstancedLod),
  the LOD view writes beside the frustum writes in scene.ts, `lodBias`,
  the query target, `uLodFade` in the stock fragments (`LOD_FADE` exported
  from `@solidrt/3d/glsl` for custom classes), `<Lod>`, `lodSize`,
  `<InstancedLod>`.
- `packages/3d/examples/lod.tsx`, `probes/3d-lod-bench.tsx`, AGENTS.md.

## Done looks like

A scene of a thousand trees, three levels each (mesh, simplified mesh,
sprite card), holds its frame time as the camera flies over, with the
level swaps invisible inside the fade band and zero per-frame JS,
measured by the spatial-core bench pattern. `examples/` gains one.

## Not in this item

Automatic mesh simplification (an authoring/bake-tool job, listed under
the interpreter losses in
[3d-differentiators](../notes/3d-differentiators.md)), screen-space-error
metrics, per-instance fades, impostor baking, HLOD.

## Findings

The knowledge that outlives the plan - the negative `proj[5]`, the
half-diagonal measure, the bench numbers - is in
[3d-lod-measurements](../notes/3d-lod-measurements.md). What stays here
is how the item went.

Landed 2026-09-11: core (`set_lod`, `set_lod_view`, the level tag the
walk propagates, `lod_pass` between the walk and the cull pass, record
sinks as a per-level list, `QueryFilter.target`, `DrawSink.fade` +
`write_fade`), plugin and flux-types, library (`createLod`/`setLod`,
`createInstancedLod`, `<Lod>` + `lodSize`, `<InstancedLod>`, `lodBias`,
`LOD_FADE` in every stock fragment), `examples/lod.tsx`,
`probes/3d-lod-bench.tsx`, AGENTS.md.

- The first live run culled everything: the focal factor was read with
  the projection's sign (see the note). One line, found by replaying the
  example's numbers in a core test that passed while the app failed -
  the gap had to be in the plumbing.
- Off before on: when a group changes level the pass touches the outgoing
  levels first, so a target's count writes read as one entry switching
  off and the next switching on, never both on in the write stream.
- Shadow tiles are ordinary view records; they take the scene camera's
  LOD view once at creation and then on every scene camera move (a flag
  on the record), so a tile created after the camera settled still
  measures.
- The `<Lod>` component declares its levels from the direct children
  carrying `lodSize`, registered through context; a grandchild with the
  prop throws at registration (the core wants direct children). The
  drei-style positional `distances` list was rejected because attach
  order is not JSX order here.
- An instanced LOD is the first level's mesh: the other levels are its
  children at identity sharing its instance-slot object, and the anchor
  of every record is that first mesh, which is also the group carrying
  the sizes. `addInstance` grows, counts and blanks every level; the
  style write fans out; `setLayers`/`setCastShadow` on the population
  reach every level.
