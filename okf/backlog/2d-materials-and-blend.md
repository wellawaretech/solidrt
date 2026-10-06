---
title: A sprite layer has one hardwired fragment and vertex stage, so there is no custom shader
description: Every 2d draw goes through one fixed shader pair with tint and the layer blend as the only knobs, so palette swaps, dissolves, outlines and scrolling UVs have no path at all, while @solidrt/3d ships four stock materials and a custom shader class.
created: 2026-09-11
---

# A sprite layer has one hardwired fragment and vertex stage

## Symptom

`createSpritePipeline` compiles the one vertex/fragment pair every 2d draw
uses (`packages/2d/src/shaders.ts`). The blend mode reaches it since
2026-10-05 (stage 1 below, landed with
[2d-screen-space-sprite-size](../done/2d-screen-space-sprite-size.md)):
`blend` on `SpriteLayerOptions`, `RecordLayerOptions` and
`TileLayerOptions` and the matching component props, so an additive layer
is two lines. Nothing else does: a sprite carries `frame`, `tint`,
`flipX`, `flipY`, `rotation`, `minScreenPx`, `maxScreenPx` and
`renderOrder`, so the
shader is a closed box and the only per-sprite colour control is a
multiply.

What that costs, in the order a game hits it:

- **No custom fragment.** Palette swap (the pixel-art idiom: one indexed
  sheet, N palettes), hit flash beyond a white tint, dissolve, outline,
  chromatic damage effect, scrolling UV for water. Each is a few lines of
  GLSL and none of them is reachable.
- **No custom vertex.** The screen-size clamp
  ([2d-screen-space-sprite-size](../done/2d-screen-space-sprite-size.md))
  went into the stock vertex stage as per-sprite fields; the next
  vertex-side want (a billboard, a wobble, a wind sway) has no path.

`@solidrt/3d` answers all three: `unlit`/`phong`/`standard`/`sprite` as
stock materials, `shaderMaterial`/`shaderMaterialClass` for custom GLSL,
`setMaterial` per mesh, `overrideMaterial` per view, and `blend` on any
material with "add" called out for glows
([3d-custom-material-scene-effects](../done/3d-custom-material-scene-effects.md)).

## Three, Unity, Godot

All three give a per-object material, and at minimum a blend mode:

| | material | blend | custom shader |
| --- | --- | --- | --- |
| Three | `SpriteMaterial`, `MeshBasicMaterial` | `blending: AdditiveBlending` and the rest | `ShaderMaterial` |
| Unity | `SpriteRenderer.material`, `Sprites-Default` / URP Sprite-Lit / Sprite-Unlit | in the shader, one material per blend | Shader Graph (Sprite targets) |
| Godot | `CanvasItem.material` | `CanvasItemMaterial.blend_mode` (Mix, Add, Sub, Mul, Premult) | `ShaderMaterial`, `canvas_item` shader |

Godot is the closest fit: a blend mode is a small enum on a cheap
material, a custom shader is the same slot holding something else, and
the renderer batches by material. Unity is the warning: a per-object
material that differs breaks the batch, which is exactly the cost our
model exists to avoid.

## What makes this awkward here

The layer is the batch. Its atlases bound together, N quads, one draw per
view, and the instance buffers are the layer's. A material per SPRITE would mean a
draw per material, which is the thing the package is built not to do.
So the shape question to settle first is where a material attaches, and
the candidates are not equal:

- **Per layer.** The layer already owns its pipeline, so a `blend` option
  and a custom fragment fit with no change to the draw model at all. Two
  blend modes means two layers, which is also how you would order them
  (additive effects draw over the scene). This is the cheap, obvious
  step, and it is probably most of the value.
- **Per sprite, selected in the shader.** The style record already
  carries eight JS-written floats per sprite; a mode float the fragment
  branches on buys per-sprite variation inside one draw, at the cost of a
  branch and a fixed menu of modes. Blend state cannot come this way
  (it is pipeline state, not shader state), so this only answers the
  fragment half.
- **Per sprite, per draw.** Rejected: it is Unity's batch-breaking
  failure mode and it undoes the package.

The 3d precedent to lean on is `shaderMaterialClass`: the class owns the
pipeline and instances own params. That maps onto a layer owning the
pipeline exactly, which suggests the 2d spelling is a material passed to
`createSpriteLayer`, defaulting to the stock one.

## Done looks like

Staged, each with value on its own:

1. Landed 2026-10-05: `blend` on `SpriteLayerOptions`,
   `RecordLayerOptions` and `TileLayerOptions` (one pipeline builder
   behind all three) and on `<SpriteLayer>` / `<TileLayer>`, taking core
   gpu's `BlendMode`; pipeline state, fixed at creation. Pinned by
   tests/screen-floor.test.tsx (two half-alpha whites: "alpha" composites
   to three quarters, "add" to opaque white).
2. A custom fragment over the layer's varyings (`vUv`, `vFrame`, `vTint`,
   `vAtlas`) with app params, modelled on `shaderMaterialClass`, plus the
   sampler and param plumbing views already have. Since
   [2d-atlas-limits](../done/2d-atlas-limits.md) the fragment stage is
   GENERATED per layer (`fragmentFor(count)` in shaders.ts): one sampler
   per declared atlas, the record's atlas index picking it, derivatives
   taken before the branch, `textureGrad` as the tap. A custom fragment
   must not re-derive any of that, so the hook is the generated `tap()`:
   the app's source is appended after the generated preamble and calls
   `vec4 texel = tap(dx, dy)` (or a variant taking a uv, for scrolling
   and distortion) and composes on top of it - palette lookup, dissolve,
   outline - never naming `uAtlasN` or branching on `vAtlas` itself. The
   stock fragment then becomes the one-line default of the same hook.
   Two consequences to design in: the app's own samplers (a palette LUT,
   a noise texture) count against the SAME `limits.maxTextureUnits`
   budget as the atlases (a layer's targets bind nothing else today, so
   `checkAtlases` assumes the whole budget is its own; with a material
   the cap is `maxTextureUnits - the material's samplers`, validated
   where the material and the atlas list meet), and a frame's clamp
   into its own atlas's texel grid lives inside `tap`, so a custom uv
   passed to it is clamped the same way - document that a scrolling UV
   wraps within the frame, not across the sheet.
3. A custom vertex stage; the instance attribute layouts (the 16-float
   record, the pose/style pair) become part of the contract at that
   point, so it wants the open-format work
   [3d-vertex-data-model](../done/3d-vertex-data-model.md) did for 3d
   rather than a second hardwired list, and the stock clamp stays a
   function a custom stage can call (core's `SCREEN_SIZE_GLSL`).

Stages 2 and 3 are additive to the stock pipeline staying the default, so
the staging holds the no-breaking-changes rule.

## Involves

`packages/2d/src/shaders.ts` (the pipeline builder takes a vertex source,
attribute layouts, a blend mode and the atlas count; `fragmentFor` is
where the generated preamble and the app's body meet), `atlas.ts`
(`checkAtlases` learning the material's sampler count),
`layer.ts`/`records.ts`/`tiles.ts` at the three call sites,
`components/sprite-layer.tsx` for the prop, and the AGENTS.md model
section ("Atlases and frames" states the generated-stage contract).
