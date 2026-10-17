---
title: One quality tier for the whole 3d package
description: The package's quality knobs each stand alone - shadows.quality, lodBias, samples, the fill density (renderScale / maxPixels), the lights' map sizes and cascade counts - so an app that wants "low-end" or "high-end" sets five things in five places and a knob it forgets keeps the medium default; one tier every knob defaults from, each knob an override, as Unity's quality settings and Godot's project settings do, and the question whether the tier lives on the Scene or on the app as a device profile.
created: 2026-10-16
completed: 2026-10-17
---

# One quality tier for the whole 3d package

Context: `packages/3d/src/scene.ts` (`SceneOptions`: `shadows`,
`lodBias`, `samples`, `renderScale`, `maxPixels`, `bloom`),
`scene-shadows.ts` (`SHADOW_QUALITY_TIERS`), `light.ts` (`shadow.mapSize`,
`cascades`, `radius`), `packages/core` (the fill mode's device pixels).

## Problem

The shadow tier set the pattern on 2026-10-16: `shadows.quality` carries
the taps and the filter width, and a light's `shadow.radius` is null by
default, meaning "the tier's", an explicit value the override. The rest
of the package's quality knobs do not follow a tier. `lodBias`,
`samples`, `renderScale` / `maxPixels`, `bloom`'s radius, and per light
`mapSize` and `cascades` each have their own default and nothing ties
them together. An app that targets a low-end device sets the shadow tier
to low, the samples to 1, the LOD bias under 1 and the map sizes down,
in four places, and the one it forgets stays at medium; an app that
says nothing gets medium everywhere, which is the right default for one
knob and not necessarily the right combination.

Unity has a quality settings asset: one tier per platform that holds the
shadow resolution and distance, the cascade count, the soft shadow
tent, the texture quality, the anti-aliasing and the LOD bias, and a
project picks a tier per device class. Godot has per-feature project
settings with a `.mobile` override of each. Three has nothing. The
shadow tier here is one row of such a table.

## Done looks like

- One `quality` setting - `"low" | "medium" | "high"` to start, the
  shadow tier's names, `"hard"` folded into low's shadow row - that
  every quality knob defaults from, documented as one table: per tier
  the shadow taps and width, the LOD bias, the MSAA samples, the fill
  density, the bloom radius, the default `mapSize` and the cascade count
  a casting light gets when it sets none.
- Every knob keeps its explicit form as the override, nullable where it
  is not already, so no existing app changes what it set; only what it
  did not set follows the tier. Additive: an app that says nothing today
  gets exactly the medium row.
- A decision, written down, on where the tier lives: on the `Scene`
  (per scene, like `shadows.quality` now, `setQuality` live), or on the
  app as a device profile the 2d package and the text raster could share
  (the fill mode's `maxPixels` is already a device question more than a
  scene one). The second needs core to carry it; the first is a day's
  work inside the package.
- The examples that hand-tune a knob for a device class (the low-end
  baseline's) say the tier instead where the tier's row matches.

## Decided (2026-10-17)

- **The tier name lives in core, the tables in the packages.**
  `quality()` / `setQuality("low" | "medium" | "high")` in
  `@solidrt/core` (`quality.ts`) is the app's one statement of the
  device's budget, a reactive value, default "medium". Core holds no
  table: the 3d package maps the name to a row of `QUALITY_TIERS`
  (`packages/3d/src/quality.ts`), a 2d package can map it to its own
  when it has one. Unity and Godot make quality global for the same
  reason this does: texture sampling is id state shared by every scene
  that draws the model, so no per-scene setting can set it. The
  per-scene form stays as the override: `<Scene quality>` takes a tier
  name (that row whatever the app tier) or a row object whose missing
  fields are the app tier's. The alternative - the tier on the Scene
  only - was the first proposal and was dropped for that reason.
- **A quality is a row, every knob keeps its explicit form.** A name
  picks a row, a row object with fields missing takes them from the app
  tier's row (Unity's custom levels, in the one rule every knob
  follows); a knob's own option (a light's `mapSize`, the scene's
  `samples`) is the override and never moves when the row does. The
  medium row is the right default, not the status quo: `samples` 4
  (every example had asked for it by hand; the sweep removed those 19 lines once the row carried it) and 2 cascades for an unboxed sun
  change what an app that says nothing gets.
- **Everything changes live.** A `samples` change reallocates the
  target's storage under its id (alloy `reshape`, the resize path with a
  sample count; `setTargetSamples`), the shadow atlas re-settles on a
  map size or cascade count change, the kernel rides the light-set
  write, the LOD bias and fade are target state, and model textures
  re-sample in place (`setTextureSampling`, with `baseLevel` as new
  texture id state for the mip limit). The one creation-fixed knob is a
  probe's size: a cube is create-once, so a probe keeps the size it was
  baked at and the app bakes again to follow.
- **An authored camera box is never replaced by cascades.** A directional
  light that sets `camera` and no `cascades` stays one map under every
  row (`camera` is `ShadowCamera | null` on the record, null = not
  authored, `DEFAULT_SHADOW_CAMERA` when it renders one map); only an
  unboxed, uncascaded light takes the row's count.
- **Left out, with the engine that agrees:** bloom radius (the glow's
  width is a look; Unity and Godot vary the filter resolution, our chain
  already runs at a quarter scale), HDR off at low (the half-float
  buffer is the lighting contract; a bloom threshold above 1 is
  unreachable in LDR), shadow distance (world-scale; Godot keeps it on
  the light), pixel light count (dropping lights changes the look), a
  per-platform default (vendor and platform keys were rejected on
  2026-09-02; the app chooses in code until a probe fact exists).

