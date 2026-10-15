---
title: Specular highlights from a light's size
description: A light's angularSize / sourceRadius softens its shadows but not its highlight, so a sized sun still draws the pinpoint highlight of a point source on glossy surfaces; widen the specular lobe from the same light size (the representative-point or roughness-widening approximation), as Unreal, Godot and HDRP do off the one value.
created: 2026-10-15
---

# Specular highlights from a light's size

Context: `packages/3d/src/glsl.ts` (`ggxSpecular`, `blinnSpecular`,
`sceneShadeSource`, `uShadowSize`), `light.ts` (`angularSize`,
`sourceRadius`), `scene.ts` (`writeLights`).

## Problem

Since [3d-contact-hardening-shadows](../plans/3d-contact-hardening-shadows.md)
a light has a size: `angularSize` on a directional light, `sourceRadius`
on a spot or point light. Only the shadow lookup reads it. The highlight
is still a point source's, so a sun with an angular size draws a pinpoint
on a polished `standard` surface, and a large lamp near a glossy floor
draws a dot where a disc belongs. The two halves of the light disagree on
what the light is.

Unreal, Godot and HDRP all drive both from the one value: the source
angle or radius widens the specular lobe and sizes the soft shadow.

## Done looks like

- With a size set, `standard`'s GGX highlight and `phong`'s Blinn
  highlight widen to the light's disc. A size of 0 renders exactly what
  it renders today.
- Energy is conserved: a wider highlight is dimmer, not brighter in
  total.
- No new API. The size already on the light drives it.
- A headless test: a glossy plane under a sized and an unsized sun, the
  highlight's width read across a pixel row, wider with the size.
- AGENTS.md Lights says the size shapes the highlight as well as the
  shadow.

## Analysis

Two known approximations, both per light in the shade loop:

- **Roughness widening** (Karis, Real Shading in Unreal Engine 4): the
  GGX alpha grows by the light's angular radius as seen from the point,
  `alpha' = clamp(alpha + radius / (2 * distance))` for a sphere light,
  the angular radius itself for a directional one, and the lobe is scaled
  by `(alpha / alpha')^2` to keep the energy. One line in `ggxSpecular`'s
  caller; Blinn takes the matching shininess drop.
- **Representative point**: the light vector bends toward the point on
  the light's disc closest to the reflection ray, then the normalisation
  above. More accurate for large, close lights, a few more ALU.

The shader already has the per-light size as `uShadowSize[i]` (the tan
of half the angle, or the radius), but only under `SHADOW_SLOTS`, which a
non-receiving material does not compose. The size wants a slot in
`LIGHT_SLOTS` instead (or `uLightParams` widened), so a material that
opts out of shadows still gets the right highlight. That moves one
uniform between the two chunks.

The Adreno 610 is sensitive to program size (Findings of the shadows
item); measure the shade loop on the tablet before and after.
