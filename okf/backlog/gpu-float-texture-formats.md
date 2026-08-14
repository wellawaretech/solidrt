---
title: Float texture formats (R32F/RGBA32F)
description: Data textures are RGBA8-only, so any float payload sampled in a shader (per-sector heights via texelFetch, bone matrices at scale) must be fixed-point encoded into RGBA8 channels and decoded in the shader; R32F/RGBA32F upload formats close it. Split from gpu-pipeline-extensions 2026-08-11.
created: 2026-08-11
---

# Float texture formats

Symptom: a shader that needs float data as a texture - per-sector heights
fetched in the vertex stage via `texelFetch`, bone matrices at scale, any
lookup table wider than 8 bits - has no direct path, because `createTexture`
uploads RGBA8 only. The workaround is fixed-point encoding into RGBA8
channels and a decode in the shader: it works, but costs precision, shader
boilerplate, and (for filtering) correctness, since filtering interpolates
the encoded bytes, not the values.

The fix is a format vocabulary on texture creation: `R32F`/`RGBA32F` (ES
3.0 core for upload and `texelFetch` sampling; linear filtering of float
textures needs `OES_texture_float_linear`, so nearest/`texelFetch` is the
portable contract). This is also the designated overflow path for uniform
data too large for uniform arrays ([gpu-uniform-arrays](../done/gpu-uniform-arrays.md),
done: a few hundred vec4-equivalents per stage is the ES 3.0 floor).

Related: the target pixel contract documentation
([gpu-pixel-contract-docs](../done/gpu-pixel-contract-docs.md)) notes that a format
vocabulary is what would make linear-space rendering expressible; device
formats beyond the WebGL2 floor are surveyed in
../notes/3d-differentiators.md.

History: deferred bullet of [gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md)
since 2026-07-15; split out 2026-08-11.
