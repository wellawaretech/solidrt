---
title: A sprite layer has one hardwired pipeline, so there is no additive blend and no custom fragment
description: Every 2d draw goes through one alpha-blended pipeline with tint as the only knob, so explosions, glows, palette swaps, dissolves and outlines have no path at all, while @solidrt/3d ships four stock materials, a custom shader class and a per-material blend mode.
created: 2026-09-11
---

# A sprite layer has one hardwired pipeline

## Symptom

`createSpritePipeline` compiles the one vertex/fragment pair every 2d draw
uses, with `blend: "alpha"` written into the call
(`packages/2d/src/shaders.ts`). Nothing in the public surface reaches it:
`SpriteLayerOptions` takes `capacity`, `tint`, `label`, `stagger`,
`autoFree` and `orderBy`, and a sprite carries `frame`, `tint`, `flipX`,
`flipY`, `rotation` and `renderOrder`. So the shader is a closed box and
the only per-sprite colour control is a multiply.

What that costs, in the order a game hits it:

- **No additive blend.** Explosions, muzzle flashes, glows, light cones,
  particle sparks and most screen effects are additive. There is no way
  to ask for one, per sprite or per layer.
- **No custom fragment.** Palette swap (the pixel-art idiom: one indexed
  sheet, N palettes), hit flash beyond a white tint, dissolve, outline,
  chromatic damage effect, scrolling UV for water. Each is a few lines of
  GLSL and none of them is reachable.
- **No custom vertex.** Already reported from another direction:
  [2d-screen-space-sprite-size](2d-screen-space-sprite-size.md) wants a
  minimum on-screen size computed in the vertex stage, and its symptom
  section states there is "no shader-side answer", so apps rewrite `w`/`h`
  from JS every camera change.

`@solidrt/3d` answers all three: `unlit`/`lit`/`standard`/`sprite` as
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

The layer is the batch. One atlas, N quads, one draw per view, and the
instance buffers are the layer's. A material per SPRITE would mean a
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

1. `blend` on `SpriteLayerOptions` and `<SpriteLayer>` (and the record and
   tile layers, which share the pipeline builder), taking core's
   `BlendMode`. An additive layer is then two lines.
2. A custom fragment over the layer's varyings (`vUv`, `vFrame`, `vTint`)
   with app params, modelled on `shaderMaterialClass`, plus the sampler
   and param plumbing views already have.
3. A custom vertex stage, which is what
   [2d-screen-space-sprite-size](2d-screen-space-sprite-size.md) needs;
   the instance attribute layouts become part of the contract at that
   point, so it wants the open-format work
   [3d-vertex-data-model](../done/3d-vertex-data-model.md) did for 3d
   rather than a second hardwired list.

Stage 1 is additive. Stages 2 and 3 are additive to it as long as the
stock pipeline stays the default, so the staging holds the no-breaking-
changes rule.

## Involves

`packages/2d/src/shaders.ts` (the pipeline builder already takes a vertex
source and attribute layouts, so it is close to parameterized),
`layer.ts`/`records.ts`/`tiles.ts` at the three call sites,
`components/sprite-layer.tsx` for the prop, and the AGENTS.md model
section, which currently states the one-pipeline rule as a fact about the
package.
