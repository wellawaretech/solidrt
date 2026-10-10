---
title: One material model for the 2d and 3d extensions
description: A 2d layer had one hardwired shader pair with blend as its only knob, and the 3d custom-look tiers had settled after that note was written; both packages now share one model - a material on whatever is one draw, blend on the material, a surface function in the stock material, a shader class over the package's set, and an app-owned per-instance style record - with the vertex codec in core.
created: 2026-09-11
completed: 2026-10-10
---

# One material model for the 2d and 3d extensions

## Symptom

Every 2d draw went through one fixed vertex/fragment pair
(`packages/2d/src/shaders.ts`) with the per-sprite tint and the layer's
`blend` option (landed 2026-10-05) as the only knobs. Palette swaps, hit
flashes, dissolves, outlines beyond the distance-field one, scrolling
UVs: each is a few lines of GLSL and none had a path. `@solidrt/3d`
meanwhile had `unlit`/`phong`/`standard`/`sprite`, `shaderMaterialClass`
with instance buffers of any vertex format, and the three custom-look
tiers decided in
[3d-custom-material-scene-effects](../done/3d-custom-material-scene-effects.md):
a function-level contract (a `surface` function over a struct), neither
injection nor bare composition.

The first shaping of this note (2026-09-11) proposed a custom fragment
body appended after the generated preamble, calling a generated `tap()`.
That design went stale twice: the fragment stage grew a distance-field
branch per atlas (world-space text), so no raw texel exists for an app to
compose over, and the 3d side chose the function slot over the body.
Re-decided 2026-10-10 with the scope widened to both packages.

## Three, Unity, Godot, and the 2D batch renderers

| | per-object material | blend | custom shader | per-object params |
| --- | --- | --- | --- | --- |
| Unity 2D | `SpriteRenderer.material` (Sprite-Unlit / Sprite-Lit) | in the shader | Shader Graph, Sprite targets | MaterialPropertyBlock, breaks the batch |
| Unity 3D | `Renderer.material` | in the shader | Shader Graph, ShaderLab | the same |
| Godot 2D | `CanvasItem.material` | `CanvasItemMaterial.blend_mode` | `canvas_item` shader: `vertex()`, `fragment()`, `light()` over built-ins (TEXTURE, UV, COLOR) | shader params per material; `instance uniform` is 3D only |
| Godot 3D | `MeshInstance.material` | in the material | `spatial` shader: `fragment()` writes ALBEDO, NORMAL, EMISSION | shader params, instance uniforms |
| Three | `Sprite.material`, `Mesh.material` | `material.blending` | `ShaderMaterial`; `onBeforeCompile` for stock surgery | uniforms per material; one draw per object |
| PixiJS v8 / Phaser 3 | the batcher shader is fixed; Phaser `setPipeline` per object, the batch flushes on change | per pipeline | a custom pipeline (Phaser), a Mesh outside the batch (Pixi) | none inside a batch |

Two readings. Unity, Godot and Three put the material on the object and
let the batcher break on it. The 2D batch renderers put it on the batch
and give objects nothing but a tint. SolidRT takes the batch renderers'
attachment (a 2d layer IS the batch) with Godot's shader contract (a
function the package's program calls over named built-ins), which is
what 3d already is.

## Decisions

**Attachment.** A material sits on the thing that is one draw entry: a
mesh in 3d, a layer in 2d. Stated once in both agent notes. A material
per sprite would be a draw per material, Unity's batch-breaking failure
mode, and undoes the package.

**Blend lives on the material, in both.** The 2d layer `blend` option and
the component prop are gone; an additive layer is `material: unlit({
blend: "add" })`, the 3d spelling. Deliberate asymmetry, recorded: 3d's
`blend` implies `transparent` and a back-to-front sort; 2d has no depth
and draws in record order, so nothing sorts.

