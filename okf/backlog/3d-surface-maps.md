---
title: Surface maps on lit - normal, emissive, specular and light maps, plus a UV transform
description: lit takes ONE map, the base color; a track surface with a normal map, a glowing sign, a glossy-versus-matte mask or a baked lightmap all have to leave the standard material for a hand-written shaderMaterial. Add the pre-PBR map slots to lit as class-key options, with the tangent layout from roadmap item 10 as the prerequisite and a UV offset/repeat for scrolling surfaces.
created: 2026-08-30
---

# Surface maps on lit - normal, emissive, specular and light maps, plus a UV transform

## Symptom

`LitOptions` (`packages/3d/src/material.ts`) is `color`, `map`,
`specular`/`shininess` (scalars), `vertexColors`, `triplanar`,
`alphaTest`, `receiveShadow`. One texture, the base color. Every standard
material in Three/Godot/Unity's non-PBR tier carries at least four more,
and a game's authored assets use them all:

- **Normal map** - the one that matters most. Track surfaces, kart
  bodies, rock walls: the detail that makes a lit surface look like more
  than its triangle count is a tangent-space normal map. Without it the
  glTF loader drops the file's `normalTexture` on the floor.
- **Emissive map** - lamps, screens, nitro glow, the "glow cards" the
  loader item already notes come out as dark wedges. Roadmap item 5 lists
  a scalar `emissive` as deferred; a map is the same slot textured.
- **Specular / gloss map** - which parts of a kart are chrome and which
  are rubber; today `specular` is one number for the whole mesh.
- **Light map** (second UV set) - baked static lighting on a track, the
  cheapest way to light a large scene on a low-end GPU; the loader item
  lists the second UV set as a dropped channel.
- **UV offset / repeat** - scrolling water, a conveyor belt, an animated
  sign; Three's `map.offset`/`map.repeat`. A scene-wide clock through
  `scene.setParams` plus a per-material transform covers it without a
  custom vertex stage.

Each of these is expressible today by composing `LIT_VERTEX` and the
`/glsl` pieces into a `shaderMaterial` - the escape hatch is real - but a
game with a dozen materials would rewrite the standard fragment a dozen
times, and the shadow variant, the alphaTest variant and the triplanar
path with it.

Roadmap items 10 and 17 bracket this without owning it: 10 names the
tangent layout "when items 7 and 16 force them" (this is what forces it),
and 17 is the PBR tier, which is a different lighting MODEL. Blinn-Phong
with a normal map is the tier every engine shipped for a decade before
PBR, and it must not wait on the color-space decision.

## Shape

Additive options on `lit`, each one more class-key dimension exactly like
`map`/`triplanar`/`alphaTest` today (one program per option combination,
cached; the combinatorics stay bounded because most scenes use two or
three combinations):

- `normalMap: TextureId` (+ `normalScale?`). Prerequisite: a "tangent"
  named layout (`aTangent` vec4, sign in w) in the geometry vocabulary -
  the parser emits it when the file has tangents, `withTangents`/a
  generator `layout` option computes it (MikkTSpace-style from UVs, pure
  array math, bake-time under bun for big models). A normalMap material
  rejects a geometry without the channel at add(), the aColor rule.
- `emissive?: [r, g, b]` and `emissiveMap?: TextureId` - added after the
  lighting terms, unaffected by shadow (the `receiveShadow: false` doc
  comment already imagines "an emissive surface").
- `specularMap?: TextureId` - multiplies the `specular` scalar per texel.
- `lightMap?: TextureId` (+ `lightMapIntensity?`) - sampled by a second
  UV channel (`aUV2`, another named layout slot), multiplied into the
  diffuse term. Depends on the loader emitting the channel.
- `mapTransform?: { offset?: [u, v], repeat?: [u, v] }` on `lit` and
  `unlit`, a per-entry vec4 param (`setMeshParams` can drive it; a
  scene-wide scroll is one `scene.setParams` write with a matching
  uniform name).

`createModel`'s default material maps glTF `normalTexture`,
`emissiveFactor`/`emissiveTexture` and (KHR_materials_specular or the
loader's own choice) onto these. The maps ship through the same repeat +
mipmap upload as `map`.

## Done looks like

A glTF kart with base, normal and emissive textures renders with all
three under `createModel`'s default material; a track plane with a
tiling normal map shows lit relief under the moving sun; a `lightMap`
track lights itself with no directional light. `examples/` gains one
material-showcase scene. No PBR math, no linear color space.

## Not in this item

Metallic/roughness maps and image-based lighting (item 17, and
[gpu-cube-maps](gpu-cube-maps.md)), parallax/height maps, detail maps.
