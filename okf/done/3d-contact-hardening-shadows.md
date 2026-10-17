---
title: Contact-hardening soft shadows from a light's angular size
description: A shadow edge is either uniformly hard (radius 1) or uniformly blurred (radius above 1), so softening it blurs the contact where a caster meets its receiver as much as the far tip and the caster looks like it floats; an angular size on the directional light (a source radius on spot and point) turning on a blocker-search lookup (PCSS) in SHADOW_LOOKUP.
created: 2026-10-15
completed: 2026-10-16
---

# Contact-hardening soft shadows

Context: `packages/3d/src/glsl.ts` (`SHADOW_SLOTS`, `SHADOW`,
`SHADOW_LOOKUP`), `light.ts` (the `shadow` options, `checkRadius`),
`scene-shadows.ts` (cascade fit, the atlas), `scene.ts` (the
`uShadowAtlas` binding), `alloy/src/gl/texture.rs` (`SamplerCache`, the
comparison sampler).

## Problem

`shadowSample` has two shapes. `radius` 1 is one hardware comparison tap
(2x2 PCF): a hard edge. `radius` above 1 is a 3x3 grid of those taps
`radius` texels apart: the edge softens by the same width everywhere.

A real penumbra is neither. Its width grows with the distance between the
occluding edge and the surface the shadow lands on, times the light's
angular size. The sun is about 0.53 degrees across, so the penumbra grows
about 1 cm per metre of that distance: sharp where a wall meets the
ground, about 12 cm at the end of an eave's shadow, over a metre at the
tip of a 100 m tower's.

The uniform blur is wrong exactly where the eye looks. It softens the
contact at a building's foot as much as the far tip, so the building
reads as floating. In close-up scenes an app's only correct choice
is `radius: 1`, hard everywhere. The radius blur also leaks a thin lit
band at contacts under the back-face map (taps comparing at nearly equal
depth), a second reason it does not suit a camera that comes close.

Unreal's directional light has a source angle defaulting to the sun's
0.5357 degrees, Godot 4 has `light_angular_distance`, Unity HDRP an
angular diameter feeding PCSS. Three ships PCSS only as an example; URP
has a uniform filter like ours. See the "Contact-hardening (PCSS)" row
of [3d-feature-parity](../notes/3d-feature-parity.md).

## Done looks like

- An angular size on the directional light (degrees, on `shadow`), and a
  source radius (world units) on spot and point lights. Unset keeps
  today's lookup.
- When set, `lightShadow` runs percentage-closer soft shadows for that
  light:
  1. Blocker search: the average depth of the occluders (raw depth closer
     than the receiver) in a region of the map around the point, sized
     from the light's size. No blocker = lit, no filter.
  2. Penumbra width from receiver depth, average blocker depth and the
     light's size.
  3. A comparison filter of that width, built from hardware PCF taps so
     contacts stay as sharp as `radius: 1`.
- A lit `standard`/`phong` scene with an angular-sized sun shows hard
  contacts and widening tips, checked headless from a snapshot.
- AGENTS.md Shadows: angular size is how to soften a shadow that a close
  camera sees; `radius` stays the uniform filter for distant views.

## Analysis

**Raw depth beside the comparison.** The blocker search needs depth
values, and `uShadowAtlas` is a `sampler2DShadow`, which only returns
compare results. The engine already picks the sampler object by declared
type: a comparison sampler behind `sampler2DShadow`, a plain one behind
`sampler2D` (the doc comment on `SHADOW` notes a plain declaration
"reads raw depth values at nearest"). So the same depth texture bound a
second time under a plain name (say `uShadowDepth`) gives the search its
values with no new engine feature, as long as one draw may bind one
texture on two units with different samplers (legal in GL; check our
binding path allows it). It must be a NEAREST sampler: GLES 3.0 makes a
depth texture with compare mode NONE incomplete under LINEAR. A
receiving material composes the second sampler only when a light in the
scene has a size, so scenes without it pay nothing.

**Units per map.** A directional cascade is an ortho map, so its depth is
linear: world distance = depth delta times that map's depth range, and
the world penumbra converts to texels by that map's texel size in world
units (`2 * radius / mapSize` from the cascade fit). That needs two more
per-map uniforms (depth range, world units per texel) beside
`uShadowRect`/`uShadowMatrix`; the scene knows both when it fits each
cascade. Spot and point maps are perspective, so depth must be
linearised with the map's near/far before the classic
`(receiver - blocker) * size / blocker` estimate.

**Cost and bounds.** A search of around 16 taps plus a filter of 16 to
25 per fragment per casting light, doubled inside the cascade blend band
where two maps are read. Bounds to design in:
- Clamp the search region and penumbra to a few texels' worth per
  cascade. A far cascade's texels are metres wide, so its penumbra is
  under a texel and the single hardware tap is already right: skip the
  search there.
- Past about 3 texels' spacing a regular grid bands (the same limit the
  radius filter documents). A wider filter wants a Poisson-style disc,
  rotated per pixel by a hash, and therefore more taps or visible noise.
  Pick one and say which.
- The low-end baseline: keep it opt-in per light, and say what it costs
  on the reference GPUs before deciding whether a default sun gets it.

**Interplay with existing knobs.** With a size set, `radius` is ignored
for that light (or is the floor of the filter; decide). `bias` and
`normalBias` still apply to the comparison taps; the search compares raw
depth and needs the same bias, or the receiver finds itself as a
blocker.

