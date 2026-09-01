---
title: Compressed texture uploads (ETC2)
description: ES 3.0 mandates ETC2/EAC in core, a free 4-8x texture memory cut on every GL target, but createTexture is RGBA8-only, so a 25-map glTF scene (Sponza) holds ~182 MB of texture for ~18 MB of source; with the honest caveat that ANGLE on Windows may software-expand it - the same split that made both web standards gate the feature.
created: 2026-07-30
---

# Compressed texture uploads (ETC2)

From [gpu-review](../notes/gpu-review.md) (lesson 17). OpenGL ES 3.0
requires ETC2/EAC in core, so every GPU at alloy's minimum spec decompresses
it in the sampler for free - 4-8x less texture memory than RGBA8. Both web
standards expose compressed formats but gate them (WebGL2 behind an
extension, WebGPU behind a feature) for a reason that applies here too:
desktop-emulation backends - ANGLE over D3D, the Windows path - lack the
hardware format and must transcode or decline. `createTexture`/
`uploadTexture` are RGBA8-only today, so a game-scale texture set pays the
full multiple everywhere.

Shape when demanded: `"etc2-rgba8"` as a value of the existing `format`
option on createTexture - the vocabulary
[gpu-float-texture-formats](../done/gpu-float-texture-formats.md)
established already documents it as reserved, and
`TextureFormat::byte_len(w, h)` in alloy is the one sizing seam block
compression changes - raw compressed bytes in, `glCompressedTexImage2D`
under it; `Flux.capabilities` (or [[gpu-labels-limits]]'s limits object)
answers whether the device takes it natively. Mutable/resize paths and the
`<texture>` display draw are unaffected (a texture id is a texture id).

The platform caveat is the reason this is filed with care: guaranteed
native on the GL targets (Linux, Android), possibly software-expanded
under ANGLE on Windows.

## Field report: Sponza (2026-08-28)

The workload has appeared. The Khronos glTF sample Sponza (25 materials,
69 images of which the parser opens the 25 base-color maps, 1024^2 to
2048^2 JPEGs, ~18 MB of source) lands as 136.5 MB of RGBA8 base level and
~182 MB with the mip chain; client RSS sat at ~235 MB. Nothing broke and
the scene ran at 60 fps, but a texture-heavy model has no lever other than
shipping smaller images: no `format` option on createTexture, no KTX2 /
basis path. [[3d-model-loader]] lists KTX2 images among the compressed
real-world glTF inputs still open; that is the bake-side half of this
item (transcode at `srt tool 3d/model` time, upload compressed at load),
and the runtime half is the `format` option above. Measured on Windows
(ANGLE / D3D11, RTX 3070), so the very platform where ETC2 may be
software-expanded; a desktop format (BC7 / S3TC via extension) or a
basis-universal transcode to whatever the device reports is the shape that
holds on both sides of the split.
