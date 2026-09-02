---
title: Half-float texture format (rgba16f)
description: HDR environment maps and IBL run on filterable half float in every engine, but the float formats here are 32-bit and nearest-only, so an HDR cube map or panorama cannot be linearly filtered or mip-mapped; rgba16f is filterable and mip-mappable in core GLES 3.0 and is one new value in the format vocabulary.
created: 2026-09-02
---

# Half-float texture format (rgba16f)

From the environment-tier comparison ([3d-environment](3d-environment.md)):
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
