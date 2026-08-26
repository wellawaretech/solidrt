---
title: Model loader follow-ups
description: The glTF subset loader (roadmap item 7, shipped 2026-08-26 as parseGltf/createModel at runtime plus the srt tool 3d/model bake) covers uncompressed triangles with base color; still open are the compressed real-world files (Draco/meshopt, KTX2), a retained node hierarchy, merge-by-material, a cull option for double-sided materials and vertex colors, each demand-gated.
created: 2026-08-26
---

# Model loader follow-ups

What shipped (documented in `packages/3d/AGENTS.md`, "Models"): a pure
glTF 2.0 subset parser (`parseGltf`: .gltf/.glb, world transforms baked,
one part per mesh node, flat normals generated, base color factor/texture,
doubleSided and BLEND reported), `createModel` (a Group of meshes, one
`lit` per material, textures uploaded, `dispose()`), the read conveniences
`loadGltf`/`loadModel`, and the bake: `srt tool 3d/model` writes the same
parse as a `.srtm` container whose payload is the GPU layout.

The measurement that shaped the two-layer split, unimog (32k vertices, 21
parts, 6 PNGs), release client on Linux: `parseGltf` on flux 124 ms
against 22 ms under bun; the whole baked `loadModel` 40 ms, of which the
six PNG decodes are ~31 ms. So the interpreter's interleave-and-bake loop
is ~4 us per vertex - fine for small models, a second for a 280k-vertex
one (the body demo), which is what the bake is for. No native interleave
was added: the bake already removes the cost where it matters.

## Open, each on demand

- **Compressed meshes and textures.** Blender exports Draco by default and
  KTX2/Basis textures are common, so real-world files bounce off the
  parser with a clear error today. The place for the decoders is the bake
  tool under bun (wasm decoders, no runtime weight), which is the "mature
  loader" half of the direction in
  [3d-differentiators](../notes/3d-differentiators.md): decode there, emit
  the same `.srtm`.
- **Node hierarchy.** World transforms are baked into vertices, so a
  part cannot be moved relative to its parent node and animation/skinning
  (roadmap item 16) have nothing to drive. The additive form: local-space
  vertices plus a parent index and TRS per part in the container (a
  version bump), `createModel` composing Groups. Two demos wanted the flat
  form; the hierarchy waits for the first consumer.
- **Merge by material.** One part per node keeps identity (picking,
  per-part hide/highlight) at one draw entry per part. A `--merge` bake
  option collapsing parts by material is the roadmap's one-draw-per-material
  leverage for static scenes; `mergeGeometries` covers it at runtime
  meanwhile.
- **Double-sided materials.** `ModelMaterial.doubleSided` is reported but
  the standard materials cull back faces; unimog's glass and the body demo's
  mirrored bones both need a cull option on `lit`/`unlit` (also body
  feedback item 2). One more class-key dimension when it lands.
- **Vertex colors, tangents, second UV set.** Dropped; the open layout
  (`withAttribute`) has the slots, the parser would emit a wider layout per
  primitive and the container already records the layout key.
- **Samplers and alpha mask.** Every texture uploads repeat-wrapped and
  mipmapped; per-material wrap/filter and `alphaMode: "MASK"` (an alpha
  test in the fragment) are ignored.
