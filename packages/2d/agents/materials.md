# @solidrt/2d - materials

Sprite materials and their shaders. Read ../AGENTS.md first: it carries
the model this file builds on.

## Materials

What a layer's pixels look like is its MATERIAL (material.ts), one per
layer, fixed at creation (`createSpriteLayer(atlases, { material })`,
the records and tile layers alike, `<SpriteLayer material>` /
`<TileLayer material>`): the material sits on the thing that is one
draw entry - the layer is the batch - as @solidrt/3d's sits on the
mesh, so two looks are two layers, which is also how you order them
(the additive glow layer draws over the scene layer). The same model
and the same words as 3d, deliberately: `unlit()` is the stock
material, `prelude`/`surface` its tier-2 slot, `shaderMaterialClass`
/ `shaderMaterial` the class form over the layer's GLSL set,
`instanceBuffers` / `instanceStyle` / `setInstanceStyle` /
`instanceAttribute` the per-sprite style record (the comparison and
the decisions: okf/plans/materials-one-model.md; Godot's
`CanvasItem.material` with a `canvas_item` shader is the nearest
engine shape, PixiJS and Phaser the batch-per-look precedent). The one
deliberate asymmetry: a 3d `blend` implies `transparent` and the
scene's back-to-front sort; a layer has no depth and draws in record
order, so nothing sorts.

- `unlit({ blend?, prelude?, surface?, params?, textures?,
  instanceBuffers?, instanceStyle?, label? })` - the atlas texel times
  the sprite's tint (a distance-field atlas decoded to its fill and
  outline), times the layer tint, blended with `blend` ("alpha" by
  default, "add" for glows and additive particles, "multiply" to
  darken, "none" to overwrite; pipeline state, so per layer). Every
  layer draws with `unlit()` unless told otherwise. The TIER-2 slot:
  `surface` declares `void surface(inout Sprite s)`, called with the
  struct filled and before the layer tint - `color` (the stock result,
  premultiplied: texel times tint, or the decoded field), `uv`,
  `frame`, `tint`, `atlas` - rewrite `s.color` or `discard`;
  `prelude` is file scope after the set (uniforms, each a LAYER PARAM;
  helpers); `spriteSample(uv)` resamples the fragment's own atlas at
  another uv (the pick, the frame clamp, the field decode, the tint),
  so a scroll is `spriteSample(mix(s.frame.xy, s.frame.zw,
  fract(t)))` - the clamp into the frame still applies, so a wrap is
  `fract` in FRAME space. One program per distinct source, cached for
  the app's lifetime like 3d's.
- PER-SPRITE DATA: `instanceBuffers: [{ attributes: [{ name, format }]
  }]` on either form declares the material's STYLE record(s), one
  stream per buffer bound after the layer's own records, any vertex
  format (a palette index is a `uint8x4` at four bytes, a dissolve
  amount a `float16x2`), every slot starting as `instanceStyle` (the
  identity of what the look does with it). On `unlit` the stock
  vertex stage forwards each attribute unchanged as a flat varying -
  `iPalette` reaches the prelude and the surface as `vPalette`
  (`forwardedName`; names follow the layer's own `i` + capital
  convention) - so a palette swap is one attribute, one surface line
  and a `setInstanceStyle(sprite, [row])` per sprite. Writes:
  `setInstanceStyle(sprite, values)` (the first buffer, one
  value per component as the shader sees them; marks the slot like
  setSprite), `instanceAttribute(layer, name)` (the accessor: slot in,
  component values out and in through the format's codec; valid
  across growth) plus `updateRecords(layer, { stream, first, count
  })`, or `records(layer, stream)` in bulk (stream 1.. the material's
  buffers in order; a Float32Array over an all-float layout, bytes
  otherwise), `<Sprite style>` in the component face, `setTile(...,
  { style })` on a tile layer. A recycled slot (and a records layer's
  shifted one) starts blank again.
- LAYER PARAMS: `layer.setParams({ uFlash: 0.5 })` writes a prelude's
  uniforms to every view (and every resident chunk of a tile layer,
  which RE-BAKES on each write like its tint) over the material's own
  `params`; the `params` option and prop seed them, a view created
  later inherits the merged record. @solidrt/3d's setMeshParams one
  dimension down. Material `textures` (a palette LUT, a noise texture)
  bind beside the atlases and count against the same
  `limits.maxTextureUnits` budget, checked at layer creation.
