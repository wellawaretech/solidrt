---
title: Morph targets - sparse-by-vertex deltas, weights as palette rows, clip and transition driven
description: The loader drops primitive targets and the "weights" channel and throws on sparse accessors, so a face or a blend-shape character cannot even parse; add morph targets as a geometry target list packed sparse by vertex into a float texture, with weights a node register the spatial core publishes as a row of a weights texture through the palette sink, driven by clip players, node transitions and setMorphWeights, per instance included.
created: 2026-09-11
completed: 2026-09-11
---

# Morph targets

The open half of roadmap item 16 ([3d-roadmap](../notes/3d-roadmap.md)).
Skinning is done: palettes composed in the spatial core, sampled from an
rgba32f texture, joints driven by the clip players with zero per-frame
JS ([animation-core](../done/animation-core.md)). Morph targets were left
out of every layer.

## Symptom

`parseGltf` skips primitive `targets`, skips the `weights` animation
channel and throws on sparse accessors - which Blender uses for nearly
every morph export, so a blend-shape asset does not parse at all. There
is no morph vocabulary in the geometry, the container, the materials, the
core or the mixer. Per-vertex JS is ruled out by the interpreter, so the
whole feature has to be data plus GPU plus core, like skinning.

## Shape

Deltas only (the glTF form; Three's absolute morph targets are not
offered), applied in the vertex stage before the skin matrix, with
weights owned by a NODE (glTF's `node.weights`), so a face split into
skin, eyes and teeth parts takes one write and one clip channel.

**1. Data model.** `Geometry.targets?: MorphTarget[]`, each `{ name,
position: Float32Array, normal?: Float32Array }` of per-vertex deltas.
The geometry's bounds grow by the union of the per-target delta boxes
(a wide-open mouth is never frustum-culled). `.srtm` version 8: a
targets block per part (names plus the sparse-by-vertex packing below,
already packed so loading is a view), the mesh's default weights, and
clip channels with a `weights` path. The bake tool writes them.

**2. Loader.** Primitive `targets` (POSITION and NORMAL; TANGENT dropped
like tangents), `mesh.weights` defaults, `mesh.extras.targetNames`,
`node.weights` overrides, the `weights` channel (one float per target per
key; cubic three per key). Sparse accessors, a prerequisite. A part's
targets ride the same un-index (flat shading) and index rewrites as its
vertices.

**3. GPU storage: sparse by vertex, not a dense delta texture.** Most
targets touch a small region of the mesh, and a dense
vertex x target layout fills a 2048x2048 rgba32f texture at about 100
targets on a 20k-vertex face; production faces have 50 to 300. The
packing is one rgba32f texture per geometry, owned by its GPU buffers
and bound as `uMorphs`: a header texel per vertex (`offset, count`),
then per vertex a run of entries `(target index, position delta,
normal delta)` for the targets that displace it. Memory is proportional
to the real deltas, the ceiling is gone, and the vertex stage loops over
that vertex's few entries (`gl_VertexID`), not every target.

**4. Weights: a texture row through the palette sink, not a uniform.**
The core's texture-slot sink already makes "a node's state becomes one
row of a float texture, one upload per texture per flush". It grows a
second row kind: the node's weights register (four weights per texel)
instead of `nodeWorld x post`. A node gains `weights: Vec<f32>`,
written by `set_weights` (JS), by the clip players (a fourth
`ChannelPath::Weights` with a variable element count, blended with the
same incremental weighted average, so crossfades just work) and by the
node transitions (a weights lane beside position/rotation/scale, so a
smile springs in from `setMorphWeights` with a transition spec and
`from`/`exit` endpoints apply). One integer uniform `uMorphRow` names a
plain mesh's row. flux:spatial: `setWeights`, `readWeights`, the slot
bind's row kind, path code 3 in the packed clip; flux-types parity.

**5. Materials.** A `morph: true` option on lit, unlit, standard and the
shadow-depth stages, mirroring `skinned`: MORPH_DECLS and MORPH_APPLY
from `@solidrt/3d/glsl` splice in before SKIN_MATRIX; `Material.morphed`
is read from the source (declares `uMorphs`) so shadow and override
entries merge the mesh's morph bindings the way they merge `uBones`. A
caster casts its morphed shape.

**6. Per instance.** An instanced mesh morphs per instance: the
population owns one weights texture with a row per record slot, each
instance node owns a register published into its slot's row, and the
shader reads the row as `uMorphRow + gl_InstanceID` - the record index
IS the row, so no style attribute and no record change (the shape as
first drafted carried the row in a style attribute; the instance index
is already there). The texture grows with the population's capacity
(the entries re-point). Skinned plus morphed plus instanced composes
with no special case (Three's InstancedMesh morph support). A record
mesh has no nodes to own weights and is rejected.

**7. Library API.**

```ts
setMorphWeights(node, { smile: 0.7, blink: 1 })  // by name, partial merge; or an array
setMorphWeights(node, weights, { duration: 300 }) // through the transition lane
getMorphWeights(node)                             // reads the core, like getTransform
```

For a model the owner is the glTF node's group (`createModel` points the
parts' entries at it and applies the default weights once); a standalone
mesh owns its own. Names come from the geometry's target list. The mixer
plays weights channels like TRS channels. A `morphWeights` prop on the
Mesh component covers the declarative case.

## Contract

Morphing is a vertex-stage effect exactly like skinning: picking and the
transparent sort key use the base shape, shadows morph, the retained
tree sees the base pose. rgba32f deltas (an rgba16f form is additive if
a consumer needs the halving).

## Done looks like

Khronos AnimatedMorphCube and AnimatedMorphSphere under `external/`
parse, bake, load and animate; the gltf check rig covers sparse
accessors, targets and the weights channel (its morph section is the
morph check); the `morph.tsx` example's debug command freezes with the
clock, reads the registers back and snapshots; `packages/3d/AGENTS.md`
documents the API and the traps; the morph bullet in
[3d-model-loader](3d-model-loader.md) moved here.

## Outcome

All four stages landed 2026-09-11 as shaped, with one change in stage 4
(above: the instance index is the row). The core tests cover the weights
channel blend, the transition lane's enter/write/exit and the one-off
write motion; the flux decoder takes any lane count for the weights
entry. Weights rows are rgba32f and cannot be read back through the
texture endpoint, so headless verification reads the register through
`getMorphWeights` and the picture through a snapshot.

## Stages, as landed

1. Data model, container, loader (sparse accessors, targets, weights
   channel), bake tool - verifiable by the check rig alone.
2. GPU packing and materials with weights written by `setMorphWeights`
   through the core register and the palette-sink row kind.
3. Clip players' weights path and the mixer; the node-transition lane,
   plus a one-off motion on a write (`setMorphWeights(node, w, spec)`).
4. Per-instance rows by the instance index; docs; the example's
   headless debug command.
