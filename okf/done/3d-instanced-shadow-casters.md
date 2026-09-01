---
title: Instanced meshes cast no shadow
description: Landed 2026-09-02 as the per-class `shadowVertex` option on shaderMaterialClass - the class's vertex stage reduced to position (instance placement included), from which the class builds one shared depth material the shadow views draw its casters with. `castShadow` on an InstancedMesh now works like on any mesh.
created: 2026-08-27
---

# Instanced meshes cast no shadow

Was: an `overrideMaterial` view (the shadow view is one) replaced every
entry's pipeline with the override's, whose vertex stage is `uViewProj *
uModel * aPos` - it cannot know an instanced material's record layout,
so shadow views skipped instanced meshes and `castShadow` on an
InstancedMesh did nothing.

Landed 2026-09-02, the shape "done looks like" described, proven first
by the skinned casters (2026-09-01):

- `shaderMaterialClass({ shadowVertex })`: the class's vertex stage
  minus everything but position - the instance placement, a
  displacement - under the same standard-uniform contract (uModel and
  uViewProj required, validated at class creation). The class lazily
  builds ONE depth class from it (shared depth fragment, the shadow
  cull side, the class's own `instanceAttributes`) with one shared
  instance, surfaced as every instance's default `Material.shadow`;
  the `shadow` instance option still overrides it (a cutout's discard
  stays an instance affair). `dispose()` disposes the depth class too.
- The shadow view's attach admits an instanced mesh when the chosen
  shadow variant declares `instanceAttributes` (a stride mismatch
  against the mesh's records throws); without a `shadowVertex` the mesh
  is skipped as before. The instance-count and buffer-growth plumbing
  (`setDrawCount` through the spatial core, buffer swaps fanned to view
  entries) already covered view entries, so breathing populations and
  capacity growth cast correctly with no extra wiring.
- A custom `overrideMaterial` view gets the same rule for free: it
  draws instanced meshes when the override itself declares their exact
  record layout, and skips them otherwise.
- `<InstancedMesh>` gained the `castShadow` prop (setCastShadow as a
  prop, like Mesh).

`examples/instanced.tsx` casts both fleets (its class declares the
7-float placement's shadowVertex); verified live - rock and pine
shadows track the spinning group, appear and vanish with
setInstanceCount, and the plain/spot caster paths regressed clean.
