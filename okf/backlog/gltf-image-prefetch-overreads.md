---
title: loadGltf reads every image a .gltf names, not the ones the parser opens
description: parseGltf's resolver is synchronous so loadGltf prefetches everything gltfExternalUris lists, which is all of gltf.images, while the parser only ever opens an image through the baseColorTexture branch - so on a fully textured model the normal, metallic-roughness, occlusion and emissive maps are read off disk, held for the length of the parse and discarded.
created: 2026-08-27
---

# loadGltf reads every image a .gltf names, not the ones the parser opens

`parseGltf`'s `resolve` is synchronous and `flux:fs` is not, so `loadGltf`
prefetches: it asks `gltfExternalUris` what the document references, reads all
of it into a Map, and hands the parser a lookup into that Map.

`gltfExternalUris` lists every entry of `gltf.buffers` and `gltf.images`. The
parser only ever opens an image through `imageSlot`, and `imageSlot` is called
from exactly one place - the `baseColorTexture` branch of the material map
(`packages/3d/src/gltf.ts`). Every other channel the file names is read,
retained for the length of the parse, and thrown away. On a fully textured
model that is most of the bytes: a scene authored with base color, normal and
metallic-roughness maps names roughly three times as many images as the parser
will open, at whatever resolution the source ships.

The bake tool does not have the problem. `srt tool 3d/model` passes a lazy
`(uri) => readFileSync(...)` resolver, so it reads exactly what the parser
asks for. Only the async path over-reads, and it over-reads because the
prefetch has to guess the demand set in advance.

## Done

`loadGltf` reads the .bin plus the images a material actually samples, and
nothing else.

## Shape

Narrow the uri list to the images the parser will open: walk
`materials[].pbrMetallicRoughness.baseColorTexture` to `textures[].source` to
`images[]`, emit those plus every buffer. It is the same predicate `imageSlot`
applies, computable from the JSON without touching geometry.

`gltfExternalUris` is exported and documented as "the external uris a .gltf
document references", so narrowing it in place is a contract change. It is
probably the right one, since prefetching for `parseGltf` is the only reason
it exists and the doc comment says so, but decide it deliberately rather than
by accident: the alternative is a second function and leaving the general one
alone.

The trap worth writing down alongside the fix: the prefetch list and the
parser's demand set become two implementations of one predicate, and they
diverge silently the moment the parser learns another texture channel. Normal
maps are the next thing that would widen it, and the failure mode then is not
a wasted read but a "references the external file X and no resolver was
given" throw at parse time, from a file that is perfectly valid. Either derive
both from one function, or have `gltf-check.ts` assert the two agree on a
fixture carrying a non-base-color texture.
