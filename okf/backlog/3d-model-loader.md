---
title: Model loader follow-ups
description: The glTF subset loader (roadmap item 7, shipped 2026-08-26 as parseGltf/createModel at runtime plus the srt tool 3d/model bake) covers uncompressed triangles with base color; still open are the compressed real-world files (Draco/meshopt, KTX2), a retained node hierarchy, merge-by-material, a cull option for double-sided materials, an alpha test for MASK materials (which ModelMaterial does not even report) and vertex colors, each demand-gated.
created: 2026-08-26
---

# Model loader follow-ups

What shipped (documented in `packages/3d/AGENTS.md`, "Models"): a pure
glTF 2.0 subset parser (`parseGltf`: .gltf/.glb, world transforms baked,
one part per mesh node, flat normals generated, base color factor/texture,
doubleSided applied as `cull: "none"`, MASK as `alphaTest`, BLEND reported), `createModel` (a Group of meshes, one
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
- **Vertex colors, tangents, second UV set.** Dropped; the open layout
  (`withAttribute`) has the slots, the parser would emit a wider layout per
  primitive and the container already records the layout key.
- **Alpha mask.** `alphaMode: "MASK"` (with `alphaCutoff`, default 0.5)
  is common in real scenes: foliage, fences, chains, hair, any texture
  with cut-away regions, usually paired with `doubleSided`. Drawn opaque
  the cut-away texels show as solid cards, so a model with masked
  materials looks broken out of the box. Two gaps, in fix order:
  1. `ModelMaterial` drops the information: the parser reads `alphaMode`
     and keeps only `transparent = alphaMode === "BLEND"`, so the
     `material` callback of `createModel`, the documented escape hatch,
     cannot tell a MASK material from an OPAQUE one. The only workaround
     is re-reading the glTF JSON and matching materials by callback
     order, which leans on the undocumented fact that `data.materials`
     is in file order. Add `alphaMode: "OPAQUE" | "MASK" | "BLEND"` and
     `alphaCutoff` to `ModelMaterial` (keep `transparent` as the BLEND
     shorthand); document that `materials` is in file order.
  2. `lit`/`unlit` have no alpha test, so even a callback that knows
     cannot act without dropping to `shaderMaterial` and reimplementing
     the standard fragment. An `alphaTest?: number` option: one
     `discard` below the cutoff in the fragment, one more class-key
     dimension (like the double-sided cull above, which the same
     materials need). `createModel`'s default material then maps
     MASK to `alphaTest: alphaCutoff`.
  The shadow depth override is position-only, so a masked caster casts
  its whole quad; that is the fragment half of
  [3d-instanced-shadow-casters](3d-instanced-shadow-casters.md).
- **Samplers.** Every texture uploads repeat-wrapped, mipmapped and 4x
  anisotropic; per-material wrap/filter is ignored.
- **Skins and animation channels through the parse and the bake.** A
  glTF `skins` block (joints, inverse bind matrices, `JOINTS_0`/
  `WEIGHTS_0` vertex channels) and `animations` (sampler tracks, TRS
  channels targeting nodes) are ignored. [animation-core](animation-core.md)
  assumes baked track buffers arrive "from the mature loaders at pack
  time" and names no loader; this is that loader. Depends on the node
  hierarchy above (tracks target nodes) and widens the `.srtm` container
  (a version bump: joints, bind matrices, a "skinned" named layout,
  clips). Runtime `parseGltf` gets the same subset so a small rigged
  model can be imported binary like an unrigged one.
- **Runtime-fetched content.** The bake tool runs under bun on the
  developer's machine, so a model the APP downloads (user-made tracks and
  karts, a mod browser, a level editor's exports) meets the runtime
  parser as-is: Draco/meshopt/KTX2 files bounce with the clear error, and
  large ones pay the 4 us-per-vertex interleave in the interpreter.
  Either a runtime decode path for the common compressions (native, in the
  loader's Rust side - the interpreter rules out a JS Draco) or a
  documented publisher-side rule ("bake with `srt tool 3d/model` before
  upload", the `.srtm` as the exchange format). Decide when a consumer
  ships user content; do not build both.
