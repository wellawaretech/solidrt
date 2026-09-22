---
title: Environment tier leftovers - SH9, aoMap, packed .srte, EXR, loadCubeImages
description: The environment tier is complete (skybox, HDR environments, PBR, prefiltered HDR probes and sky bakes); what Three, Unity and Godot ship on top of the same tier and we do not yet - SH9 irradiance, ambient occlusion maps, EXR input, six-face cube image sets - plus the tier's own leftovers, a smaller environment file, a per-probe format, half-float readback and the probe cost on the low-end devices.
created: 2026-09-06
---

# Environment tier leftovers

Everything here was decided additive during
[3d-environment](../done/3d-environment.md), nothing changes a shipped
shape. SH9, `aoMap`, EXR input and `loadCubeImages` are parity gaps,
each tagged with the engines that have it; the rest are the tier's own
engineering leftovers.

- **SH9 irradiance** (Three's LightProbe form). `standard` takes the
  image-lit diffuse from the chain's fully rough level (`envIrradiance`,
  Three getIBLIrradiance / Godot max LOD), which is a texture fetch per
  fragment; nine coefficients are a uniform. Cheaper on the TV and the
  Pi, and the bake tool already has the cube to integrate.
- **`aoMap`** on `standard` and `lit`: a baked occlusion map scaling the
  ambient and environment terms, glTF's occlusion texture (red channel;
  a third use of the packed metal-rough-occlusion image). Three's
  `aoMap`, Unity's Occlusion map, Godot's `ao_texture`: all three.
- **Packed .srte payload**: float32 rgba faces today, 2 MiB at 128. RGBE
  or half packing is 4x smaller; the decoder side is one branch on a
  header field.
- **EXR input** to `srt tool 3d/environment`: only Radiance .hdr decodes
  today; all three engines import EXR (Three's EXRLoader, Unity and
  Godot at import), and EXR is what most HDRI sites ship first.
- **`loadCubeImages`**: a Three-style six-face image set is seen from
  inside and must be mirrored per image at load; the helper does that
  once so app code never learns the convention. Three's
  CubeTextureLoader, Unity's and Godot's six-image cubemap import.
- **Per-probe format override**: probes follow `bufferFormat()` (half
  float where renderable). Unity's per-probe `hdr` toggle is a one-line
  pass-through of the cube draw target's `format` if a probe ever needs
  to be 8-bit on purpose.
- **Half-float readback**: rgba16f draw targets are sampler-only; a
  Float32Array readback through the half-float color-buffer extension
  would let a probe script check HDR values directly instead of through a
  scaled sampling pass, and let a snapshot tool tone map them.
- **Probe cost on Android, the TV and the Pi**: the fixed ~0.3 ms per
  pass that samples cube mip levels > 0 was measured on Intel/Mesa only.
