---
title: Model loader follow-ups
description: The glTF subset loader (roadmap item 7, shipped 2026-08-26 as parseGltf/createModel at runtime plus the srt tool 3d/model bake; v3 container with retained hierarchy, skins and animation clips plus the JS mixer since 2026-08-31) covers rigged models end to end; still open are the compressed real-world files (Draco/meshopt, KTX2), morph targets, merge-by-material, vertex colors, per-material samplers and runtime-fetched content, each demand-gated.
created: 2026-08-26
---

# Model loader follow-ups

What shipped (documented in `packages/3d/AGENTS.md`, "Models"): a pure
glTF 2.0 subset parser (`parseGltf`: .gltf/.glb, the node hierarchy
retained as a pruned pre-order table with node-local vertices since the
v3 container (2026-08-31; matrix nodes TRS-decomposed, winding flips
baked from the rest pose), flat normals generated, base color
factor/texture, doubleSided applied as `cull: "none"`, MASK as
`alphaTest`, BLEND reported; since 2026-08-31 skins - "skinned" layout,
joints, inverse binds - and animations as baked clips), `createModel`
(the hierarchy as nested Groups - `model.nodes` moves a named subtree -
one `lit` per material with a skinned variant per skinned part, textures
uploaded, `dispose()`), `createMixer` (crossfading clip playback over
the core evaluator of [animation-core](../done/animation-core.md); the
uBones palettes are core-composed at the spatial flush since 2026-09-02), the read conveniences
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
- **Merge by material.** One part per node keeps identity (picking,
  per-part hide/highlight) at one draw entry per part. A `--merge` bake
  option collapsing parts by material is the roadmap's one-draw-per-material
  leverage for static scenes; `mergeGeometries` covers it at runtime
  meanwhile.
- **Vertex colors, tangents, second UV set.** Dropped; the open layout
  (`withAttribute`) has the slots, the parser would emit a wider layout per
  primitive and the container already records the layout key.
- **Samplers.** Every texture uploads repeat-wrapped, mipmapped and 4x
  anisotropic; per-material wrap/filter is ignored.
- **Morph targets.** The `weights` channel path and primitive targets
  are skipped (the one animation feature left out when skins landed);
  they ride the float-texture machinery of roadmap item 16 and stay out
  of scope until a model demands them.
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