**Custom materials.** The third tier of custom looks composes
`SHADOW_SLOTS`/`SHADOW`/`SHADOW_LOOKUP` and calls `lightShadow`, so it
gets this for free once the extra slot and uniforms live in those
chunks. An app that wants this today has to re-derive the cascade select
and blend by hand over a plain `sampler2D uShadowAtlas`, and drifts from
every later change to `SHADOW_LOOKUP`.

## Plan (2026-10-15)

Decided against the three engines (Godot, Unreal, HDRP agree on every
point below) and widened from the backlog shape to engine grade. Nothing
in alloy changes: the binding resolver already maps each binding on its
own, a depth id on a plain `sampler2D` is the legal raw-depth read, and a
depth texture registers with the NEAREST sampler state.

**Light shape, not a shadow option.** `angularSize` on the directional
light (degrees, the disc's diameter, default 0) and `sourceRadius` on
spot and point lights (world units, default 0) sit on the light, as
Godot's `light_angular_distance`/`light_size`, Unreal's source angle and
radius and HDRP's angular diameter and shape radius do. The specular
widening off the same fields is a later, additive item.

**One kernel on the receiving side.** The 3x3 grid goes. A Vogel disc
rotated per pixel by interleaved gradient noise is the one filter: its
radius in texels is `shadow.radius` for a light without a size and the
penumbra estimate floored by `radius` (HDRP's min filter size) for a
light with one. Radius 1 without a size stays the single hardware tap.
No TAA exists to hide the rotation noise, so the trade is Godot's and is
documented as such; a screen-space mask pass was rejected.

**Quality is a scene setting.** `shadows: { quality }` with `hard`,
`low`, `medium` (default) and `high`, live through `setShadowQuality`,
written as two tap-count uniforms (search, filter): the shader loops on
them, so no program variant exists per scene. `hard` collapses every
light to the one tap, the low-end profile's switch.

**Receiver-plane slope correction** on every tap wider than one, from the
normal `lightShadow` already takes, transformed into each map's space;
no screen derivatives, so it holds inside the divergent light loop.

**Penumbra per map type.** Orthographic maps convert the depth delta by
the map's depth range and the map's world units per texel; the search
region is the cone to the light's disc at the map's near plane, which a
cascade's far-back near plane always clamps to the texel cap.
Perspective maps linearise depth with near and far and use the classic
similar-triangles estimate. The estimate never exceeds the search
radius, so all blockers found is exact umbra. One `vec4` per map slot
(`uShadowMetric`: near, far, texel, range) rides with the matrices.

**Steps.** glsl.ts (slots, kernel, lookup, doc comments); light.ts and
the three light components; math.ts `shadowMetric` with a unit test;
scene-shadows.ts (quality table, metrics); scene.ts (size and taps in
writeLights, the second binding, the option and setter); scene.tsx and
index.ts; `examples/soft-shadows.tsx` as a new lane with debug commands
for a headless test that measures the contact and tip transitions; the
Shadows sections of AGENTS.md and the parity row.

**Verification.** The 3d suite and `sol check` headless, then the Adreno
610 phone first (one depth texture on two units with and without compare
in one draw is the driver risk; the fallback is a copy into an R32F
target), then the tablet and TV for the GPU frame time at medium with and
without a size. The default sun size stays 0 until those numbers exist.
The two caps (`SHADOW_MAX_TEXELS`, `SHADOW_SLOPE_MAX`) are the tuning
constants.

## Closed (2026-10-16)

Decisions taken at the close, after the pick-up round:

- **The quality tier sets the filter width, as Unity's does.** `low`,
  `medium` and `high` carry a radius of 1.5, 2.5 and 3.5 texels (the
  reach of Unity's 3x3, 5x5 and 7x7 soft-shadow tents) beside their tap
  counts, and a light's `shadow.radius` is null by default, meaning the
  tier's width; an explicit radius is the override and, under a sized
  light, the floor. The first-person example dropped its hand-picked
  radius 2 and takes the tier's 2.5. The hard tier is still the one tap
  for every light.
- **The texel cap stays.** Unity's penumbra is texel-based too, so an
  `angularSize` means more on a coarser map there as well; the contract
  is on the prop. A wider world-unit softness is a map size or a tighter
  frustum.
- **The soft-shadows example pins its lights at radius 1.** Its maps are
  deliberately coarse (384 and 256 texels; 13 cm on the second cascade),
  where the tier's 2.5 texels blur every contact a third of a metre and
  leak at the wall's base; the example is about the contact, so it keeps
  the hardware-tap floor and `sun({ radius: null })` shows the tier's
  width (its own test).
- **Fixed on the way:** `mergeSpotShadow` never copied `radius`, so a
  spot or point light's `shadow.radius` had been silently ignored.
- **Not done, on purpose:** a wider contact under a sized light (the
  search region shrinking near the receiver); nothing on a device asked
  for it.

The findings are in
[3d-soft-shadows-findings](../notes/3d-soft-shadows-findings.md). The
specular half of the light size is its own item:
[3d-light-size-specular](../backlog/3d-light-size-specular.md); the
contact leak of the uniform width on a coarse map is
[3d-shadow-contact-leak](../backlog/3d-shadow-contact-leak.md), and the
package-wide quality tier the close raised is
[3d-quality-tier](3d-quality-tier.md).
