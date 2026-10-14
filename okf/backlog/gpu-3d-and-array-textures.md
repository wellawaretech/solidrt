---
title: 3D and 2D array textures in flux:gpu
description: flux:gpu creates only 2D textures and cube maps, so a volume sampled by world position (a sparse brick pool for mesh paint, a 3D LUT, density fields) must be faked as slices in a 2D atlas with two taps, manual trilinear and guard borders; TextureShape gains D3 and D2Array (both GLES 3.0 core), with sampler3D/sampler2DArray bindings, box uploads and two new limits.
created: 2026-10-14
---

# 3D and 2D array textures

Symptom: `flux:gpu` has `createTexture`/`createMutableTexture` (2D) and
`createCubeTexture`, nothing else. No `TEXTURE_3D`, no `TEXTURE_2D_ARRAY`,
no `sampler3D`/`sampler2DArray` binding, no sub-volume upload.

The case that hits it: paint stored in a sparse 3D brick field. An
indirection grid says which brick of a pool holds the paint around a
point, and the material samples the pool by world position with hardware
trilinear filtering. Without a 3D texture the pool becomes slices laid
side by side in a 2D atlas: two bilinear taps blended along the third axis
in every fragment, the slice arithmetic, and a guard border per slice so
bilinear filtering does not bleed between neighbouring slices; per-brick
writes become 2D rect uploads. Other volumes hit the same wall: 3D color
grading LUTs, density/fog volumes, SDF bricks. Three (`Data3DTexture`,
`DataArrayTexture`), Unity (`Texture3D`, `Texture2DArray`) and Godot
(`Texture3D`, `Texture2DArray`) all have both; the parity table lists the
gap ([3d-feature-parity](../notes/3d-feature-parity.md), "2D array and 3D
textures").

A 3D texture is the one that matters for volumes: it filters across all
three axes. A 2D array filters within a layer only, so a volume stored in
one still needs the two taps and the manual blend; it removes only the
atlas arithmetic. Arrays earn their place elsewhere (many same-size
layers behind one sampler: terrain splat sets, sprite sheets of equal
size), and the work is nearly identical, so both land together.

Not removed by a 3D texture: the border around each brick in a sparse
pool. Bricks that sit next to each other in the pool are not neighbours in
world space, so trilinear sampling near a brick edge still needs a
one-texel apron. That is the brick scheme's own cost, not the platform's.

## Platform

Both targets are GLES 3.0 core (`glTexStorage3D`, `glTexSubImage3D`, no
extension), on every client including ANGLE; glow exposes the calls.
Nothing here leans toward GLES 3.1. The GLES 3.0 floors are low:
`GL_MAX_3D_TEXTURE_SIZE` and `GL_MAX_ARRAY_TEXTURE_LAYERS` are only
guaranteed 256, against 2048 for 2D, which is what the limits doc has to
say plainly.

## What exists

The cube map work already made the shape a first-class axis:

- `TextureShape { D2, Cube }` in `alloy/src/gpu/texture.rs` (with the
  inventory name, "2d"/"cube"); `TextureEntry` carries it, and a
  non-2D entry has no Impeller texture behind it.
- `alloy/src/gl/pass.rs` picks the bind target from the shape.
- `UniformKind::sampler_shape` validates a binding against the uniform's
  declared sampler type, so a mismatch throws at the bind.
- `uploadTexture` already takes a `TextureRect` for sub-rect writes.
- The 3d package's materials pass `TextureBindings` straight through, so
  a custom material sampling a `sampler3D` works once flux:gpu binds one;
  the package needs no change.

## Shape

- `createTexture3D(data, width, height, depth, opts)` and
  `createArrayTexture(data, width, height, layers, opts)`, the existing
  options (`SamplerOptions & TextureFormatOption & LabelOption`); `data`
  holds the slices/layers back to back. Mutable: the brick pool is
  written after creation, so these are not create-once like cube maps.
  Whether that is a separate mutable create (mirroring
  `createMutableTexture`) or the only form is decided when the plan is
  made; the second is simpler and nothing asks for an immutable volume.
- `uploadTexture(id, data, at)` accepts a box
  `{ x, y, z, width, height, depth }` beside the rect (z is the layer for
  an array): one brick written without re-sending the pool. A rect `at`
  on a 3D or array id throws, and a box on a 2D id throws.
- `wrap` applies on all three axes for 3D (`TEXTURE_WRAP_R`); for an
  array it applies within a layer.
- `mipmap` as today; a 3D chain halves depth too, an array chain does not
  halve layers.
- `limits.max3DTextureSize` and `limits.maxArrayTextureLayers`, validated
  at the create like `maxCubeMapSize`.
- Sampler kinds `sampler3D`, `sampler2DArray` and their `usampler`
  variants (an `rgba32ui` pool) in `UniformKind`, bound only by the
  matching shape.
- Out of contract, throwing as they do for cube maps: `<texture src>`,
  `readTexture`, `copyTexture`, `resizeTexture`, render target creation
  over one (rendering into a layer or slice is a separate item if anyone
  asks), compressed formats.
- Inventory and logs: shape names "3d" and "2d-array", depth/layers in
  the size.

## Done when

A shader target and a 3d package custom material each sample a 3D texture
by position with hardware trilinear filtering and an array texture by
layer, a box upload changes exactly the written texels (read back through
a shader target, since `readTexture` stays 2D), the shape mismatch and
limit errors throw at the call site, and the alloy validation tests and a
flux test cover both shapes.
