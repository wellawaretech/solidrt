---
title: 3d instances as spatial arena nodes - the full-matrix record projection
description: An instanced mesh's records are JS-written floats, so native transitions, clip players and per-instance picking never reach an instance, while every 2d sprite already is an arena node; the core's InstanceProjection has only Pose2D and the full-matrix sibling it anticipates is the missing piece.
created: 2026-09-06
completed: 2026-09-06
---

# 3d instances as spatial arena nodes

## Symptom

`createInstancedMesh(geometry, material, records)` draws N copies from an
interleaved record buffer the app writes (`setInstances`). The mesh is
one arena node (uModel places the population), the instances are opaque
floats: no instance has a transform the core knows about, so

- a moving instance costs a JS record rewrite per frame (the fleet, the
  crowd, the particle field - the "thousands of dynamic objects" tier
  the spatial core was framed for);
- native transitions (`setTransition`), clip players and root motion
  cannot target an instance;
- picking returns the mesh, by one box around the whole population
  (`bounds`), never the instance struck; `Hit` has no instance index.

The 2d package settled the opposite design in
[2d-spatial-citizenship](../done/2d-spatial-citizenship.md): every live
sprite is an arena node whose `InstanceRecord` sink writes its pose slot
at the core flush, one coalesced buffer write per frame however many
moved, hidden slots zeroed, growth by `retargetRecords`. That sink is
generic; only its projection is 2d. `InstanceProjection` in
`alloy/src/spatial/mod.rs` has the one variant `Pose2D`, and its own
comment calls a full-matrix projection "the anticipated 3d sibling". So
the 3d package is behind its 2d sibling on the core it was the first
consumer of.

## Shape

Two forms on one mesh, exactly 2d's split of node layer and records
layer:

- Core: `InstanceProjection::Matrix` writes the node's world matrix (16
  floats, column-major) into its record slot; per-buffer staging, dirty
  ranges, zeroing and `retargetRecords` are the Pose2D machinery
  unchanged. A `Matrix` slot may sit inside a wider record, so the
  projection carries its float offset within the stride, and the other
  attributes (color, a frame index) stay JS-owned in the same record -
  or in a second instance buffer, the two-slot split 2d uses.
