---
title: Instanced meshes - the additive follow-ups
description: What the instance-citizenship item left as strictly additive work - per-instance frustum gating, the transparent sort center from the instances' union box, shear-exact normals as a second projection, instanced sprites, and a per-instance frame/atlas convention for the stock materials; none changes a shipped contract.
created: 2026-09-06
---

# Instanced meshes - the additive follow-ups

Leftovers of [3d-instance-citizenship](../done/3d-instance-citizenship.md),
each additive to what shipped: the two-slot instanced mesh, the stock
materials' `instanced`/`instanceColors`, `<InstancedMesh>`/`<Instance>`.

## Symptom

- A transparent instanced mesh without `bounds` sorts by its node
  position. Its frustum box follows the instances (the scene sets the
  core's cull group over the live instance nodes), but the sort center
  is computed in JS from the local bounds, so the union's center needs
  a core read of the group box (six floats through the plugin) before
  the sort can use it.
- Records are drawn whether or not the instance is in view: an instance
  far outside the frustum still costs its vertices. Per-instance gating
  (a hidden record for an instance whose box fails the target's
  frustum, restored when it re-enters) is a core-side flush concern:
  the record projection already writes the hidden record for
  invisibility, so culling is the same write from a different test.
  Per target, though, so a shadow view must not lose casters the camera
  cannot see - the gating has to be by the union of the targets that
  draw the mesh, or off for casters.
- `instanceNormalMatrix()` is exact for rotation and any scale and
  misses only shear (a non-uniformly scaled group above a rotated
  instance). Unity ships the inverse per instance; the additive path
  here is a second projection (`InstanceProjection::MatrixNormal`, 16 +
  9 floats, or a second slot the core writes) chosen by the material
  when shear-exact normals should be the default, with the existing
  derivation staying the cheap form.
- `sprite()` stays one quad per mesh: an instanced billboard field (a
  particle cloud, a crowd of cutout trees) is a custom class today.
  `sprite({ instanced: true })` would place the billboard at the
  instance's position with the instance's scale as its size, Three's
  Points/Godot's particle billboards.
- The stock materials carry one per-instance value, the color. A
  per-instance atlas frame or uv offset (`instanceFrames`: a vec4 in
  slot 1 beside the color) is the next most common per-copy value
  (Godot's custom data, Unity's per-instance property block).

## Done looks like

Each is its own small item; the order is by demand. None renames or
reshapes a shipped call: `bounds` stays, the style slot layout stays the
material's, `instanceNormalMatrix()` stays.
