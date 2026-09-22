---
title: Model file: bake from own geometry, extras, part reuse
description: A procedural model cannot be written to .srtm without importing the runtime (the only export path pulls flux:*), the container has no slot for app data, and a part belongs to exactly one node so a variant placed hundreds of times cannot be expressed; three bounded changes to the model file and createModel, from a demo that baked its own geometry.
created: 2026-09-22
completed: 2026-09-22
---

# Model file: bake from own geometry, extras, part reuse

Context: `packages/3d/src/model-file.ts` (`encodeModel`/`decodeModel`,
the `.srtm` container), `gltf.ts` (`ModelData` and `parseGltf`),
`tools/model.ts` (`srt tool 3d/model`, the glTF bake under bun) and
`model.ts` (`createModel`). Open loader work that is not this is in
[3d-model-loader](3d-model-loader.md). The three items below came out of
one app that generated hundreds of thousands of vertices at runtime,
found the startup cost, and baked them instead; each is what the bake
path lacked. Do them in this order: the first two are small and the
third builds on the container bump the second makes.

## 1. A runtime-free entry point for writing .srtm

Symptom: `encodeModel` is exported from `@solidrt/3d`, whose entry
imports the runtime (`flux:*`) and so does not load under bun. A bake
script has to import `node_modules/@solidrt/3d/src/model-file.ts` by
path, which breaks silently on a restructure.

The chain is already pure: `model-file.ts`, `gltf.ts` and `geometry.ts`
import `@solidrt/core/gpu` for types only, and `math.ts`/`color.ts`
import nothing. `tools/model.ts` runs under bun on exactly these files.

Done looks like:

- `package.json` exports `"./model"`, a small entry re-exporting
  `parseGltf`, `gltfExternalUris`, `isGlb`, `encodeModel`,
  `decodeModel`, the `Model*` types, and the pure geometry pieces a bake
  wants (`layoutStride`, `vertexView`, the generators and `withAttribute`
  if they are pure - check `geometry.ts` for any runtime import first).
- A check under bun (`packages/3d/checks/`) that imports the entry with
  no `flux:*` shim and round-trips a two-part `ModelData` through
  encode/decode, so a runtime import creeping in fails the check.
- `tools/model.ts` imports the same entry, so the tool and apps share
  one path.
- AGENTS.md "Models": one paragraph, "bake your own geometry", with the
  import line.

Not this: teaching `srt tool 3d/model` to run a JS module that returns
`ModelData`. The export gives an app the same thing in its own script.

## 2. An extras slot in the model data

Symptom: a bake that must ship app data beside the meshes (an
occupancy grid, a set of collider boxes) has nowhere to put it: the
grid went into a second file, the colliders became nodes named
`collider` with half-extents smuggled through `scale`.

glTF has `extras` (JSON) on the root and on every node, and that is the
shape to mirror:

- `ModelData.extras?: Record<string, unknown>` and `ModelNode.extras?:
  Record<string, unknown>`: JSON, written into the container's header
  chunk. `parseGltf` carries the file's own `extras` through, so a
  collider box authored in Blender as a custom property arrives.
- `ModelData.blobs?: Record<string, Uint8Array>`: named binary
  sections for what does not belong in JSON (a grid, a nav mesh), each
  its own aligned chunk.
- Container VERSION bump; `decodeModel` of an older file yields empty
  extras and blobs. `createModel` exposes `model.extras`,
  `model.nodes(name).extras` (or the node table), and `model.blobs`.
- One test in the model-file check: encode with extras on the root and
  a node plus one blob, decode, deep-equal.

## 3. One part under many nodes

Symptom: `.srtm` ties a part to exactly one node, so placing a handful
of variants hundreds of times meant one named node per placement, then
`decodeModel` in the app and hand-built meshes, because
`createModel` cannot express the placement. glTF can: one mesh
referenced by many nodes, and `EXT_mesh_gpu_instancing`.

Shape:

- `ModelPart.node` stays the first placement; `ModelPart.placements?:
  number[]` lists the other nodes that place the same geometry.
  `parseGltf` folds a mesh referenced by several nodes into one part
  with placements, and reads `EXT_mesh_gpu_instancing` matrices as
  placement nodes (synthesized children of the referencing node, so
  the hierarchy stays one table). The container writes the list.
- `createModel` builds a part with placements as an InstancedMesh (one
  draw entry, the shared geometry, the part's material) whose instances
  are the placement nodes themselves, so they ride the hierarchy,
  animate with a clip that targets them, pick and cull like any
  instance (the instance-citizenship rules in AGENTS.md). A part with
  no placements stays a plain mesh: identity per part is unchanged.
- `model.nodes(name)` still moves a placement; a placement is an
  instance node, so the instance-vs-mesh difference only shows for
  per-part material swaps, which apply to the whole part (say so).
- A `--merge`-style bake flag is a different lever (one draw per
  material for static scenes) and stays in the loader item.

Verify with a glTF that reuses a mesh (Blender's linked duplicates
export that way) through `srt tool 3d/model`, then `createModel`: one
entry in `/gpu` for the shared part, `instanceCount` = placements + 1,
and a pick on any copy names the part.

## Done (2026-09-22)

All three shipped, against the shape above with these decisions:

- The entry is `@solidrt/3d/model` (`src/model-data.ts`): the parser,
  the container, the whole pure geometry kit (generators, `withAttribute`,
  merge/transform, profiles, sweeps), color and math. `tools/model.ts`
  imports it, `tests/model-data.test.ts` runs it under `bun test` (the
  existing bun-test convention rather than a flux check) and resolves the
  published subpath too.
- `extras` follow glTF and Three's userData: root, node, mesh (onto the
  part) and material, present only when non-empty; `blobs` are named
  4-aligned blocks. Container VERSION 10 rejects 9 like every earlier
  bump (the item's "older file yields empty extras" reading was dropped:
  one policy, re-bake).
- Placements: `parseGltf` folds a mesh several nodes reference and reads
  `EXT_mesh_gpu_instancing` as synthesized children `<node>[i]`. Not
  folded, each its own part per node: skinned, morphed, and a placement
  whose rest-pose determinant sign differs from the bucket's (the winding
  flip is baked into the indices). `createModel` refuses a hand-built
  skinned or morphed part with placements.
- The instances are NOT the placement nodes themselves: a multi-primitive
  mesh gives several parts per placement and an instance belongs to one
  mesh, so the placement stays a Group and each shared part adds an
  instance at identity under it (`part.instances`). Moving, hiding and
  animating the placement node moves the copy; a pick reports the part's
  mesh and the instance, whose parent is the placement node.
- The InstancedMesh hangs at identity under the model root and is anchored
  there: `createInstancedMesh` gained `anchor?` (an ancestor the mesh sits
  under at identity; instances may live anywhere in its subtree; checked
  at scene add). The scene binds records against the anchor and a mesh
  entering after its instances binds the ones waiting. The instanced LOD
  already lived under this contract; it is now nameable.
- `ModelOptions.material` gained a sixth flag, `instanced`.
- `bindSkeleton`'s graft skips instance children (slot-bound; they stay
  with their placement node).
- `*.srtm` joined the binary-import module declarations in core.

Verified: `gltf-check.ts` (a reuse fixture with a mirrored copy, an
instancing node, extras and blobs; bun and flux), `model-data.test.ts`,
and live through `srt tool 3d/model` on a generated glb
(`probes/model-reuse/`): one entry with `instanceCount` 6 in `/gpu`,
the record buffer holding the composed placements, a placement move and
a hide landing in the records, and taps naming the part and the
placement node per copy.
