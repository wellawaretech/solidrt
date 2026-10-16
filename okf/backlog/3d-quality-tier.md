---
title: One quality tier for the whole 3d package
description: The package's quality knobs each stand alone - shadows.quality, lodBias, samples, the fill density (renderScale / maxPixels), the lights' map sizes and cascade counts - so an app that wants "low-end" or "high-end" sets five things in five places and a knob it forgets keeps the medium default; one tier every knob defaults from, each knob an override, as Unity's quality settings and Godot's project settings do, and the question whether the tier lives on the Scene or on the app as a device profile.
created: 2026-10-16
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