**Stock material names.** 2d's one look is `unlit()`: the name says the
shading model, which is the axis a later `lit()` (2D lights, Unity's
Sprite-Lit, Godot's `light()`) differs on. 3d's `sprite()` names a
billboard behaviour with no 2d meaning, so mirroring it would be a false
match.

**Three tiers, the same on both sides.**

1. Stock fragment, custom vertex. 2d exports its vertex pieces
   (`SPRITE_VARYINGS`, `SPRITE_VERTEX_BODY`, `SPRITE_VERTEX`) and
   `unlitFragment(options)` from `@solidrt/2d/glsl` as 3d exports
   `LIT_VERTEX` and `phongFragment`. One vertex stage serves both 2d
   record layouts now that they share attribute names (the records
   layout's `iCenter`/`iSize` became `iPos`/`iScale`).
2. A `surface` function inside the stock material: `unlit({ prelude,
   surface })` declares `void surface(inout Sprite s)`, called once the
   generated program has filled the `Sprite` struct (`color`: the stock
   result, premultiplied, before the layer tint; `uv`, `frame`, `tint`,
   `atlas`). Rewrite `color` or `discard`. `spriteSample(uv)` resamples
   the fragment's own atlas (pick, frame clamp, field decode) at another
   uv for scrolling and distortion; the clamp into the frame still
   applies, so a wrap is `fract` in frame space. 3d's `unlit` and
   `sprite` take the slot too, so every stock material on both sides has
   it (`sprite`'s radial falloff moved out of the slot into a source
   flag to free it).
3. A fragment of your own over the package's set: `shaderMaterialClass`
   and `shaderMaterial` in 2d with the 3d signatures (`instanceBuffers`,
   `instanceStyle`, `blend`, `instance({ params, textures })`,
   `dispose`). The one necessary asymmetry: 2d's set is generated per
   atlas list (one sampler per atlas, the sdf decodes), so the LAYER
   prepends `spriteSource(atlases)` and the class compiles one program
   per atlas list met - 3d composes `sceneSource` by template and
   compiles one pipeline per geometry layout met. The set is an exported
   function, so what runs is inspectable, which is the line the 3d
   decision drew against injection. The vertex contract is checked at
   creation the way 3d checks for `uModel`/`uViewProj`: a 2d vertex
   stage must declare and use `uCamera`, `uCameraRot` and `uViewport`.

**Per-sprite data, 3d's words unchanged.** A material declares
`instanceBuffers` of any vertex format and an `instanceStyle` fresh
record; the layer appends them as record streams after its own, and the
app writes them with `setInstanceStyle(layer, sprite, values)`,
`instanceAttribute(layer, name)` plus `updateRecords(layer, { stream })`,
or `records(layer, stream)` in bulk. The naming clash on the 2d side is
resolved by renaming what 2d's notes called the style record: the three
instance streams are the pose (core-written), the SPRITE record (the
layer's: what its verbs write and picking reads, `SPRITE_FLOATS`) and
the STYLE (the material's, app-owned). A material reads the sprite
record, never defines it. A tile layer takes the same material; a cell's
style is `setTile(..., { style })`.

**The codec moves to core.** The vertex-format table, the layout helpers
and the record accessors (`VERTEX_FORMATS`, `layoutStride`,
`attributeAccess`, `encodeRecord`, ...) lived in `packages/3d/src/
geometry.ts`; the 2d style streams need them and 2d must not import 3d,
so they are `@solidrt/core/gpu`'s (`packages/core/src/vertex.ts`), with
the 3d presets staying in 3d. The rule both package CLAUDE.md files
state.

**Params.** `layer.setParams(params)` is `setMeshParams` one dimension
down (a method beside `setTint`, the layer's other layer-level verbs
being methods): the layer fans the write out to every view and chunk
target, over the material's own `params`, and a view created later
inherits the merged record. Material textures share the sampler budget
with the atlases, validated at layer creation where the two lists meet
(`checkAtlases` takes the material's sampler count).

**The stock stage forwards the style record.** A fragment cannot read
an instance attribute, so tier 2 on `unlit` would have needed a custom
vertex stage for every per-sprite look. Instead `unlit({ instanceBuffers
})` makes the stock vertex stage forward each attribute unchanged as a
flat varying (`iPalette` to `vPalette`, the layer's own `i` + capital
convention, checked at `unlit()`), and the generated fragment declares
the matching `flat in`s: a palette swap is one attribute, one surface
line and a `setInstanceStyle` per sprite. Godot's instance uniforms
reaching `fragment()`, in effect.

**One vertex stage for every layer kind.** The records layout's
`iCenter`/`iSize` became `iPos`/`iScale`, the pose record's names, so
the stock stage (and any custom one) reads its attributes by the same
names whichever buffers a layer binds; a material compiles one program
per atlas list and one pipeline per layout list met.

**Tiles.** A material bakes; a param change marks every resident chunk
dirty, so animated uniforms on a tile layer cost a rebake per change,
like its tint. Documented as the cost it is.

## Involves

core: `vertex.ts` (new), `gpu.ts` re-exports. 3d: `geometry.ts`
delegating to core, `mesh.ts` on core's record codec, `material.ts`
(`prelude`/`surface` on `UnlitOptions`, forwarded by `unlit` and
`sprite`), `glsl.ts` (`radial` as a source flag), AGENTS.md. 2d:
`glsl.ts` (new: the set, the struct, the vertex pieces, `unlitFragment`),
`material.ts` (new), `shaders.ts` reduced to the layouts, `atlas.ts`,
`layer.ts`, `records.ts`, `tiles.ts`, `views.ts`, the three components,
`index.ts`, `package.json` (`./glsl`), AGENTS.md, README.md, the
markers example, `tests/screen-floor.test.tsx` and a new
`tests/materials.test.tsx`.

## Outcome (2026-10-10)

Landed as decided, all suites green (`sol test packages/2d`: 127,
`packages/3d`: 178, `packages/core` unchanged), `sol check` on core, 2d
and 3d:

- core: `packages/core/src/vertex.ts` (the format codecs, the layout
  arithmetic, `attributeAccess`, `layoutFields`, `writeRecord`,
  `encodeRecord`, `checkLayout`), re-exported from `@solidrt/core/gpu`;
  3d's geometry.ts keeps the presets and delegates, mesh.ts encodes
  through core.
- 3d: `prelude`/`surface` on `UnlitOptions`, so `unlit` and `sprite`
  take the tier-2 slot (a surface attaches the unlit shadow twin like
  phong's; the sprite's radial falloff is a `radial` flag of
  `unlitFragment` now, no longer occupying the slot). AGENTS.md states
  the shared tiers and the one asymmetry.
- 2d: `glsl.ts` (the set with the `Sprite` struct, `spriteOf`,
  `spriteSample`; `unlitVertex(forwards)`, `SPRITE_VERTEX_BODY`,
  `unlitFragment`; the three layouts under one attribute vocabulary),
  `material.ts` (`unlit`, `shaderMaterialClass`, `shaderMaterial`),
  `style.ts` (the material's style streams on core record streams);
  `blend` on the layers and the components gone in favour of
  `material`; `params` option/prop and `setParams` on all three layer
  kinds; `setInstanceStyle`, `instanceAttribute`, `records(layer,
  stream)`, `updateRecords({ stream })`, `<Sprite style>`, `setTile({
  style })`; the node layer's second slot renamed the SPRITE record
  (`SPRITE_FLOATS`). `tests/materials.test.tsx` (2d, nine tests) and
  `tests/materials.test.tsx` (3d, two) pin it; `shaders.ts` is gone.

Not done on purpose: a 2d `lit()` (2D lights) is an idea, not a
leftover; the Unity-style per-object material stays rejected.
