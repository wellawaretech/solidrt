---
title: Environment tier - skybox and environment reflections
description: The 3d scene has no environment - no skybox from a cube map, no reflections, no image-based ambient - which every engine treats as one scene-level resource; the Three/Godot/Unity comparison here fixes the shape (scene-level, cube map with a mip chain, equirect and six-face sources, HDR half float) and confirms the samplerCube primitive.
created: 2026-09-02
---

# Environment tier - skybox and environment reflections

Roadmap item 14 (../notes/3d-roadmap.md). Today `@solidrt/3d` has a
screen-space GLSL background (`setBackground`), a hemisphere ambient, and
Blinn-Phong `lit`; nothing in the scene reflects its surroundings, and a
photographed or rendered sky has no path in. The engine primitive is
[gpu-cube-maps](../done/gpu-cube-maps.md) (landed 2026-09-02, after the
comparison below fixed what the tier needs from it); this item is the
library shape above it.

## The three-way comparison

Field by field, per the standing rule (packages/3d/CLAUDE.md).

**Where the environment lives.** Godot: one `Environment` resource per
world (background mode, sky, ambient source, reflected-light source).
Unity: `RenderSettings` / the Lighting window (skybox material,
environment lighting source, environment reflections source). Three:
`scene.environment` plus `scene.background`, and additionally a
per-material `envMap` that predates them. Two engines are scene-level
only; Three has both and its own default moved to the scene form.
Verdict: scene-level, `scene.setEnvironment(...)`, no per-material map in
v1. A per-material override (Three's `envMap`) is a later additive option
on `lit`.

**Source interchange.** All three accept an equirectangular panorama
(Three's `EquirectangularReflectionMapping`, Godot's
`PanoramaSkyMaterial`, Unity's `Skybox/Panoramic` and the importer's
cube shape from lat-long) and all three accept six faces (Three
`CubeTexture` in +X,-X,+Y,-Y,+Z,-Z order, Godot's `Cubemap` import from
1x6/2x3/3x2/6x1 layouts, Unity's `Cubemap` asset). HDR `.hdr`/`.exr` is
the IBL source everywhere. Verdict: both forms, with equirect as the
common interchange and six faces as the GPU form. Equirect-to-cube is a
conversion, not a shader-time sampling mode: Unity does it at import,
Godot bakes the panorama into its radiance cube, Three converts on
upload. Ours belongs in the `srt` asset pipeline (build time, like fonts
and icons), with a runtime fallback for a fetched panorama.

**Reflection representation.** Godot: a radiance cube map with
roughness-convolved mip levels (`Sky.radiance_size`), sampled with
`textureLod` on a `samplerCube`. Unity: the same (the specular-convolved
reflection probe cube map, `UNITY_SAMPLE_TEXCUBE_LOD`). Three: PMREM,
which packs the convolved levels into ONE 2D texture and samples it
through `textureCubeUV`. Three's own source explains the packing as a
WebGL1 workaround: no seamless cube filtering and no cube `textureLod`
there. GLES 3.0 has both in core. Two engines plus the reason for the
third's divergence: verdict is a mip-chained cube map. This is the
finding that confirms the primitive - Three's 2D packing is a constraint
we do not share, not a design to copy.

**Diffuse ambient.** Unity: L2 spherical harmonics (the ambient probe,
27 floats). Three: `LightProbe`, L2 SH, generated from a cube texture.
Godot: derived from the sky's radiance map. SH is a 2-vs-1 and is 27
uniform floats, no texture. Verdict: keep the hemisphere ambient as the
v1 term; SH9 from the environment is an additive later step (an
`ambient: "sh"` mode, coefficients computed once from the cube's smallest
level). Three's `HemisphereLight` stays a first-class light, as here.

**The specular BRDF.** All three use the split-sum form: prefiltered
radiance times an environment BRDF, and all three use an analytic fit for
the second factor (Three's `DFGApprox` and Godot's `brdf_approx` share the
Lazarov constants; URP's `EnvironmentBRDFSpecular` is a lerp on a
surface-reduction term). Verdict: no lookup texture; an analytic fit in
the GLSL library. This only matters at the PBR stage below.

**Skies as shaders.** Godot: a `sky` shader type, the `Sky` resource
takes any ShaderMaterial and renders it INTO the radiance cube, so a
procedural sky lights the scene. Unity: skybox is a material
(`Skybox/Procedural`, custom skybox shaders are routine) and the
environment reflection bakes from it. Three: no sky shader concept; a
sky is a box mesh with a ShaderMaterial. Verdict: 2-vs-1 for a
shader-driven sky, and it is what `setBackground` already is once the
fragment gets a view ray. That settles roadmap item 18's open question:
sanction the directional background (hand the fragment a world-space
view ray), document it, and later bake the same fragment into the
radiance cube through render-to-face. Background and environment then
share one authoring surface, as in Godot.

**Knobs.** Three: `backgroundIntensity`, `backgroundBlurriness`,
`backgroundRotation`, `environmentIntensity`, `environmentRotation`.
Unity: skybox rotation and exposure, reflection intensity multiplier.
Godot: `sky_rotation`, `background_energy_multiplier`,
`ambient_light_sky_contribution`. All three: a rotation and an intensity,
on the background and on the environment separately. Verdict: `{ cube,
intensity?, rotation? }` for both (the key names the shape, since a
TextureId is a bare number and a 2D image background can widen later
under its own key); rotation as a y-axis angle in radians, like node
rotation, blurriness deferred (it is a mip bias on the background draw,
additive).

**Dynamic probes.** Three `CubeCamera` + `WebGLCubeRenderTarget`, Unity
`ReflectionProbe` (realtime), Godot `ReflectionProbe` (`update_mode`
Always). All three have it, all three treat it as the expensive option.
Verdict: render-to-face is stage 2 of the primitive, and a
`createReflectionProbe` is the additive library shape above it; nothing
in v1 depends on it.

**Color space.** Three: linear lighting on half-float HDR environments,
tone mapping, sRGB output. Godot: linear HDR internally. Unity: linear
color space is the default for new projects. Three-way convergence on
linear HDR; the pixel contract here is non-linear RGBA8 everywhere
(roadmap item 17). Verdict: the tier ships LDR-correct first (an rgba8
cube map, the current non-linear math, a skybox and a Phong-style
reflection look right), and the HDR path lands with item 17. What must
be decided NOW so that is additive: the HDR upload format. All three
engines' IBL runs on half float (Three's `HalfFloatType` is its PMREM
default); GLES 3.0 makes RGBA16F filterable and mip-mappable in core
while RGBA32F is not, so the existing nearest-only float contract is
right for 32f and a separate `rgba16f` format (linear filter, mipmaps,
upload-only) is the HDR environment format. That is a new value in the
existing format vocabulary, nothing else moves.

**The handedness trap.** GL samples a cube map in a left-handed frame
(the RenderMan convention: faces are seen from inside). Three flips x on
every cube lookup of an image-sourced cube (`flipEnvMap`), and not on a
render-target-sourced one. This lives in the library's GLSL (the
reflection and skybox lookups), never in the primitive; document it next
to the lookup so a ported Three shader does not double-flip. Landed as
`CUBE_LOOKUP` in `@solidrt/3d/glsl` with stage 1.

