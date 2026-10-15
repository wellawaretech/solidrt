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

## Where to pick up (2026-10-15)

Built, headless-tested and tablet-measured (see Findings). Open before the
item can move to `done/`:

1. **Look at `examples/first-person.tsx` on a device.** It is the one
   app with `shadow.radius` above 1, so it now filters through the disc:
   the grid's banding became per-pixel grain at the medium tier, exactly
   where a first-person eye over a floor looks. A human judgment; if the
   grain reads badly, the answer is a tier or a radius, not the grid.
2. **Decide the softness cap.** `SHADOW_MAX_TEXELS` (8) caps the penumbra
   in map texels, so a finer map gives a narrower maximum and the same
   `angularSize` means less on a sharper map; the real sun's 0.53 degrees
   barely shows at ordinary map sizes (the example exaggerates to 10).
   Options: a cap in world units, or a higher texel cap with the tiers'
   tap counts raised to hold the grain. Decide after item 1.
3. **Close the test gaps.** `tests/soft-shadows.test.tsx` measures the
   sun (box and two cascades) and the spot, not a point light with
   `sourceRadius` and not a probe row inside a cascade blend band. Add
   both to the example's layout and the test.
4. **Measure the other devices.** The TV (armeabi-v7a, its own GPU) and
   the Pixel 7 at hard, low, medium and high on the example, with the
   protocol in Findings (a fixed idle or a thermal-zone cooldown before
   each row).
5. **Optional: a tighter contact under a sized light.** The blocker
   search averages the caster's own side, so a sized light's foot
   measures 0.10 to 0.14 world units against 0.04 for a point source.
   A search region that shrinks near the receiver would tighten it at
   the cost of missing far blockers. Every PCSS has this; only pursue it
   if item 1 shows it.

Then `git mv` to `done/` and cut Findings into a note. The specular
half of the light size is its own item:
[3d-light-size-specular](../backlog/3d-light-size-specular.md).

## Findings

- **A sized light's contact is near-hard, not hard.** The blocker search
  averages every occluder in its region, so under a tall caster it
  includes the caster's side up to the search radius and the foot's
  penumbra is about the light's size times the height the search
  reaches, not zero. Every PCSS behaves so; the headless test bounds a
  sized foot at 0.25 world units and a point source's at 0.15.
  Measured on the example at medium (transition width across the edge,
  world units, foot / tip): sun box 0.04 / 0.38, sun as two cascades
  0.10 / 0.34, spot 0.14 / 0.34; size 0 or the hard tier 0.02 to 0.04
  everywhere.
- **The texel cap makes softness a function of map resolution.** With
  `SHADOW_MAX_TEXELS` 8, a finer map caps the penumbra lower in world
  units; the example uses coarse maps (384 and 256) so the tips read.
  The cap is deliberately not tied to the quality tier: a tier changes
  the grain, never the look. If a scene wants wider penumbrae than its
  map allows, the knob is `mapSize` (or a tighter frustum), and the
  device numbers decide whether the cap itself should rise.
- **Light layers and shadow views.** A casting light's shadow view draws
  the meshes the SCENE draws that the light lights (its mask is the
  scene mask AND the light's layers), so a light on a bit the scene's
  `layers` does not hold casts nothing. The example keeps the post out
  of the sun this way and needed `layers={3}` on the Scene for it.
- **Adreno 610 takes one depth texture on two units, with and without
  compare, in one draw.** The SM-T500 (release client of 2026-10-15)
  renders the example exactly as the desktop does: hard foot, wide tip,
  the spot's post the same; no driver or shader warning. The feared
  R32F-copy fallback is not needed.
- **A capped loop bound unrolls into a fixed cost.** The first kernel
  looped to MAX_SHADOW_TAPS with a break on the tier's count, and spent
  a sine, cosine and root per tap; on the tablet the three tiers cost
  the same (about 2.5x a hard frame): the compiler unrolled both loops
  to the table's length and predicated every iteration. The disc is now
  a constant table rotated by one matrix per kernel, and the loops run to
  the tap-count uniform itself with the table index masked, so the
  compiler has no cap to unroll to. Costs then scale with the tier.
- **Tablet measurements drift with heat and the clock is unreadable.**
  Every kgsl clock node is permission-denied on the SM-T500, and the
  same build measured 240 and 335 ms at medium minutes apart as the GPU
  sensor climbed from 33 to 40 C. Rows are only comparable when taken
  after a cooldown (the `cool.sh` protocol: wait for gpu-usr under 35 C
  before each configuration) or within one short batch.
- **The kernel must be instantiated ONCE in the fragment program.** The
  lookup first called `shadowSample` at three sites (a point light's
  face, the cascade, its blend partner); inlined three times the
  program outgrew the Adreno 610 and every pixel paid, the hard path
  included (115 ms a frame against 72 with one copy; the soft tiers two
  and a half times their cost). Three one-site forms were tried on the
  tablet before the one that works: a two-pass loop over the chosen
  maps with dynamically indexed local arrays (scratch memory, hard 286
  ms), a counted two-pass loop over scalars (unrolled into two
  predicated copies, hard 105, medium 190), a while loop on the map
  index with lean state (hard 105 still). The form that keeps the gain
  is the original scan itself: ONE `for` over the light's maps with the
  blend partner sampled by the same call on the next iteration and a
  `blend` sentinel saying so (hard 89, low 123, medium 135, each after
  a fixed idle). Rule for every future kernel of this size: one textual
  call, inside the scan loop, scalars only, no counted loop around it.
- **Cost on the SM-T500 (Adreno 610), release client, 2000x1092, the
  example's two casters, GPU ms per frame from the compositor
  timestamps, each row after a fixed idle of 100 s (the USB link and
  with it the thermal sensor had dropped; the rows repeat within 3 ms):**

  | configuration | ms |
  | --- | --- |
  | hard tier, or no size and radius 1 | 77 to 79 |
  | low (8 / 8) | 117 |
  | medium (16 / 16, default) | 139 to 141 |
  | high (24 / 32) | 189 |

  The soft path costs its taps and nothing else now: about 2.5 ms per
  tap over a ground that fills the frame. A default sun size stays 0:
  on this class of device the medium tier is a third of the frame, the
  app's `shadows.quality` and the per-light sizes decide.
