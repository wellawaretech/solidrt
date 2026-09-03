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

**The standard material (stage 3c).** Names: Three says `metalness`
and `roughness`, Godot, Unity and glTF say metallic (Unity's roughness
is `smoothness`, its inverse); Three's are kept, the porting consumer's
vocabulary, with `roughness` perceptual (alpha = roughness squared) as
in all three. Maps: glTF packs roughness in green and metalness in blue
of one texture; Three imports it as `roughnessMap` + `metalnessMap`
reading those channels from the same texture, Godot selects a channel
per texture, Unity packs metallic in red with smoothness in alpha.
Verdict: Three's two channel-select options, so the glTF texture passes
twice and a Three port is verbatim. Direct lobe: GGX + height-correlated
Smith + Schlick in all three; dielectric f0 0.04 in all three (Godot's
`metallic_specular` 0.5 is the same number). Light convention: Godot
and Unity light a white diffuse surface to 1 at intensity 1 and carry
pi inside the specular normalisation; Three's lights are lux-scaled, so
its intensities run a factor pi larger for the same look. Verdict: the
2-vs-1 form, which `lit` already used. Image lighting: split sum in all
three, with the analytic fit for the environment BRDF in Three
(DFGApprox) and Godot (brdf_approx); Unity's URP folds a surface
reduction into a lerp. Verdict: the Lazarov fit, no lookup texture.
Metals without an environment: Three and Godot render them black (no
diffuse, nothing to reflect); Unity falls back to its ambient probe.
Verdict: 2-vs-1, no fallback - documented as a trap, and the reason
createModel keeps `lit` as its default until an environment asset
ships with 3d. Not offered: Three's per-material `envMapIntensity`
(Godot and Unity scale at scene level, as `setEnvironment` does),
`aoMap` (additive, next), multi-scatter compensation (Three only).

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
   prop. `examples/skybox.tsx`, `probes/skybox-probe.tsx`. The runtime
   equirect path landed with it: `equirectToCube(map, size, opts?)`
   renders the six faces on the GPU and reads them back into a cube
   TextureId (center column -Z, top row +Y - Godot's and Unity's
   convention, Three centers +X); `probes/equirect-probe.tsx`.
2. **Environment reflections on `lit`.** Landed 2026-09-02.
   `scene.setEnvironment({ cube, intensity?, rotation? } | null)`, the
   createScene option and the reactive prop; one `uEnv` samplerCube per
   receiving target (a 1x1 black placeholder while unset, seeded through
   the light rewrite so new views get it) and shared `uEnvIntensity` /
   `uEnvRotation` / `uEnvOn`. `lit({ reflectivity })` composes the
   exported `ENVIRONMENT` set: `mix(rgb, reflection, weight)` with a
   Schlick weight (reflectivity face-on, 1 at grazing) and the mirror
   direction sampled by `textureLod` at roughness `sqrt(2 / (shininess +
   2))` times the cube's top level. LDR, current color math, the
   generated box-filtered chain. `probes/environment-probe.tsx`.