## Structural leverage

Where the different internal model helps, beyond parity:

- **Build-time prefiltering.** Unity convolves at import, Godot and Three
  at runtime every time the sky changes. The `srt` pipeline can emit the
  six faces AND the roughness-convolved levels from a `.hdr` at build time,
  so a static environment costs one upload and zero shader passes at
  startup, on every device class. That needs the primitive's upload to
  accept explicit levels (reserved; see
  [gpu-cube-render-targets](gpu-cube-render-targets.md)).
- **One binding per scene.** `setTargetTextures` already binds a texture
  once per scene target; the environment is exactly that (Three rebinds
  per material, per draw).
- **The sky is a fragment.** The background slot is a raw fragment with
  the shader-target contract; a Godot-style procedural sky is app code,
  no material class, no box mesh, and it is the same source the radiance
  bake will consume.

## Staging

1. **Skybox from a cube map.** Landed 2026-09-02. `setBackground`
   widened to `string | { cube: TextureId, intensity?, rotation? } |
   null` (the union widening item 18 reserved), drawn as the same first
   entry with a view-ray lookup; the directional GLSL background is
   sanctioned (the fragment receives `vRay`, a world-space ray rebuilt
   from the new shared `uInvViewProj`). A skybox-to-skybox replace
   rewrites the entry in place, so `rotation` animates from the reactive
   prop. `examples/skybox.tsx`, `probes/skybox-probe.tsx`.
2. **Environment reflections on `lit`.** `scene.setEnvironment({ map,
   intensity?, rotation? })`; `lit` gains `reflectivity` (Three's
   Phong/Lambert knob) and mixes a `textureLod` reflection lookup at a
   level derived from `shininess`, fresnel-weighted. LDR, current color
   math. The generated (box-filtered) chain is enough here.
3. **HDR and PBR.** Item 17:
   [rgba16f](gpu-half-float-format.md), linear lighting, tone mapping,
   metallic/roughness on `lit` (or a `standard` material), split-sum IBL
   with the analytic BRDF, explicit prefiltered levels from the `srt`
   pipeline, SH9 ambient. Each an additive option on the shapes above.
4. **Dynamic probes.** `createReflectionProbe` over render-to-face
   ([gpu-cube-render-targets](gpu-cube-render-targets.md)), and baking
   the GLSL sky into the radiance cube.

## Divergences to document

- Scene-level only, no `material.envMap` in v1 (Three porters).
- `reflectivity` on Blinn-Phong is the Three Phong shape; Godot and Unity
  have no non-PBR reflective material, so stage 2 is a Three-parity
  intermediate that stage 3 supersedes without removing it.
- Rotation is a single y angle, not Three's Euler triple, until asked.