The table as built (medium = Unity's Medium plus the 4x MSAA every
example asked for; low = the Adreno 610 / Mali-T860 rows measured in
notes/3d-soft-shadows-findings.md and backlog/3d-low-end-gpu-performance.md):

| knob | low | medium | high |
|---|---|---|---|
| shadow kernel | hard | medium (16/16, 2.5 texels) | high (24/32, 3.5) |
| light `mapSize` | 512 | 1024 | 2048 |
| directional `cascades` (unboxed) | 1 | 2 | 2 |
| `lodBias` | 0.7 | 1 | 1.5 |
| LOD cross-fade | off | on | on |
| `samples` | 1 | 4 | 4 |
| fill `maxPixels` | 1 Mpx | none | none |
| model texture anisotropy | 1 | 4 | 16, device-clamped |
| model texture mip limit | 1 | 0 | 0 |
| probe size | 64 | 128 | 256 |

## Built (2026-10-17)

- core: `quality.ts` (`quality`, `setQuality`, `QUALITIES`, `Quality`).
- alloy: `SamplerState.base_level` (texture-object state,
  `TEXTURE_BASE_LEVEL`, applied at every create and dropped to 0 around
  each `glGenerateMipmap` so the chain derives from level 0),
  `SamplerUpdate` + `Context::set_texture_sampling` (every axis but the
  chain, a target's output sampling included, `baseLevel` refused on
  targets and depth ids), `Context::set_target_samples` over the
  resize path (`ShaderTexture::reshape`, `make_msaa` factored out of the
  create, the create's clamp-and-fall-back rule kept),
  `Spatial::set_lod_fade` (a per-target hard switch, forgotten with the
  view). The inventory reports `baseLevel` with the sampler.
- flux: `setTargetSamples`, `setTextureSampling`, `baseLevel` in the
  create options, `setLodFade`; flux-types updated.
- 3d: `quality.ts` (row type, table, resolve, validation,
  `textureSampling`); `createScene` resolves every knob against the row
  with the explicit value beside it, `setQuality` / `quality()` /
  `setSamples` on the scene, `setSamples` on a view, `setLodBias` and
  `setShadowQuality` take null for "the row's", a scene under an owner
  follows the app tier through an effect; the shadow system resolves
  `mapSize` and `cascades` through the row (`defaults` dep); light
  records carry `mapSize: number | null`, `cascades: number | null`,
  `camera: ShadowCamera | null`; `createModel` samples textures per the
  app tier and one app-lifetime effect re-samples every live model
  texture; `<Scene quality lodBias>` props, `samples` live, the row on
  the scene context (`useScene().quality()`), the fill cap default on
  Scene and View3d; the-third-dimension demo gained a `quality` debug
  command; AGENTS.md has a Quality section with the table.
- Tests: `packages/3d/tests/quality.test.tsx` (rows through the
  inventory: samples, taps, the atlas size per map edge and cascade
  count, overrides, the fill cap, validation),
  `packages/core/tests/gpu-sampling.test.tsx` (live sampling and sample
  counts, the chain rules).

## Findings

Appended during the work, per the rule in [../README.md](../README.md).

- GL derives a mip chain from `TEXTURE_BASE_LEVEL` down, so a texture
  with a raised base level must drop it to 0 around every
  `glGenerateMipmap` (uploads regenerate per frame for a streaming
  texture) and restore it after; left in place, the levels under the
  base would be derived from the base level's stale content and level 0
  would never reach the chain. A render target keeps its chain
  regenerated from level 0 after every render, which is why a base
  level is a data texture's knob only.
- The sample count is a property of the mesh storage the resize path
  already rebuilds: `attach_storage` sizes color, depth and multisample
  storage together, so a live count change is the resize with the
  `Msaa` member replaced (fresh explicit names, or the in-tile flavor's
  count) and the old explicit names deleted after the FBO completes.
  The create's fall-back rule (a refused count renders single-sample
  with a warning) carries over unchanged.
- A model's textures are the reason the tier is app-wide: a texture id's
  sampling is shared by every scene that draws the model, and
  `createModel` runs outside any scene. The 3d package keeps a registry
  of live model textures with their chain lengths and one app-lifetime
  effect (created under no owner, like core's device facts) re-samples
  them; a texture leaves the registry with the model's dispose.

- The TV and laptop rows of 2026-10-17 (the tier set live on the
  the-third-dimension demo), with the open gap between today's low row
  and the September reading of the same configuration, are in the
  Findings of
  [3d-low-end-gpu-performance](../backlog/3d-low-end-gpu-performance.md).

## What it involves

- The table itself, with the device rows as evidence: the SM-T500 and TV
  rows for shadows, the LOD and fill measurements in the notes, so each
  row is a measured combination and not a guess.
- `scene.ts`: resolve each nullable knob against the tier where it is
  read, the way `writeLights` resolves `shadow.radius`; `setQuality`
  marks what the tier touches dirty (the light set, the LOD bias, the
  target format for samples - the last one recreates the target, so it
  is the expensive one and may stay creation-only).
- `light.ts`: `mapSize` and `cascades` nullable, the tier's row the
  default; the atlas budget (8 slots) does not change.
- Docs: the `SceneOptions` comment and the AGENTS.md table name the tier
  once and each knob says "default: the tier's".