3. **HDR and PBR.** Item 17, in four parts:
   - 3a, landed 2026-09-03: the formats
     ([gpu-half-float-format](../done/gpu-half-float-format.md)):
     `"rgba16f"` (half float, filterable, mip chain gated on the device's
     half-float renderability) and `"rgba8-srgb"` (hardware decode on
     sample), 2D and cube, upload-and-sample only.
   - 3b, landed 2026-09-03: the color pipeline, LINEAR-ONLY (Three since
     r152 and Godot; Unity's gamma switch is legacy). Every `[r, g, b]`
     option is sRGB and decoded when the uniform is written
     (`packages/3d/src/color.ts`); color maps decode through
     `rgba8-srgb` (createModel tags glTF's base color and emissive
     images); vertex colors are linear. Every library fragment ends in
     the exported `OUTPUT` set's `outputColor(rgb, alpha)`: exposure,
     tone mapping (`scene.setToneMapping("none" | "aces")`,
     `setExposure`, the reactive props) and the sRGB encode, done in the
     fragment (Three's way) because the runtime samples the scene target
     raw for display, so an sRGB render target would show decoded, too
     dark. Consequences documented in packages/3d/AGENTS.md (Color):
     transparent meshes blend in encoded space, the clearColor is not
     tone mapped, a plain rgba8 color map renders washed out.
     `equirectToCube` takes the panorama's format and re-encodes the
     faces for an sRGB cube. `lit` gained `emissiveIntensity` (the
     glTF emissive strength, no longer folded into the color).
   - 3c, landed 2026-09-03: `standard(opts)` beside `lit` - lit's base,
     maps, cutout, shadow, emissive and fog options with the Blinn-Phong
     knobs replaced by `metalness`/`roughness` and Three's channel-select
     `metalnessMap` (blue) / `roughnessMap` (green), which is glTF's one
     packed texture passed twice. GGX distribution, height-correlated
     Smith visibility and Schlick fresnel (dielectric f0 0.04) in the
     existing light and shadow loop; the scene environment sampled ALWAYS
     as the split sum (`envRadiance` at the material's roughness over
     the generated chain, times Lazarov's analytic `envBrdf`); the
     hemisphere stays the diffuse ambient. One shared builder behind
     `litFragment` and the new `standardFragment`; the `PBR` set is
     exported beside the others. glTF: `ModelMaterial` carries
     `metalness`, `roughness` and `metalnessRoughnessMap` (model file
     VERSION 4 - version-3 .srtm files are rejected, re-bake), createModel
     hands the packed map to `material` as both `maps.metalnessMap` and
     `maps.roughnessMap`; its DEFAULT STAYS `lit` until 3d ships a real
     environment asset (decided 2026-09-03: a glTF metal in a scene with
     no environment renders near black, and glTF's default metallic
     factor is 1). `examples/standard.tsx` (the sphere grid),
     `probes/standard-probe.tsx`.
   - 3d, open: explicit `levels` on createCubeTexture, the `srt`
     pipeline turning a `.hdr` into a prefiltered rgba16f cube asset
     (build-time equirect for LDR too), SH9 ambient from the cube.
4. **Dynamic probes.** `createReflectionProbe` over render-to-face
   ([gpu-cube-render-targets](gpu-cube-render-targets.md)), and baking
   the GLSL sky into the radiance cube.

## Divergences to document

- Scene-level only, no `material.envMap` in v1 (Three porters).
- `reflectivity` on Blinn-Phong is the Three Phong shape; Godot and Unity
  have no non-PBR reflective material, so stage 2 is a Three-parity
  intermediate that stage 3 supersedes without removing it. Its combine
  is Three's MixOperation with a Schlick weight; Three's default
  MultiplyOperation (a tint) and AddOperation are not offered.
- Rotation is a single y angle, not Three's Euler triple, until asked.
- No per-material `envMap` on `unlit` (Three's MeshBasicMaterial has
  one); the environment is scene-level and `lit`-only until asked.
- The panorama's center column faces -Z (Godot, Unity); Three's faces
  +X, so a Three-tuned environment rotation differs by a quarter turn.
- Linear-only, no gamma mode (Unity porters): a scene tuned under the
  old non-linear math shows softer terminators and brighter mid-tones;
  drop ambient rather than lights.
- Tone mapping is a uniform branch (`uToneMapping`), not Three's
  per-material compiled define, and there is no per-material
  `toneMapped: false`: a fragment that writes fragColor directly skips
  the stage instead.
- Only "aces" beside "none" so far; AgX (Three, Godot) and Neutral
  (Three, Unity) are additive values.
- `standard` says `metalness`/`roughness` (Three), not metallic /
  smoothness (Godot, Unity, glTF). Light intensities follow Godot and
  Unity (1 lights white to 1): a Three scene's intensities divide by pi.
- No per-material `envMapIntensity` (Three) on `standard`; the scene's
  `intensity` is the knob. No `aoMap` and no multi-scatter compensation
  yet, both additive. A metal in a scene without an environment renders
  near black (Three, Godot), no ambient-probe fallback (Unity).
