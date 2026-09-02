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
  [gpu-mipmaps](../done/gpu-mipmaps.md) (landed 2026-08-23).

Shape decided 2026-09-02 (proposal review while scoping spot lights;
noted so a consumer does not re-derive it):

- **Currency**: ordinary `TextureId`; cube-ness is id state like
  format, sampling-only (`<texture>` display, `readTexture`,
  `copyTexture` reject at the call site - the depth-id precedent).
- **Creation**: dedicated `createCubeTexture(faces, size, opts?)` - six
  buffers in GL order +X,-X,+Y,-Y,+Z,-Z, square, the existing format
  vocabulary (float faces are the HDR/IBL upload path - float is
  sampling-only anyway, which a cube map already is), `mipmap` as id
  state, `wrap` accepted-and-ignored (ES 3.0 cube filtering is always
  seamless).
- **Sampling**: `UniformKind::SamplerCube` joins the table; bindings
  stay name -> id, the reflected kind picks the bind target; cross-shape
  misbinds throw both ways (route the fused creates through the shared
  binding validator - the "second rule" depth-ids-views-shadows
  anticipated).
- **Render-to-face** does NOT multiply the target model by six: one
  `createCubeDrawTarget(size, params, opts)` with one entry list, and
  the face is a render-time argument (`renderTarget(cube, face)`);
  `depth: true` is one renderbuffer reused across the six face passes.
  Reject `samples >= 2`, `mipmap`, `depth: "texture"` initially.

Point shadows dropped out as a consumer (2026-09-02): the
Three/Godot/URP comparison put them on six face tiles in the existing
shadow atlas instead - library-only, hardware PCF retained, no
sampler-array cap (reasoning recorded in
[3d-point-light-shadows](../done/3d-point-light-shadows.md), landed
2026-09-02). That removes the
driving consumer for render-to-face; static uploads plus `samplerCube`
sampling serve the environment tier, and render-to-face waits for
dynamic environment probes. Worth noting for the tier's own shape pass:
Three's IBL pipeline (PMREM) packs into a 2D texture and equirect
panoramas are the common interchange, so even the environment tier
should get a fresh Three/Godot/Unity comparison before this builds.

Filed 2026-08-04 from ../notes/scene-graph-3d.md (stage 5,
demand-gated); no field report asks yet. The spot half of the old
spot-and-point shadow item landed 2026-09-02 without cube maps; the
remaining consumer is the environment tier.
