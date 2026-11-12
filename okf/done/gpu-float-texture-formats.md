---
title: Float texture formats (R32F/RGBA32F)
description: "Done 2026-09-01: \"r32f\"/\"rgba32f\" joined the createTexture format vocabulary - Float32Array payload, ES 3.0 core upload and texelFetch sampling, nearest-only (linear/mipmap/anisotropy throw), no readback or copy (float is not color-renderable in core). byte_len(w, h) on TextureFormat is the sizing seam a future compressed format changes; \"etc2-rgba8\" and \"rgba8-srgb\" are documented reserved values of the same vocabulary."
created: 2026-08-11
completed: 2026-09-01
---

# Float texture formats

Symptom was: a shader that needs float data as a texture - per-sector
heights fetched in the vertex stage via `texelFetch`, bone matrices at
scale, any lookup table wider than 8 bits - had no direct path, because
`createTexture` uploaded byte formats only. The workaround was fixed-point
encoding into RGBA8 channels and a decode in the shader: it worked, but
cost precision, shader boilerplate, and (for filtering) correctness, since
filtering interpolates the encoded bytes, not the values. This is also the
designated overflow path for uniform data too large for uniform arrays
([gpu-uniform-arrays](gpu-uniform-arrays.md): a few hundred
vec4-equivalents per stage is the ES 3.0 floor).

What landed: `"r32f"` and `"rgba32f"` as values of the existing `format`
option on `createTexture`/`createMutableTexture`, fixed for the id's
lifetime like the sampler state.

- Payload is a `Float32Array`, and the view type must match the format
  everywhere pixels cross the boundary (byte formats require a
  `Uint8Array`), so a reinterpret slip throws at the call site instead of
  uploading garbage. `uploadTexture`/`resizeTexture` resolve the required
  view from the id's format.
- Upload and `texelFetch` sampling are ES 3.0 core (`R32F`/`RGBA32F`,
  `GL_FLOAT`). Sampling is nearest-only, the portable contract: linear
  float filtering needs `OES_texture_float_linear` and RGBA32F is never
  filterable in core, so `filter` defaults to `"nearest"` and `"linear"`,
  `mipmap` and `anisotropy` throw (`SamplerState::parse_for` in alloy owns
  the invariant). The GL completeness-fallback filter is NEAREST like a
  depth texture's.
- No readback or copy: `readTexture` and `copyTexture` (as source) throw -
  float is not color-renderable in core GLES 3.0, so no portable FBO path
  exists. Float textures are upload-and-sample data textures; displaying
  one via `<texture src>` is documented as out of contract.
- `TextureFormat::byte_len(width, height)` replaced per-pixel sizing at
  every validation site: the one seam a future block-compressed format
  changes.

The vocabulary is shared forward: `"etc2-rgba8"`
([gpu-compressed-textures](../backlog/gpu-compressed-textures.md)) and
`"rgba8-srgb"` (linear-space rendering, per
[gpu-pixel-contract-docs](gpu-pixel-contract-docs.md)) are documented as
reserved values of the same option - grammar: base layout plus a qualifier
suffix - so they slot in without an API rethink.

History: deferred bullet of [gpu-pipeline-extensions](gpu-pipeline-extensions.md)
since 2026-07-15; split out 2026-08-11; landed 2026-09-01.
