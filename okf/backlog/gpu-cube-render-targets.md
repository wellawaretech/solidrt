---
title: Cube map render targets (render-to-face)
description: A cube map is upload-only, so dynamic reflection probes and baking a GLSL sky into the environment's radiance cube have no path; one cube draw target with the face as a render-time argument, decided in the cube map shape pass.
created: 2026-09-02
---

# Cube map render targets (render-to-face)

Split out of [gpu-cube-maps](../done/gpu-cube-maps.md) when its stage 1
(upload + `samplerCube`) landed; the shape was decided there and is
repeated here so it does not get re-derived.

Consumers, all in [3d-environment](3d-environment.md) stage 4: a
reflection probe (Three `CubeCamera`, Unity/Godot `ReflectionProbe`
realtime), and rendering the scene's GLSL sky into the radiance cube the
environment samples (Godot's sky-to-radiance bake), with roughness
prefiltering per level after that (the split-sum specular term).

Stage 1 LANDED 2026-09-05 as 3d-environment stage 4a: the shape below,
minus levels - `createCubeDrawTarget` (alloy `create_cube_storage` /
`ShaderTexture::new_cube_draw_target`, raster `CreateCubeDrawTarget`,
UI-side `create_cube_draw_target`), `renderTarget(id, face)` (the
`RenderTarget` command grew `face: Option<u32>`, validated UI-side
against the entry's shape), the face pass inverting the front-face rule
(`PassDraw::Draws::invert_winding`) because a GL cube face is the x
mirror of a 2D target's image. Open here: render-into-level for the
prefiltered chain (4b), a color format on draw targets for HDR (4c).

Shape:

- One `createCubeDrawTarget(size, params, opts)` with ONE entry list; the
  face is a render-time argument (`renderTarget(cube, face)`), so the
  target model is not multiplied by six. `depth: true` is one renderbuffer
  reused across the six face passes.
- Reject `samples >= 2`, `mipmap` and `depth: "texture"` initially.
- The output is the same sampler-only cube id `createCubeTexture` returns
  (shape state, `TextureShape::Cube`); the raster side must delete the
  name itself, as for uploaded cube maps (never Impeller-adopted).
- Prefiltered levels: rendering into level N of a face (the GGX
  convolution pass) is the additive follow-on; the upload side landed
  the same thing 2026-09-03 as the explicit-chain form of
  `createCubeTexture` (an array of six-face arrays, the full chain).