- The class form, `shaderMaterialClass({ vertex, fragment,
  instanceBuffers?, instanceStyle?, blend?, cullMargin?, label? })` with
  `instance({ params?, textures? })` and `dispose()` on the class
  (`shaderMaterial(opts)` is a class with one instance): TIER 1 pairs
  a vertex stage of your own with `unlitFragment(options)` from
  `@solidrt/2d/glsl` - compose `SPRITE_VARYINGS`, core's
  `SCREEN_SIZE_GLSL`, your placement into `unit`/`corner`/`center`/
  `rot`/`clampScale` (`unit` from `spriteCorner`, see Traps), then
  `SPRITE_VERTEX_BODY`; `SPRITE_VERTEX` is the
  stock stage, `unlitVertex(forwards)` the stock stage forwarding
  attributes - and TIER 3 writes a `main` of its own over the layer's
  SET, which the layer prepends (the varyings, `uAtlasN`, `uTint`, the
  `Sprite` struct, `spriteOf()`, `spriteSample(uv)`: declare none of
  them, start with `Sprite s = spriteOf();` since the derivatives
  every tap uses are taken there, and write premultiplied). One
  program per ATLAS LIST met (the set is generated per list), one
  pipeline per instance-layout list met on top - the records and tile
  layers bind one record buffer, the node layer a pose and a sprite
  buffer, with the same attribute NAMES on both (iPos, iRot, iScale,
  iUv, iTint, iScreenPx, iAtlas, iOutline), so one vertex stage
  serves every layer kind. A vertex stage must declare and use
  `uCamera`, `uCameraRot` and `uViewport` (checked at creation), an
  instance attribute may not reuse a layer name
  (`LAYER_ATTRIBUTE_NAMES`), a texture may not take an atlas sampler's
  (`uAtlasN`). A vertex stage that moves geometry beyond the sprite's
  rect (a sway, a wobble, a padded quad) declares how far at most, in
  layer pixels, as `cullMargin`: the layers cull by default, and grow
  every cull box by it, so a sprite whose rect is just off screen but
  whose drawn quad reaches in still draws (Godot's
  `extra_cull_margin`). Without it such a sprite vanishes early at the
  view's edge, and a write to it re-renders only its box, leaving stale
  pixels where the quad drew outside it (views.md, View culling); an
  unbounded move wants `cull: false` on its layer.
  `tests/materials.test.tsx` pins all three tiers, the
  style streams through growth and shift, the params, the resample,
  the tile bake and the throws.

## Traps

- A custom vertex stage never declares or reads `aPos` (a throw at
  `shaderMaterialClass`): the layer prepends the corner input and
  `vec2 spriteCorner(vec2 size, float zoom, float outlinePx, vec4 uv)`,
  the vertex in unit-quad space (-0.5..0.5 over the frame). Call it with
  the drawn world size (`iScale * clampScale`), `uCamera.z`, the outline
  in screen pixels (`iOutline.a * uCamera.z * clampScale`, 0 without
  one) and `iUv`, then place `corner = unit * size` and map the uv
  through `unit` (SPRITE_VERTEX_BODY reads it): on a quad layer it is
  the quad's corner, on a hull layer (an atlas with `mesh: "hull"`) the
  frame's mesh point pushed out and mirrored, so one stage serves both.
- A custom vertex stage tests the size first, as the stock one does:
  right after `vec2 size = iScale * clampScale;`, write
  `if (spriteDrawsNothing(size)) { gl_Position = SPRITE_NOWHERE;
  return; }` (both prepended with `spriteCorner`). A zero-size record
  is how a slot stays free (a reserved capacity, a particle pool at
  rest), and without the test each of its vertices still unpacks the
  mesh and goes through the camera to draw nothing.
- A `surface` pays its work in every fragment its quad covers, the
  transparent corners included: a surface that samples or takes
  derivatives before it returns on a transparent pixel (derivatives want
  uniform control flow, so the early return comes after them) costs that
  work in every empty corner. Over a sheet of irregular shapes, a hull
  atlas (`mesh: "hull"`) removes most of those fragments; returning
  early does not.

- A `surface` reads the struct, never a local of the generated program:
  `s.color` is premultiplied and BEFORE the layer tint (`uTint`
  multiplies after the slot), so a rewrite to an opaque colour is
  `vec4(rgb, 1.0) * s.color.a` to keep the sprite's coverage.
- A custom `main` (tier 3) starts with `Sprite s = spriteOf();`: the uv
  derivatives every tap uses are taken there, in uniform control flow.
  A `spriteSample` before it reads zero derivatives and picks the
  sharpest mip everywhere.
- `spriteSample(uv)` clamps into the FRAME like the stock tap: a uv
  past the frame's edge lands on the edge texel, never on the next
  cell. Wrap in frame space (`fract`) before mixing into the frame rect.
- The stock stage forwards a material's instance attributes as FLAT
  varyings named `v` + the name without its `i` - `iPalette` is
  `vPalette` in the surface; a name not spelled `i` + capital throws at
  `unlit()`. A custom vertex stage forwards what it likes.
- A material's `textures` and the atlases share one sampler budget
  (`limits.maxTextureUnits`, 16): a LUT and a noise texture leave room
  for fourteen sheets, and the layer creation throws past it.
- A tile layer BAKES: `setParams` on it re-renders every resident chunk,
  like `setTint`. Animated uniforms belong on a sprite layer, whose
  params are one shared write per view.
- The stock material classes are cached for the app's lifetime (one per
  blend x prelude x surface x layouts, like 3d's unlit); a
  `shaderMaterialClass` is yours to `dispose()`, and its programs are
  compiled per atlas LIST met - two layers over the same atlases share
  the program, two over different lists compile twice.
