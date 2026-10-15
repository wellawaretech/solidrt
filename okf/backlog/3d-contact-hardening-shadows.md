---
title: Contact-hardening soft shadows from a light's angular size
description: A shadow edge is either uniformly hard (radius 1) or uniformly blurred (radius above 1), so softening it blurs the contact where a caster meets its receiver as much as the far tip and the caster looks like it floats; an angular size on the directional light (a source radius on spot and point) turning on a blocker-search lookup (PCSS) in SHADOW_LOOKUP.
created: 2026-10-15
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
