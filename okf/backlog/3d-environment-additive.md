---
title: Environment tier leftovers - SH9, aoMap, packed .srte, EXR, loadCubeImages
description: The environment tier is complete (skybox, HDR environments, PBR, prefiltered HDR probes and sky bakes) and each of these is a deliberate non-goal of that work that a consumer would ask for next - an image-lit diffuse cheaper than the chain's rough level, ambient occlusion maps, a smaller environment file, EXR input, Three-style face sets, a per-probe format, half-float readback, and the probe cost on the low-end devices.
created: 2026-09-06
---

# Environment tier leftovers

Everything here was decided additive during
[3d-environment](../done/3d-environment.md) and none of it blocks a
consumer today. Pick up when one asks.

- **SH9 irradiance** (Three's LightProbe form). `standard` takes the
  image-lit diffuse from the chain's fully rough level (`envIrradiance`,
  Three getIBLIrradiance / Godot max LOD), which is a texture fetch per
  fragment; nine coefficients are a uniform. Cheaper on the TV and the
  Pi, and the bake tool already has the cube to integrate.
- **`aoMap`** on `standard` and `lit`: a baked occlusion map scaling the
  ambient and environment terms, glTF's occlusion texture (red channel;
  a third use of the packed metal-rough-occlusion image).
- **Packed .srte payload**: float32 rgba faces today, 2 MiB at 128. RGBE
  or half packing is 4x smaller; the decoder side is one branch on a
  header field.
- **EXR input** to `srt tool 3d/environment`: only Radiance .hdr decodes
  today.
- **`loadCubeImages`**: a Three-style six-face image set is seen from
  inside and must be mirrored per image at load; the helper does that
  once so app code never learns the convention.
- **Per-probe format override**: probes follow `probeFormat()` (half
  float where renderable). Unity's per-probe `hdr` toggle is a one-line
  pass-through of the cube draw target's `format` if a probe ever needs
  to be 8-bit on purpose.
- **Half-float readback**: rgba16f draw targets are sampler-only; a
  Float32Array readback through the half-float color-buffer extension
  would let a probe script check HDR values directly instead of through a
  scaled sampling pass, and let a snapshot tool tone map them.
- **Probe cost on Android, the TV and the Pi**: the fixed ~0.3 ms per
  pass that samples cube mip levels > 0 was measured on Intel/Mesa only.