- Package: `addInstance(mesh, transform?)` returns an instance node - a
  `SceneNode` child of the mesh with `setTransform`, `setTransition`,
  `setVisible` (hidden = zeroed slot, zero scale draws nothing) and
  `remove` (its slot recycles). `Hit` gains `instance` for a node-backed
  population; each instance node carries the geometry's bounds (and the
  shared shape) so picking, overlap and sweep resolve per instance.
  `setInstances(records)` stays as the raw escape hatch for motion only
  JS can compute, with the 2d rule: a buffer is node-backed or
  JS-written, never both (the core's staging republishes gap slots).
- Frustum culling stays per mesh (one draw entry, the population box);
  per-instance culling is a zeroed slot, which is the visibility rule
  already.

Comparison: Three's `InstancedMesh.setMatrixAt` is the records form and
its `instanceId` on a raycast hit is the per-instance pick; Unity's GPU
instancing draws GameObjects, so every instance is a node with its own
transform and animation; Godot's `MultiMesh` is records-only with
`MultiMeshInstance3D` as the one node. The node-backed form is Unity's
model over Three's buffer, which is what the 2d package already is.

## Done looks like

A fleet example where a thousand instances spring to new targets through
`setTransition` with zero per-frame JS, a tap reports the instance
struck, and the bench shows one coalesced record write per flush. The
records form's example is unchanged.

## Not in this item

Skinned instances (per-instance palettes), per-instance material params
beyond the record's own attributes, and the
[3d-lod](../backlog/3d-lod.md) swap. The additive follow-ups this item
surfaced are in [3d-instance-additive](../backlog/3d-instance-additive.md).

## Findings

Stage 1 (core, function face, probe) and stages 2 and 3 (components and
stock-material instancing; the app-owned style slot) all landed
2026-09-06.

- The record is the instance's matrix RELATIVE to the mesh, not its
  local matrix and not its world matrix: `InstanceProjection::Matrix`
  plus a per-buffer anchor on the instance group (`set_instance_record`
  takes it; one anchor per buffer, validated like the palette anchor),
  staged as `inverse(anchorWorld) * world`. That keeps `uModel *
  instanceMatrix()` as the shader convention and lets instances sit
  under groups below the mesh. The anchor inverse is cached per flush
  (`flush_id` stamp on `Spatial`): the walk visits an anchor before its
  descendants, so it is fresh by the time a bound node stages. A mesh
  move restages every instance and publishes nothing, since the relative
  records do not change.
- A hidden `Matrix` record is a zero-scale matrix with w kept at 1, never
  all zeros: an all-zero record leaves clip w = 0, a homogeneous point
  the clipper need not reject. Gap slots between bound ones start hidden
  too (`InstanceGroup::reserve`), so a fresh mirror never publishes raw
  zeros. `release_record` writes the projection's hidden record.
- Normals: `instanceNormalMatrix()` in `INSTANCE_MATRIX` is Three's
  derivation (columns divided by their squared lengths), exact for any
  rotation and scale; a sheared hierarchy takes
  `transpose(inverse(mat3(...)))` in the app's own stage. Unity's
  per-instance inverse in the record was considered and left as the
  additive path (a second projection) if shear-exact normals should
  become the default.
- Package: `createInstancedMesh(geometry, material, { capacity })` is
  node-backed, `createRecordMesh` the raw form (`setRecords`,
  `setRecordCount`, `<RecordMesh>`); no backwards compatibility, the five
  consumers moved. An instance is a `SceneNode` of kind "instance", the
  generic add/remove throw on it, `removeInstance` destroys (2d's rule).
  `disposeInstances` flushes the core BEFORE destroying the buffer so the
  destroyed instances' hiding writes land in a live buffer (the 2d
  layer's trap, the same order).
- Verified live (probes/3d-instance-probe.tsx, INSTANCE-OK): growth
  4 -> 16 while attached with the live records retargeted, slot
  recycling, a squad group between mesh and instance, a spring settling
  on an instance, per-instance pick with a triangle face, a hidden
  instance skipped, overlap naming the instance, and a synthetic tap
  delivered to the instance's handler and bubbled to the mesh with
  `event.instance` set.
- Style slot (stage 3): an instanced mesh binds TWO instance buffers,
  slot 0 the core's matrix records and slot 1 an app-owned style record
  whose layout is the material's `slot: 1` instance attributes
  (`InstanceAttribute.slot`, the gpu layer's per-slot binding that 2d's
  pose/style split already used). `setInstanceStyle(instance, floats)`
  writes a JS mirror with a coalesced dirty range; the scene's sync
  publishes it as one `writeBuffer` per mesh (a scene hook `_setStyle`,
  the mirror republished whole at attach and after growth). A fresh or
  recycled slot starts from the material's `instanceStyle` - white for
  a tint, so an unwritten instance shows the material's own color
  (Godot's default; Three's zero-filled instanceColor is the trap
  avoided). One JS-owned slot, not four: any per-copy layout interleaves
  into one record, and the mesh stays two buffers.
- Stock materials instance (stage 2's GLSL ergonomics): `lit`,
  `standard` and `unlit` take `instanced` (the vertex stage places by
  `instanceMatrix()`, normals by `instanceNormalMatrix()`, the class
  declares the matrix attributes and carries an instanced depth
  `shadowVertex`, so the fleet casts) and `instanceColors` (implies
  instanced; `iColor` in slot 1 forwarded as vColor, times aColor under
  vertexColors; the cutout shadow variants declare the same slots). The
  match: Unity's per-material "Enable GPU Instancing" flag, Three's
  built-ins instancing implicitly on an InstancedMesh with
  `setColorAt`, Godot's MultiMesh colors. Sprites stay per mesh (an
  instanced billboard field is a custom class's vertex stage). The
  depth class binds only the instance slots its shadowVertex reads
  (the records always, else it throws; the cutout variants read the
  color and keep the style slot), each bound slot with its whole
  layout: no variant binds a buffer it never reads, and an unread
  attribute inside a bound record is an inactive layout entry like an
  extra geometry channel.
- Components: `<InstancedMesh capacity bounds? label?>` is a parent
  providing both the scene context (parent = the mesh, so `<Group>`
  children are squads) and the mesh for `<Instance>`; `<Instance>` adds
  under the nearest enclosing group or instance, syncs the transform,
  transition, pointer and `style` props, and is a parent too. The mesh
  components now share one `syncMesh` (material, params, renderOrder,
  layers, culling, node) with the ref and cleanup per component, since
  a populated mesh frees its buffers where a plain one is removed.
- Verified live: the probe (INSTANCE-OK with the style checks: recycled
  slot blank, center instance green in the mirror and in the `/buffer`
  readback of the published slot-1 buffer, two buffers of 16 slots); the
  fleet example (a thousand `<Instance>` nodes under `lit({
  instanceColors })`, formations parked through the debug command,
  shadows and fog on the snapshot, `scene.pick` at each instance's
  projected pixel naming its slot, synthetic taps flipping the struck
  instance's style record in the readback, JS at 0.05 ms per frame with
  the springs running in the core).
- Found on the way: the fill-mode `<Scene>` read `displayScale()` inside
  its size effect's callback (a STRICT_READ_UNTRACKED warning on every
  fill scene's first layout); it now untracks like the 2d layer's
  oversample pick.
