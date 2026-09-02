---
title: Cube map textures
description: No TEXTURE_CUBE_MAP support anywhere (upload, sampling, or render target), so environment/reflection mapping, skyboxes and cube shadow maps have no path; ES 3.0 has cube maps in core, seamless filtering included. Shape confirmed by the environment-tier comparison (3d-environment.md).
created: 2026-08-04
completed: 2026-09-02
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

Environment-tier comparison done 2026-09-02
([3d-environment](3d-environment.md)), as this note asked for. It
confirms the primitive: Godot and Unity both sample a mip-chained
`samplerCube` with `textureLod`, and Three's 2D PMREM packing is its own
WebGL1 workaround (no seamless cube filtering, no cube `textureLod`
there), both of which GLES 3.0 has in core. What it adds to the stage-1
shape above:

- **Explicit mip levels, reserved.** Prefiltered radiance (Godot's
  radiance cube, Unity's convolved probe) has roughness-convolved levels
  that `glGenerateMipmap` cannot produce; the `srt` pipeline will emit
  them at build time. Stage 1 takes `faces: Buffer[6]` with the
  generated chain; the widening to `Buffer[6][]` (level 0 first, each
  level six faces, sizes halving) is additive and must not need any other
  change, so `mipmap: true` with explicit levels means "the chain is
  what you uploaded".
- **`rgba16f` follow-on.** The HDR environment format all three engines
  run IBL on is half float, and GLES 3.0 makes RGBA16F filterable and
  mip-mappable in core (RGBA32F is not, which is why the existing float
  contract is nearest-only). A new value in the shared format vocabulary
  (linear filter allowed, mipmaps allowed, upload-only), for 2D and cube
  alike; JS supplies Float32Array, the upload converts. Its own small
  item when item 17 (PBR/color space) starts, nothing in stage 1 blocks
  it.
- **Handedness** (GL's left-handed cube frame, Three's `flipEnvMap`) is a
  library-GLSL concern, not the primitive's: raw GL sampling here.
- **Render-to-face** stays stage 2; its consumer is now named
  (reflection probes and baking the GLSL sky into the radiance cube,
  3d-environment stage 4).

Landed 2026-09-02 (stage 1: static upload + `samplerCube`):

- `createCubeTexture(faces, size, opts?)` in flux:gpu and `@solidrt/core/gpu`
  (auto-freed like `createTexture`); `TextureShape { D2, Cube }` is id
  state beside `format` on both registries (`TextureEntry.shape`,
  `GpuTexture.shape`), `TextureEntry.impeller` became `Option` (Impeller
  adopts 2D names only, so a cube name is never adopted and the raster
  thread deletes it itself on destroy - the one exception to the
  Impeller-owns-the-name rule).
- `UniformKind::SamplerCube`; `PassInput` is a struct carrying the shape
  and binds on `TEXTURE_CUBE_MAP`, with the cube binding of every touched
  unit saved and restored beside the 2D one.
- The shape rules live once in `validate_binding_shapes` (gpu/vocab.rs):
  samplerCube <-> cube map both ways, sampler2DShadow <-> depth texture.
  Every UI-side rebind path and the raster-side fused creates run it (the
  "second rule" depth-ids-views-shadows anticipated); it replaced the ad
  hoc `check_compare_bindings`.
- Sampler-only rejections at the call site: `<texture src>` (warns at
  build: the src property setter has no registry access), readTexture,
  copyTexture, uploadTexture, resizeTexture. `limits.maxCubeMapSize` is
  the new ceiling (GL_MAX_CUBE_MAP_TEXTURE_SIZE is its own query).
- Verified with probes/cubemap-probe.tsx: six solid faces looked up by
  axis read back as the six colors, and `textureLod` on a `mipmap: true`
  cube reads the chain.

Deliberate non-goals here, each its own item: render-to-face
([gpu-cube-render-targets](../backlog/gpu-cube-render-targets.md)), the
half-float HDR format
([gpu-half-float-format](../backlog/gpu-half-float-format.md)), and
explicit mip levels on the upload (reserved as the `Buffer[6][]` widening
above, filed with the render-target item since the prefilter pass and the
build-time levels are the same feature seen from two sides).

Filed 2026-08-04 from ../notes/scene-graph-3d.md. The spot half of the
old spot-and-point shadow item landed 2026-09-02 without cube maps.
