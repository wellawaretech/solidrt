---
title: Cube map textures
description: No TEXTURE_CUBE_MAP support anywhere (upload, sampling, or render target), so environment/reflection mapping, skyboxes and cube shadow maps have no path; ES 3.0 has cube maps in core, seamless filtering included. Demand-gated on the scene-graph environment tier.
created: 2026-08-04
---

# Cube map textures

Nothing in the stack creates or binds a cube map: uploads and targets are
2D `rgba8`/`r8` only, and a `samplerCube` uniform falls out of the
typed-uniform table as an unsupported kind. That closes off the standard
environment tier of 3D rendering - skyboxes, reflection/environment
mapping (and image-based lighting with it), and cube shadow maps for
point lights. ES 3.0 has the whole feature in core, including seamless
cube filtering.

Shape questions to settle when a consumer arrives, in rough order:

- **Currency.** A cube map wants to be an ordinary `TextureId` (the
  universal-currency rule), but it is sampling-only: the `texture`
  element cannot display one and `readTexture`/`copyTexture` are
  2D-shaped. Either the id carries a flavor that display and readback
  sites reject with a clear error (the `format: "r8"` precedent - format
  is id state), or cube maps get their own brand. The flavor route fits
  the existing pattern better.
- **Creation.** Six same-size square faces in one call (a `faces` option
  on the create, or a dedicated `createCubeTexture`); per-face re-upload
  only if a use case asks.
- **Sampling.** `samplerCube` joins the uniform-kind table; the
  `textures` binding map stays name -> id, with the reflected sampler
  type picking the bind target.
- **Render-to-face** (dynamic environment probes, cube shadow maps):
  defer - it multiplies the target model by six, and cube shadows also
  want sampleable depth (extensions file). Static uploads first.
- **Mipmaps.** Environment maps alias badly without minification control
  and IBL sampling wants the mip chain; this item leans on
  [gpu-mipmaps](gpu-mipmaps.md) landing first or together.

Filed 2026-08-04 from ../notes/scene-graph-3d.md (stage 5,
demand-gated); no field report asks yet.
