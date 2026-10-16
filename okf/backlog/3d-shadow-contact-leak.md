---
title: A uniform shadow filter leaks light at a contact on a coarse map
description: A light without a size filters at the tier's width (2.5 texels at medium), and on a map whose texel is centimetres that kernel reaches past the occluder's depth at a caster's base, so a lit sliver opens between the object and its shadow (peter-panning) and the umbra near the base shows the disc's noise; give the uniform width the blocker search a sized light already runs, so every contact hardens to the one tap, and scale the normal bias with the texel as Unity does.
created: 2026-10-16
---

# A uniform shadow filter leaks light at a contact on a coarse map

Context: `packages/3d/src/glsl.ts` (`SHADOW`: `shadowSample`,
`shadowBlockers`, `shadowSlope`, `SHADOW_SLOPE_MAX`), `light.ts`
(`shadow.radius`, `normalBias`), `scene-shadows.ts`
(`SHADOW_QUALITY_TIERS`), the measurements in
[3d-soft-shadows-findings](../notes/3d-soft-shadows-findings.md).

## Problem

Since the tier sets a filter width, a casting light that sets no
`shadow.radius` filters its edges uniformly at 1.5 / 2.5 / 3.5 map texels
(low / medium / high, the reach of Unity's tents). The lookup compares the
receiver's depth, pushed toward the light by `normalBias` (world units)
and by the receiver-plane slope term at each tap's offset, against the
map. At a contact - a wall's foot, a pillar's base - the occluder is only
a few centimetres closer to the light than the receiver. When the kernel's
reach in world units (texel size times radius) is larger than that gap,
the outer taps compare past the occluder and read lit: a sliver of light
opens between the object and the start of its shadow, and the disc's
rotation shows as noise in the umbra next to the base.

Measured on the soft-shadows example's wall foot at radius 2.5 (the
lit-to-shadowed transition in world units; the one tap reads 0.04):

| map | texel | foot | slope off |
| --- | --- | --- | --- |
| box, 384 texels over 14 m | 3.65 cm | 0.12 | 0.12 |
| cascade 0 | 4.3 cm | 0.40 | 0.28 |
| cascade 1 | 13 cm | 0.70, noisy umbra | 0.44 |

Away from the contact the tip reads 0.14 to 0.16 on every map, so the
kernel is right and only the contact is wrong. On the first-person
courtyard (0.77 cm texels, a 2 cm reach) the pillar shadows attach with
no visible gap: the leak is a coarse-map phenomenon, and the example
with coarse maps pins `radius: 1` for now. A light with a size does not
leak: its blocker search finds every tap occluded at the contact and
returns the one tap.

Every engine with a wide PCF kernel has this failure on a coarse map;
they keep it under a pixel with resolution (cascades), keep the bias
tiny and texel-scaled (Unity's normal bias is in texels), harden contacts
with a blocker search (PCSS), or fill the gap with screen-space contact
shadows (Unreal, HDRP). Unity URP has no contact shadows and its users
see exactly this.

## Done looks like

- The uniform width hardens at a contact the way a sized light does: the
  example's wall foot at `sun({ radius: null })` on the 13 cm cascade
  reads a transition no wider than the one tap plus a texel, and the
  umbra next to the wall is as dark as the umbra away from it (no
  noise), on the box map and both cascades; the tip keeps its uniform
  width.
- `normalBias` no longer pushes a coarse map further than a fine one: a
  texel-scaled bias (Unity's form) or a documented cap, decided and
  tested.
- The soft-shadows test gains the contact row for the uniform width;
  the example can drop its pinned `radius: 1` if the look holds on its
  coarse maps, else the pin stays with the reason.
- The tablet row for the medium tier is re-measured: the search taps the
  uniform path gains are the sized path's, so the medium tier's cost
  should not move for a scene that already has a sized light, and rise
  by the search for one that does not.

## What it involves

- In `shadowSample`, run `shadowBlockers` for the uniform case too, with
  the search radius equal to the filter radius: `found == 0` lit, `found
  == searchTaps` the one tap (umbra), else the filter at the tier's
  width. The sized path keeps its penumbra estimate; the two share the
  search. The hard tier (0 taps) is untouched.
- The receiver-plane term: measure whether capping the slope bias to a
  fraction of a texel's depth range, or scaling `normalBias` by the map's
  texel (`uShadowMetric.z`), closes the remaining gap on the 13 cm map
  with the search in place.
- Costs on the SM-T500 and the Pixel 7 per the notes' protocol
  (`frames({ count })`, cooldown per row).
