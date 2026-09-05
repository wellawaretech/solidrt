---
title: Half-float and sRGB texture formats (rgba16f, rgba8-srgb)
description: "Done 2026-09-03: \"rgba16f\" (Float32Array payload packed to half on upload, filterable, mip chain gated on the device's half-float renderability) and \"rgba8-srgb\" (SRGB8_ALPHA8, hardware decode on sample) joined the format vocabulary as the HDR image and the color-map formats of the 3d color pipeline; both upload-and-sample only."
created: 2026-09-02
completed: 2026-09-03
---

# Half-float and sRGB texture formats (rgba16f, rgba8-srgb)

What landed (stage 3a of [3d-environment](../done/3d-environment.md),
verified by `probes/hdr-format-probe.tsx`):

- `"rgba16f"` on createTexture / createCubeTexture / createMutableTexture:
  the same Float32Array payload as the 32-bit formats, packed to f16 at
  the flux boundary (`TextureFormat::f16_bytes`, the `half` crate), so
  alloy sees every payload at its stored `byte_len`. Stored RGBA16F /
  HALF_FLOAT. Linear filter by default, anisotropy allowed:
  `TextureFormat::filterable` split off `is_float` (32-bit float stays
  nearest-only, 16f and every byte format filter). One correction to the
  shape below: `glGenerateMipmap` requires a color-renderable format and
  half float is renderable only through EXT_color_buffer_half_float (or
  the float one), so `mipmap: true` on rgba16f is gated on
  `GpuLimits::half_float_renderable` (queried from the extension string,
  exported as `limits.halfFloatRenderable`) and throws where absent.
- `"rgba8-srgb"`: SRGB8_ALPHA8 storage, Uint8Array payload, everything
  else as rgba8 - the hardware decodes on sample, before filtering, so
  linear filter and mip chain are correct; alpha is not decoded.
- Both are upload-and-sample only (`TextureFormat::sample_only`): float
  is not color-renderable in core, and an sRGB readback would go through
  Impeller's sampling path and return decoded values, not the stored
  bytes. readTexture and copyTexture refuse both; `<texture src>` display
  is out of contract.
- The first consumer is the 3d color pipeline (linear lighting, item 17
  of the 3d roadmap): color maps are rgba8-srgb, HDR environments
  rgba16f.

The shape as decided before it landed:

From the environment-tier comparison ([3d-environment](../done/3d-environment.md)):
Three's PMREM default is `HalfFloatType`, Unity's and Godot's HDR sky and
reflection storage is half float. GLES 3.0 lists RGBA16F as
texture-filterable (and mip-mappable through `glGenerateMipmap`), while
RGBA32F is not filterable in core - which is exactly why the existing
float contract (`r32f`/`rgba32f`, see `TextureFormat::is_float`) is
nearest-only and `texelFetch`-shaped. Half float is the missing HDR
sampling format, not a variant of the data-texture formats.

Shape (additive, decided in the comparison):

- `format: "rgba16f"` (and `r16f` if a consumer wants it) on
  `createTexture`, `createMutableTexture` and `createCubeTexture`; JS
  supplies a `Float32Array` like the 32f formats and the upload converts
  to f16 (the `half` crate is already in the tree), so the pixel contract
  stays "one float per component in JS".
- Linear filter allowed (the default), `mipmap` and `anisotropy` allowed;
  `SamplerState::parse_for` gets a third branch beside the 32f one.
- Upload-and-sample only, like 32f: half float is not color-renderable in
  core (EXT_color_buffer_half_float), so no readback or copy path, and no
  half-float targets until that extension is deliberately adopted.
- Lands with roadmap item 17 (PBR and the color-space decision), which is
  the first consumer; nothing in stage 1 of the environment tier needs it.
