---
title: Replace Impeller with an own renderer, in stages
description: An own renderer (anvil) behind the renderer seam, built on a branch that deletes Impeller outright, staged so minimal apps run on it first (rects, ovals, lines, texture quads, text through its layers, transforms, rect clips) and the binary size is measured against main at the first commit; then batching and rounded shapes, shaders drawn straight into the frame, paths, full clips, layers and effects, and parity. The end state draws text and generator shaders straight into the frame and keeps offscreen passes only where a pass reads its own input.
created: 2026-11-17
---

# Replace Impeller with an own renderer, in stages

## Why

[replacing-impeller](../notes/replacing-impeller.md) has the analysis: what
Impeller still does for us, the sizing, the parity tail, and the iOS
arithmetic. Two things have changed since it was written:

- The renderer seam landed ([renderer-seam](../done/renderer-seam.md)).
  A second renderer is a module beside `alloy/src/renderer/impeller.rs`
  that implements `contract.rs`. Nothing outside `renderer/` changes.
- Text is ours ([text-own-rasterizer](../done/text-own-rasterizer.md)).
  The note's largest row, the text engine, is built. A `<text>` reaches
  Impeller as one `draw_texture_rect` of its layer.

The performance case is
[display-list-op-cost](../backlog/display-list-op-cost.md): about 30 us of
Impeller's CPU per op on the armv7 TV, paid even for ops the clip rejects,
so 800 ops miss the 20 ms budget at 50 Hz. The quality case is the
`upstream/impeller-*` defects nobody can fix for us.

## The end state

- The own renderer is the only renderer, on every platform. Impeller and
  `impellers` are gone.
- Everything that can draw straight into the frame does: shapes, paths,
  images, text (glyph quads from the shared atlas, no layer per text), and
  generator shaders (a program drawn as a quad under the frame's clip).
- An offscreen pass exists only where a pass reads its own input: a node
  shader that samples `uSource` or `uPrevious`, group opacity, filters,
  blur, backdrop capture, snapshot boundaries.
- The dense-field mover probe runs locked at 50 fps on the TV.

Stages 1 and 2 reach the end state's shape for minimal apps. The text
layers and the offscreen node shaders are interim mechanisms, kept because
they work through `draw_texture_rect` with no extra renderer work.

## How the stages run

- **One renderer, on a branch.** The work happens on the `own-renderer`
  branch, which deletes Impeller outright: `alloy/src/renderer/impeller.rs`
  goes, the `impellers` entries leave `alloy/Cargo.toml` (all four target
  tables, with the armv7 live-bindgen case and the SDK download), and the
  aliases in `renderer/mod.rs` point at the new module,
  `alloy/src/renderer/anvil.rs`. No cargo feature, no passthrough, no
  switch: a build holds exactly one renderer, as the seam intended. The
  Android staging step in `packages/cli/runtime/runtime.mk` that copies
  `libimpeller.so` into the APK is removed when the branch first builds
  Android. `make renderer-seam-check` goes with the module it checked.
  The constraint this buys: the branch cannot merge before stage 7. If it
  must land earlier, the feature switch is reintroduced then, not now.
- **The whole contract compiles from stage 1.** `mod.rs` checks at compile
  time that the renderer implements every trait. A method a stage does
  not support yet records nothing and warns once per process, naming the
  op ("Renderer: clip_oval is not supported yet; the clip is ignored").
  The content draws as if the op were absent, or as the nearest supported
  op where one exists (a rounded rect as a rect), so a screen degrades and
  does not break. The dev client's player uses components, which fade
  through group opacity, so its start screen shows wrong until stage 6;
  `sol render` and `sol test` do not go through the player.
- **The binary size is measured at the first commit.** A stub module that
  implements every trait as a warn already builds a client without
  Impeller, and the finished renderer adds a few hundred kilobytes at
  most, so the number does not wait for pixels. Main and the branch are
  built the same way, `PROFILE=release-opt` (fat LTO, stripped): the plain
  release build keeps symbols and would hide the delta. The branch builds
  into its own target dir (`GO_TARGET_DIR`) so both binaries stay on
  disk. Expected from [replacing-impeller](../notes/replacing-impeller.md):
  Impeller and its deps were 4.5 MB of symbol bytes in a 41 MB stripped
  Linux binary; on Android the APK loses the 8.7 MB `libimpeller.so`.
  Linux first; the APK number needs the runtime.mk edit.
- **Pixels are diffed against main's binary.** `sol render --settle` is
  deterministic, so the same app renders under main's client and the
  branch's, and the frames are compared. There is no PNG diff tool in the
  tree yet: stage 1 adds a small alloy example over the `image` crate it
  already has as a dev-dependency, printing the count of differing pixels
  and writing a diff image. As in the text plan, the diff is reported, not
  asserted: Impeller is the reference for "nothing moved", not for
  correctness. The contract test (`alloy/src/tests/renderer.rs`) and the
  op log (`renderer/oplog.rs`) run against the new renderer unchanged.
- **The TV is measured from stage 2.** The dense and sparse mover probes
  (`probes/damage-probe.tsx`, `probes/damage-probe-sparse.tsx`), release
  builds only. The text bench the text plan measured with
  (`probes/text-draw-bench.tsx`) is no longer on disk; stage 2 rebuilds it
  from the description in
  [text-rasterizer-findings](../notes/text-rasterizer-findings.md).

The contract's conventions stay as they are, so callers do not change:
logical pixels, y down; framebuffers drawn bottom-up, with the caller
pre-flipping (`raster::offscreen::flip_for_fbo`); `Renderer::draw` clears
the target first; anti-aliasing is the caller's (MSAA storage plus a
resolve) for every edge a quad provides. The edges the fragment stage
decides (an oval's rim, a stroke band's inner edge, a round cap) get
analytic coverage in the shape shader, because under GLES 3.0 MSAA the
fragment stage runs once per pixel and a discard keeps or drops all of
its samples (decided in stage 1, see Findings); coverage AA as a
replacement for MSAA stays out of scope. Two conventions
the contract leaves implicit, pinned here because the first pixels depend
on them:

- A target's logical (0, 0) maps to clip space (-1, +1), the top of the
  window. A sampled texture's texel row 0 is the top of `src`: the glyph
  pass writes its layers that way (`gl/glyphs.rs`), and snapshots are
  pre-flipped for it.
- Textures hold premultiplied color, so the blend is `ONE,
  ONE_MINUS_SRC_ALPHA`, and blending happens in sRGB with no linear step,
  as Impeller's does on an RGBA8 target.

## Stage 1: minimal apps

Goal: an app built from core elements (views with backgrounds and
borders, text, images, scrolling) renders correctly. No radii, no
opacity, no filters, no shadows, no paths. Hello-world itself needs only
translate, save and restore, an axis-aligned intersect clip, and one
texture quad per text; the other core-only examples (drag, recurse, spin,
text-flow, audio) add solid rects, ovals and strokes.

**Recording.** An own `DisplayList`: a plain op enum recorded on the UI
thread into a `Vec`, shared behind an `Arc` (a list is cloned and crosses
threads); a nested list is held by its `Arc`. The cull rect of
`DisplayListBuilder::new` is ignored: the executor's clip rejection covers
it. In a test build the recording types keep the op log beside the real
recording, one `cfg(test)` statement per forwarder, exactly as
`impeller.rs` did: the executable op carries texture handles and
premultiplied colors, the log carries paint fields and path segments, and
folding the two is cleverness the stage does not need.

**Execution.** One executor on the raster thread. Every draw is an
instanced quad through one program: per instance its quad, a 2x3 affine
transform, a shape kind (rect, oval, line, texture), fill or stroke
width, color (premultiplied), a texture rect, and a clip rect. Instances
go up in a VBO with instanced attributes under one VAO the executor owns
(glow 0.17 has the GLES 3.0 instancing calls), rather than the glyph
pass's float-texture trick, so stage 2's batching is a wider buffer write
and not a rewrite. Stage 1 may issue one draw per op, but the instance
layout is the one stage 2 merges.

- Shapes are evaluated in the fragment stage from local coordinates, fill
  and stroke alike. This covers `draw_rect`, `draw_oval` and `draw_line`
  (butt and square caps; round caps if they come free with the line's
  distance function). `draw_rounded_rect` draws the plain rect and warns;
  `clip_rounded_rect` clips to the plain rect and warns. Rounded shapes
  are stage 2: leaving them out drops the per-corner radii from the
  instance and the rounded distance function from the shader, and the
  `<texture>` with radii (`kinds/texture.rs`, an image color source over
  a rounded rect) then stays with the other warned color sources.
- `draw_texture_rect` with linear and nearest sampling. With a paint,
  only its alpha applies (a `<texture>`'s opacity is an alpha per op, not
  a group). Text arrives here as its layer.
- `adopt_texture` wraps a GL name and a size, and the RGBA8-only limit
  goes, so the `unadopted` path in `raster/targets.rs` is never taken.
  The handle owns the name, but it must not delete it on drop: the last
  clone can drop on the UI thread (lists cross threads, textures ride in
  the context-restored event), and deletion needs the raster thread with
  the context current. Impeller's reactor did this quietly. The handle's
  drop pushes the name onto a queue shared with the renderer, the
  executor drains the queue at the start of every `draw`, and the queue
  is discarded when the renderer is recreated after a context loss
  (`raster/recover.rs`), so a stale name is never deleted on a fresh
  context.
- `save`, `restore`, `translate`, `scale`, `transform`. The vertex stage
  takes the full affine, so a rotated draw costs nothing. Perspective
  warns and draws with the affine part.
- `clip_rect` with intersect, as the instance's clip rect, tested in the
  fragment stage, so a clip change never ends a batch. The clip is
  intersected in device space after the transform, not stored as the
  local rect; the overflow clip's "infinite" extent on an unclipped axis
  (`composite.rs`, `CLIP_INF`) is fine in a highp float. An op whose
  bounds fall outside the clip is dropped at execution before any GL
  work. A clip under a transform that is not translate plus scale warns
  and clips to the rotated rect's bounding box.
- `draw_display_list` at opacity 1 executes the nested list in place.
  Below 1 it draws at full opacity and warns: group opacity needs an
  offscreen target, which is stage 6. No shortcut that multiplies the
  opacity into each child, because overlapping children would show
  through.
- Solid color only, blend mode source-over only. Gradients, image
  sources, filters and other blend modes warn. A rect shadow without its
  own layer (`kinds/shadow.rs`) therefore draws hard-edged; one with a
  layer draws flat.
- The program compiles at context creation, like the glyph program.
- GL state: the executor sets every piece of state it uses on each draw
  and restores what `run_pass` and the other alloy passes expect. There
  is no hidden state cache shared with anything else. The rig's
  depth-stencil attachment stays: stage 1 does not use it, stage 5 does.

**What a minimal app may not use yet.** Core sets no opacity on its own.
The components do: a disabled `Item` is 0.5 (`item.tsx`), popups, modals,
selects and dismissibles fade, and checkbox and switch marks fade in
(`motion.tsx`). Under stage 1 these show at full opacity at once. Stage 1
test apps use core elements.

**Order.** Each step leaves the client running.

1. Branch. Delete `impeller.rs` and the `impellers` manifest entries, add
   `anvil.rs` as a complete stub (every method warns once), point the
   aliases at it. The client boots to a cleared window. Take the Linux
   `release-opt` size against main here.
2. State ops, the solid rect, the clip. The recurse and drag examples
   render.
3. Adoption with the deferred deletion queue, the textured instance.
   Hello-world and text-flow render with text.
4. Oval, stroke, line. The remaining core examples render.
5. The PNG diff example, the diff against main's binary over the core
   examples, the findings into this plan.

**Done.** `examples/hello-world` and the core-only examples render under
anvil with a reported diff against main's binary of edge pixels only,
beyond the rounded corners stage 2 brings and the stroke-and-fill strokes
main never painted. The
contract test passes, and each unsupported op warns once. The size of
both `release-opt` Linux clients is recorded in Findings.

## Stage 2: batching, and text straight into the frame

Goal: the per-op cost and the per-draw cost are both gone for minimal
apps, and the core elements draw in full.

**Rounded shapes.** `draw_rounded_rect` and the axis-aligned
`clip_rounded_rect` as a rounded distance function in the shape program,
with per-corner radii on the instance. The `<texture>` with radii follows
as the one image color source: shape coverage times the texture sample,
clamp tiling, one multiply in the fragment stage. First TV read of the
mover probes once these are in.

**Batching.** Adjacent instances that share a program and textures merge
into one instanced draw. Painter's order is kept: only adjacent ops merge.
Reordering across non-overlapping ops is a later option, taken only if a
measurement asks for it.

**Text straight into the frame.** The glyph pass's quads (`gpu::GlyphQuad`)
join the frame's instance stream. They sample the shared text atlas, and
the coverage policy and gradient ramp move into the frame's program. All
text on a screen then batches over one texture, and under the new
renderer the text layers and their retention machinery are not used.
The Impeller build keeps them until stage 7.

The open design question of this stage. A text layer holds pixels, never
atlas cells, so the atlas may grow, repack or evict under it
(`text_atlas.rs`, "a cache, not a store"). A direct draw holds cell
placements, and a cached recording replays them frames later. Two ways
out, to be decided at the start of the stage:

- Record glyph keys, not placements, and resolve them against the atlas
  at execution on the raster thread. The atlas then needs a raster-side
  view, and a miss at execution has no synchronous path to fill it.
- Record placements, and invalidate every recording that references a
  cell the atlas moved or evicted, as cells landing damage text nodes
  today (`RenderTree::note_text_layer`).

The scale policy carries over from the layer to the cells: at a moving
scale the quads stretch cells made at the last rest scale, within
`LAYER_SCALE_MOTION_BOUND`, and the cells are remade hinted at rest.

**Textures.** Images and the remaining texture quads still end a batch at
each texture change. Binding several textures per batch with an index per
instance is an option to measure. GLSL ES 3.00 indexes sampler arrays only
with constant expressions, so it is a switch over the index.

**Done.** The TV's dense mover probe and the text bench run locked at 50
fps under the new renderer. A screen of prose draws in a few batches.

## Stage 3: shaders straight into the frame

Pulled ahead of paths on 2026-11-18: it is the end state's remaining
structural piece for minimal apps (a shader panel costs one render pass
and one texture today), and it decides the shape of the per-draw program
path the mesh program of stage 4 will reuse.

Goal: a generator shader (a fragment program whose output depends on
position, time and uniforms alone) draws as a quad in the frame, under
the frame's transform and clip and into its MSAA target, with no
offscreen texture. Before, the shader drew into an offscreen texture and
that texture was drawn; now the shader draws. Shaders that sample
`uSource` or keep `uPrevious`, multi-pass chains, mesh pipelines,
render-once-sample-many targets, readback and every non-shader texture
stay with shader targets and `<texture>`, unchanged.

**The API (decided 2026-11-18).** A draw primitive beside `<rect>` and
`<texture>`, over a handle from `@solidrt/core/gpu`:

```tsx
let plasma = createPaintShader(glsl`
  uniform float uTime;
  void main() {
    vec2 p = vUV * 2.0 - 1.0;
    fragColor = vec4(vec3(0.5 + 0.5 * sin(p.x * 6.0 + uTime)), 1.0);
  }
`)
<shader src={plasma} params={{ uTime: t() }} textures={{ uNoise: noise }} width={240} height={160} />
```

The fragment contract is `createShaderTexture`'s, so a source written for
a shader texture works unchanged: `vUV` 0..1 over the element's box with
a top-left origin, `iResolution` the box in device pixels, `fragColor`
premultiplied, params and sampler inputs by name (a number drives a
scalar, a flat array the declared type; a texture binding takes an id or
`{ id, filter?, wrap? }`). The handle is source text, not a compiled stage
id: the program cannot reuse `linkProgram`'s programs, whose vertex stage
is the covering triangle, and anvil has to add an epilogue to the
fragment stage, which a compiled stage object does not allow.
`destroyPaintShader` frees it; the `create*` owner-scoped auto-free
applies. `<shader>` takes the layout props, `params`, `textures`, and
`radius` (its own rounded corners through the instance's rounded-clip
slot while no rounded clip is in effect; under one it warns once and
draws square).

**In anvil.** `Renderer::create_paint_shader(fragment_src)` compiles and
links on the raster thread: anvil's vertex stage (`VERTEX_COMMON` plus a
`vUV` output from the quad's corner) and a fragment stage made of anvil's
preamble (`vUV`, `iResolution`, `fragColor`, the clip inputs), the app's
source with its `main` renamed by macro, and an epilogue that calls it and
multiplies `fragColor` by the clip factor (the box and the rounded clip,
as the glyph program applies them). Uniforms are reflected at link time
into a name to (location, kind, count) table the handle keeps, and the
reflected table is returned so alloy validates params on the UI thread as
it does for `linkProgram`. The handle is `Arc`-shared and `Send + Sync`;
its drop pushes the GL program onto a deletion queue beside the texture
one, drained at the start of every draw, discarded with the renderer on a
context loss. `DisplayListBuilder::draw_shader(rect, shader, params,
textures)` records one op whose params and inputs are shared slices
(the element keeps them and re-records by reference). The walk emits one
instance (the rect, the map, the clips, `tex` the unit square) in a batch
of its own with a `Paint` program kind; the executor uses the program,
sets `iResolution` from the quad's device size and the params by the
table, binds the inputs on units 0.. through sampler objects per (filter,
wrap) it creates as it meets them, and draws. A direct draw ends the
batch before it and the one after it.

**In alloy.** `Context::create_paint_shader` and `destroy_paint_shader`
over a blocking RPC (`RasterCmd::CreatePaintShader`, `DestroyPaintShader`),
a UI-side registry of handles like the texture registry, the source kept
on the raster side so recovery relinks and the context-restored event
carries the new handles, a `Shader` element kind (`kinds/shader.rs`) that
records `draw_shader` from the registry's handle and the texture
registry's handles for its inputs (a missing id warns and the input is
skipped, as a node shader's does), and a row in the resource inventory.
Params and texture writes are paint damage on the node.

**In flux and core.** The `shader` tag and its props in `alloy_plugins`
and `types.d.ts`; `createPaintShader`, `destroyPaintShader` in `gpu.ts`;
`packages/core/examples/paint-shader.tsx`; `docs/shaders.md` gains a
"Paint" section stating which programs are a `<shader>` and which stay a
shader texture, `docs/drawing.md` the element.

**Measure.** A probe with a grid of shader panels, once through
`createShaderTexture` plus `<texture>` and once through `<shader>`, on
the desktop and the TV: GPU time per frame, draw calls, the resource
inventory. Into Findings.

**Done.** The example renders; a GL example diffs a direct draw against
the same source through a shader texture at the same size (identical but
for the 1/255 rounding class); the paint shader draws under a rect clip,
a rounded clip and a rotation; params and inputs validate as
`linkProgram`'s do; a destroyed handle held by a retained recording keeps
drawing until the recording is dropped and the program is deleted after;
context recovery relinks; the probe's numbers are in Findings.

## Stage 4: paths, lines and gradients

Started 2026-11-19, after stage 3 closed.

Goal: every `draw_path` the rendertree records draws (fills under both
fill rules, strokes with their caps, joins and miter limit, the dashed
paths the walker makes), and a paint's linear or radial gradient paints
every shape it is set on, as Impeller drew them. A path's mesh is
tessellated once and redrawn from a cache while its geometry, its stroke
and its scale hold. Lines keep their quad (`draw_line`); polylines and
dashes go through `draw_path` as they do today. The recording API the
rendertree calls does not change; the rendertree stops building a lyon
path of its own beside the renderer's.

**Gradients (step 1, anvil and the text resolver).**

- One gradient type in anvil, `RampGradient`: stage 2's `GlyphGradient`
  renamed, since it serves every draw now. It holds the kind, the tile
  mode, the ramp (`RAMP_WIDTH` straight-alpha sRGB texels) and two affine
  rows mapping the op's space to the gradient's parameter space. anvil
  owns the ramp sampler (`RampGradient::ramp(colors, stops)`: the stops
  interpolated in straight-alpha sRGB, the first color before the first
  stop and the last after the last, as the text resolver does today) and
  the small affine toolkit (compose, invert, the 2D part of a `Matrix`)
  that `rendertree/text/gradient.rs` carries; the text resolver keeps its
  box-units and layer-scale logic and calls anvil for both.
- Record time: `set_color_source` with a gradient no longer warns.
  `draw_rect`, `draw_rounded_rect`, `draw_oval` and `draw_line` resolve
  the paint's gradient to a `RampGradient` (`gradient_of`, beside
  `image_of`): the inverse of the source's transform composed with the
  linear projection (`t = ((q - start) . d) / |d|^2`) or the radial
  normalization (translate by minus the center, scale by one over the
  radius), exactly the text resolver's arithmetic. A degenerate source (a
  zero-length axis, a non-positive radius, a singular transform, no stops)
  warns once and the shape draws the paint's color. The shape ops carry
  `source: Option<Source>`, `Source::Image` or
  `Source::Gradient(Arc<RampGradient>)`; a line takes a gradient and keeps
  its image warning.
- Evaluation is in device space. A batch carries
  `GradientDraw { gradient: Arc<RampGradient>, rows: Affine }`, the rows
  mapping a device pixel to the parameter space: the op's rows composed
  with the inverse of the instance's device map, computed in the walk per
  op (two 2x3 products; the ramp is shared by `Arc`, never copied). Every
  program then reads a position it already has, `vDevice`, and none grows
  a varying for it: the glyph program drops `vPos` and evaluates its
  gradient the same way, a line's gradient is right although its quad
  lives in the segment's frame, and a gradient under a rotation or a
  nested transform is right everywhere. A singular device map draws the
  op's solid color.
- Programs: one GLSL snippet, `GRADIENT_COMMON` (the uniforms `uRamp`,
  `uGradient`, `uGradientX`, `uGradientY`, `uTile`, and
  `vec4 gradientAt(vec2 device)`: the straight-alpha ramp color, the decal
  mode's outside as alpha 0), used by the glyph program and by a gradient
  variant of the fill and of the shape program (`fill_gradient`,
  `shape_gradient`: the same sources with `#define GRADIENT` ahead; the
  color is then the premultiplied ramp sample times `vColor`, which the
  walk sets to the paint's alpha in all four channels, as it does for an
  image). Five programs link at `new`. A gradient draw is a batch of its
  own, sealed, as a glyph gradient run is: it rewrites the one ramp
  texture and sets the rows. `BatchProgram` keeps its classes; the
  executor picks the variant from `Batch.gradient`.
- A plain rect with a gradient (the common gradient: a background, a
  sheen face) draws through `fill_gradient`: the fill program's rule, in
  a sealed batch.

**Paths (step 2, anvil).**

- `anvil::Path` is an `Arc` over a `lyon_path::Path`, its fill rule, its
  control-point bounds and a mesh cache. `PathBuilder` wraps lyon's
  builder and tolerates the contract's walks (a segment before any
  `move_to` begins at the current point; a `close` with nothing open is
  nothing). `Path::geometry()` hands the lyon path out for the
  rendertree's bounds, hit test and dash walk: an inherent method on the
  concrete type, not a contract method. New pins in `anvil/Cargo.toml`:
  `lyon_path = "=1.0.19"` (alloy's pin) and `lyon_tessellation =
  "=1.0.23"` (the release that requires lyon_path 1.0.19). The fill rule,
  the caps, the joins and the miter limit are lyon's.
- Tessellation (`renderer/mesh.rs`, pure, tested without GL): a `Mesh` is
  positions in the path's space, `u32` indices and the vertex bounds. A
  fill tessellates with lyon's `FillTessellator` under the path's rule, a
  stroke with the `StrokeTessellator` under the paint's width, cap, join
  and miter limit, both at a tolerance of `MESH_TOLERANCE` device pixels
  (0.25, the dash walker's figure; a tuning value) divided by the scale.
  The scale is the larger axis stretch of the instance's device map,
  rounded up to a power of two (the scale bucket), so a smooth zoom
  retessellates only when it crosses a power of two and a mesh is never
  coarser than the tolerance. A tessellation error logs once and draws
  nothing for that op.
- The cache is on the path: up to `MESH_CACHE_ENTRIES` (4) meshes, keyed
  on the style (fill, or stroke with its width, cap, join and miter) and
  the scale bucket, the oldest replaced. The element's `Path` and every
  recording's clone share the one `Arc`, so a retained list redraws from
  the cache and an element whose stroke animates retessellates only that
  key. Tessellation runs in the walk, on the raster thread, where the
  scale is known; its count is reported.
- A `DrawPath` op carries the path, the style (fill, or stroke with its
  metrics), the premultiplied color and the source (image or gradient),
  one op per style: a stroke-and-fill records a fill op and a stroke op,
  so the stroke covers the fill as Skia's does. Before tessellating, the
  walk rejects an op whose control-point bounds, outset by the stroke's
  reach (half the width times the larger of the miter limit and sqrt 2),
  miss the clip; after, by the mesh's own bounds.
- Meshes draw through a mesh program and batch like quads. The walk
  transforms a mesh's vertices into device space on the CPU (cheap next
  to the tessellation it skipped) and appends them to a mesh vertex
  buffer, `MeshVertex { device: [f32; 2], color: [f32; 4] premultiplied,
  clip: [f32; 4] }`, with the indices rebased; a mesh batch is a range of
  indices, drawn by `glDrawElements` from a second VAO over the mesh
  buffers, uploaded once per draw beside the instances. Consecutive path
  ops share a batch while their texture, their gradient (none, or one
  sealed batch) and their rounded clip agree. The rounded clip is a
  per-batch uniform pair (`uRClip`, `uRClipRadii`), the one state that is
  not per vertex, because a path under a rounded clip is rare and the
  alternative is eight more floats a vertex. Per-vertex color and clip,
  40 bytes a vertex, are the deliberate minimum that batches across paths
  with different colors and clips; a slot table or retained GPU meshes
  are the next step if the TV read says the upload is the cost.
- The mesh program's fragment stage is the fill program's: the clip box
  as a factor, no discard, no derivative, `roundedClip()` from the
  uniforms with its early return. Variants as the quads': `mesh`,
  `mesh_gradient` (`gradientAt(vDevice)`), `mesh_textured` (an image
  source: the uv from `vDevice` through a per-batch device-to-texel map,
  composed at walk time from the source's map and the instance's inverse,
  so no vertex carries a uv). Eight programs in all; each stays minimal.
- `DrawReport` gains `meshes` (path ops drawn) and `tessellated` (cache
  misses this draw); alloy sums them into the raster stats beside
  instances and draw calls, so `/stats` shows whether the cache holds.
- Batches interleave in painter's order: the executor binds the instance
  VAO or the mesh VAO per batch. The scissor is the union of the quad
  clips and the mesh clips.

**In alloy (step 3).** `kinds/path.rs` drops its own lyon builder and
reads the geometry from the drawn path (`Path::geometry()`): bounds,
stroke shape, dash pieces and the hit test over the one path. The raster
stats carry the two new counts. `alloy/examples/path_mesh.rs` on the real
GL path: a nonzero and an even-odd star, a stroke's miter tip against a
bevel, round caps, a dashed curve, a gradient rect against CPU-evaluated
expected colors, a gradient and an image-filled path, each under a rect
clip, a rounded clip and a rotation; and a second draw of the same list
reports no tessellation while a doubled scale reports one.

**Measure.** The branch's own output before and after step 1 for the
gradient text example (a refactor: zero, or the 1/255 class), and the
path examples (`line-points`, `parse-svg`, `on-layout-connect`, the
gallery's icons) against main's reference client, reported. A probe,
`probes/path-bench.tsx`: a grid of stroked polylines (sparklines of N
points; `route=live` rewrites the points every frame, `route=static`
retains them) and gradient panels; the frame rate, the GPU time, the draw
calls, the mesh and tessellation counts and the raster thread's CPU on
the desktop and the TV. Into Findings.

**Done.** Every `draw_path` the rendertree records draws: fills under
both rules, strokes with caps, joins and the miter limit, dashes;
gradients on rects, rounded rects, ovals, lines, paths and text through
one ramp path; the example's checks hold; a retained list's redraw
tessellates nothing; the walk tests cover the batch rules, the rows
composition, the cache key and the scale bucket; the probe's numbers are
in Findings.

## Stage 5: full transforms and clips

- Clips under rotation and perspective, `clip_rounded_rect`, `clip_oval`,
  and the difference op. An axis-aligned rounded clip stays per instance
  (a distance test in the fragment stage). The general case uses the
  stencil buffer.
- Perspective for draws.
- The dash, stroke and gradient cases under these transforms that the
  parity tail lists in the note.

## Stage 6: layers and effects

- An offscreen target pool, sized and reused by bucket, for `save_layer`.
- `save_layer` with group opacity and color filters, and
  `draw_display_list` below opacity 1. The components' fades work from
  here.
- Gaussian blur as an image filter, the mask blur behind shadows,
  `save_backdrop_layer`, the color matrix.
- Shadows of rects and rounded rects drawn analytically in the shape
  program where the shadow does not need its own layer (`kinds/shadow.rs`,
  `needs_own_layer`).
- Blend modes beyond source-over: first the ones the apps and examples
  emit; the rest of the contract's 29 after.

## Stage 7: parity, platforms, merge

- The pixel diff over every example and the `sol test` suite, with the
  differences explained one by one.
- Damage and partial repaint on the rig path and buffer age, re-verified
  against the new renderer's output.
- Android, Windows and macOS, and the TV again.
- The branch merges. Impeller left at stage 1; what goes here is the
  interim machinery it left behind: the text layers (`TextLayer` and its
  retention in `rendertree/text/mod.rs`) and the adoption bookkeeping
  (`TextureEntry::handle`'s `Option`, `unadopted` in `raster/targets.rs`).
  The glyph vocabulary and the old GL glyph pass were dealt with early,
  after stage 2 (the vocabulary lives in the `anvil` crate, the pass is
  gone; see Findings, "Where anvil lives").
  The Impeller mentions in `alloy/CLAUDE.md`, the root CLAUDE.md and the
  `impeller-*` upstream notes are updated or closed.

## Not in scope

- Vulkan or Metal backends. GLES stays the one backend
  ([graphics-backend-strategy](../notes/graphics-backend-strategy.md)).
- Coverage AA as a replacement for MSAA.
- Changes to the recording API the rendertree calls, beyond the stage 3
  shader declaration.

## Findings

### Binary size without Impeller (2026-11-18, stage 1 step 1)

Linux x86_64, `make client`, both from the same tree: main at `4922b6f4`,
the branch at its first commit (the anvil stub, which draws nothing, so
the delta is Impeller alone; the finished renderer adds a few hundred
kilobytes at most).

| Profile | main | own-renderer | delta |
|---|---|---|---|
| `release-opt` (fat LTO, stripped) | 69,074,264 | 65,106,360 | -3,967,904 (-5.7%) |
| `release` (symbols kept) | 99,947,992 | 94,419,224 | -5,528,768 (-5.5%) |

The stripped delta is 4.0 MB, a little under the 4.5 MB of symbol bytes
[replacing-impeller](../notes/replacing-impeller.md) counted: LTO had
already dropped part of Impeller's dependencies.

Android, armeabi-v7a (the TV's ABI), the branch's first Android build on
2026-11-18, a debug player APK (`make android-client ANDROID_ABI=armeabi-v7a`):
48,531,181 bytes, carrying `libmain.so` (38,537,708), `libSDL3.so` and
`libc++_shared.so` and no `libimpeller.so`; main's 32-bit APK carried a
4,987,340 byte `libimpeller.so` beside them. One trap: Gradle packages
every library under `dist/android/<abi>/`, so a `libimpeller.so` staged
there by an earlier main build rides into the branch's APK until it is
deleted (the first build here did exactly that).

### Stage 1 diff against main (2026-11-18)

The six core-only examples were rendered twice from the same staged
bundle at 1280x720: by `sol render` under the anvil client, and by main's
`release` client at `4922b6f4` with the arguments `sol render` passes.
The resting examples (hello-world, text-flow, drag) ran with `--settle
--duration 0.1 --fps 10`, one frame; the animated ones (recurse, spin,
audio) with `--duration 0.5 --fps 10`, five frames. The frames were
compared with `alloy/examples/png_diff.rs`, the diff tool stage 1 added,
at tolerance 0. The diff is reported, not asserted: Impeller is the
reference for "nothing moved", not for correctness.

| Example | Frames | Differing pixels (frame 0) | Of | Max channel difference | Explanation |
|---|---|---|---|---|---|
| hello-world | 1 | 0 | 921600 | 0 | identical |
| text-flow | 1 | 657 | 921600 | 18 | the circle's rim (644), the 1/255 rounding class (13) |
| drag | 1 | 3438 | 921600 | 204 | rounded corners drawn square, the card outlines' edges, a few quad edge pixels |
| recurse | 5 | 114640 (114028 to 114640) | 921600 | 255 | rounded corners drawn square, a few quad edge pixels |
| spin | 5 | 4244 (4244 on every frame) | 921600 | 102 | the stroke-and-fill outline main never painted, rounded corners drawn square |
| audio | 5 | 106 (106 to 110) | 921600 | 22 (22 to 61) | the dot's rim |

**Rounded corners drawn square** (drag, recurse, spin; stage 2). The
diff images mark the corner fan of every rounded rect: drag's surface
(radius 16) and cards (radius 10, fill and outline), recurse's 25 nested
squares (radius a quarter of the side) and its two pause pills, spin's
four buttons (radius 20). In drag, a tolerance of 16 leaves 1204
differing pixels, all of them in the corners. In recurse the nested
squares shade to red in steps of about ten levels, so a corner anvil
fills with a square's color differs from main's (the parent's color) by
one step; a tolerance of 16 leaves 30958 pixels, the corners of the
outer squares. These carry the large maxima (204, 255, 163).

**Edges decided in the fragment stage, and quad edges under MSAA**
(text-flow, audio, drag, recurse). Anvil gives the oval rim and a stroke
band's inner edge analytic coverage; Impeller tessellates and resolves
4x MSAA. The ramps differ by a few levels: text-flow's circle by at most
18 (at (412, 220): anvil `#E2C897`, main `#DFC189`), audio's dot by 22 on
frame 0. Audio's frame 4 reaches 61 in four pixels of the dot's leftmost
column, at the tangent point where the analytic edge meets the quad
edge (anvil `#0B6B97`, main `#0596D4`): the double attenuation stage 1
accepted. Drag's 1 px card outlines on the rotated surface differ along
their whole length by at most 16 (`#0586BEE8` against `#067EB1E9`).
Quad edges, MSAA under both, differ only in single pixels by one sample
of four: recurse's straight edges by 2 to 3 levels (`#110000` against
`#140000`), drag's translucent surface edge by an alpha of 16 (0 against
`0x10`, a quarter of its 0.25 alpha).

**Text** (every example). The text layers draw identically: hello-world
differs in no pixel, and the labels of drag, spin and audio and the
lines and the per-glyph headline of text-flow differ in none, except 13
pixels of text-flow's final "l" (x 591, y 464 to 475, `#827D75` against
`#817C74`). Those are the 1/255 rounding class: a bilinear blend of two
texels whose highp result rounds to the other unorm8 level under anvil.
The translucent cards of drag match main to the unit inside (`#029CDCE3`
under both), so premultiplied color blended in sRGB is the same
convention.

**Spin's stroke-and-fill.** Spin's buttons are `drawStyle="stroke-and-fill"`,
`strokeWidth={2}`, in the translucent `rgba(60,70,90,0.4)`. Main
(Impeller) paints only the fill: apps that used the style saw a fill.
Anvil follows the contract and paints the stroke too, so each button
grows by a pixel all round (the 4244 pixels above, the outline's outer
band at alpha `0x66` where main has `0x00`, plus the corners). The first
cut pushed the stroke as a second instance on top of the fill, and the
stroke's inner half blended a translucent color a second time (alpha
`0xA3` where main had `0x66`); a stroke-and-fill now records one fill of
the shape outset by half the stroke width (`shape_ops` in
`alloy/src/renderer/anvil/record.rs`), the union of the fill and the
stroke covered once, as Skia covers it. A rounded rect's radii join the
outset in stage 2. Spin draws no other difference: its ring is ovals that
pointer input adds, empty in a render, so its five frames are identical
under both binaries.

The unsupported ops each example still hits, one warn line each:

- hello-world, text-flow, audio: none.
- drag, recurse, spin: `[alloy] Renderer: draw_rounded_rect is not
  supported yet; the rect draws with square corners`.

The animated renders are deterministic across the two binaries: recurse
and audio move between frames (recurse's frame 0 and frame 4 differ in
167232 pixels under main), and every frame pair differs only in the
classes above, with the moving bounding boxes in step.

ImageMagick was not enough for this: `magick compare -metric AE` on
ImageMagick 7.1 here prints a weighted fraction, not a pixel count, which
is what made an own tool worth writing. To rerun a diff:
`cargo run -p alloy --example png_diff -- <a> <b> [diff] [--tolerance N]`,
where `a` and `b` are two PNGs or two `sol render` output directories
(frames paired by name), and `diff` a diff image or a directory of them.

### What stage 1 taught

- Fragment-decided edges get no MSAA. Under GLES 3.0 the fragment stage
  runs once per pixel and a discard keeps or drops all samples together,
  so an oval rim, a stroke band's inner edge and a round cap came out
  stair-stepped while the quad edges were smooth. Those edges now get
  analytic coverage, a one-pixel ramp over the signed distance divided
  by `fwidth`, so scaled and anisotropic transforms stay right
  (`alloy/src/renderer/anvil/exec.rs`); MSAA keeps the quad edges. Where
  an analytic edge meets a quad edge (an oval's four tangent pixels, a
  round cap's two junction pixels) the coverage is attenuated twice; the
  diff shows it as audio's 61.
- The per-fragment clip discard is hard-edged under MSAA, where shape
  edges are antialiased. Stage 5's general clips must decide whether it
  stays.
- The instance layout held at 24 floats (six vec4 attributes, 96 bytes,
  `INSTANCE_FLOATS` in `alloy/src/renderer/anvil/walk.rs`) to the end of
  stage 1. A stroke expands the quad by half its width at walk time and
  the fragment stage recovers the inner shape from the width; a line's
  cap and length ride in the texture slot, which shapes do not use.
  Stage 2's per-corner radii need a seventh attribute or the unused
  texture rect of a rect.
- Rect strokes use the Chebyshev inner test (sharp miter corners), not
  the Euclidean offset of the distance, which would round the outer
  corners. Ovals use the gradient-normalized ellipse distance, exact to
  first order and a few percent off for thick strokes on eccentric
  ovals. A stroke width of zero or less draws nothing (the rendertree
  never asks for a hairline); sub-pixel widths draw thin.
- Batches split only at a texture or sampling change; a shape instance
  joins any run. A screen of rects and text layers is one draw per
  text-layer change, which stage 2's text into the frame removes. Anvil
  owns two sampler objects (linear and nearest, clamped, no mipmaps) and
  binds them per batch, so texture-object state stays untouched.
- A nested list's restore pops only what that list saved, and the
  parent's state comes back after the nested walk regardless: stricter
  than Impeller, and an unbalanced nested recording cannot leak state.
- The initial device clip is the target rect, so every instance's clip
  is finite and offscreen draws are rejected before GL; the rendertree's
  "infinite" clip extents of 1e7 intersect down to the target.
- Each draw disables `RASTERIZER_DISCARD` and `SAMPLE_COVERAGE` as well
  as the usual states: both silently kill draws when a shared context
  leaves them on.
- Colors are premultiplied at record time and blended in sRGB, and the
  text layers are sampled upright under anvil's window mapping with no
  flip: drag's translucent cards match main to the unit and hello-world
  differs in no pixel. `raster::offscreen::flip_for_fbo` and its callers
  stayed unchanged.
- The 1/255 rounding class: anvil writes 0.5 from a highp float and the
  driver's unorm8 conversion rounds the 127.5 tie down to 127, where
  Impeller produced 128. It shows in 13 pixels of text-flow and fails
  the strict precheck of `alloy/examples/texture_paint.rs` (a white rect
  at alpha 0.5); that example's other cases pass under anvil except the
  plus blend mode (stage 6). Stage 7's parity pass decides its
  tolerance.
- Main never painted the stroke of a stroke-and-fill draw, so apps that
  used the style saw a fill. Anvil paints it, as one fill outset by half
  the stroke width, so a translucent color covers the union once.
- `half` is a reserved word in GLSL ES 3.00.
- The warn-once macro (`unsupported!` in
  `alloy/src/renderer/anvil/mod.rs`) guards with a relaxed load before
  the swap, so an op that stays partial costs a plain read per record.
  Hello-world records only seven distinct ops.
- `packages/cli/runtime/runtime.mk` keeps the armv7
  `BINDGEN_EXTRA_CLANG_ARGS` variables: rquickjs-sys runs bindgen for any
  non-host target, so they were never Impeller's alone.
- `alloy/examples/pixel_snap.rs` passes its coverage assertions under
  anvil; its snapshot re-make count at 1.5x fails identically on main, a
  stale expectation of the example.

### Stage 2: rounded shapes (2026-11-18)

A rounded rect is its own kind in the shape program (`KIND_ROUNDED_RECT`
in `alloy/src/renderer/anvil/walk.rs`): fill and stroke band are both
analytic over the per-quadrant rounded-box distance, and the plain rect
keeps its exact on-grid path, which `alloy/examples/pixel_snap.rs`
checks. The quad is padded one local pixel past the edge
(`ROUNDED_QUAD_PAD`) so a straight run is not attenuated by MSAA and the
ramp both. Radii are circular, in CSS order, clamped at record time as
CSS clamps them (the rendertree does not clamp for drawing); an
elliptical corner warns once and draws with its x radius; a
stroke-and-fill grows its non-zero radii by the outset. The axis-aligned
`clip_rounded_rect` rides on the instance in device space, one deep (the
device rect and its device radii, eight floats); a nested, a rotated or a
non-uniformly scaled rounded clip degrades to the box with one warning
each, and a mirrored transform swaps the corner radii. Any rect, rounded
rect or oval whose paint has an image color source draws its coverage
times the texel (the `KIND_TEXTURED` flag, the uv from the inverse image
transform at the padded quad's corners; scale-plus-translate maps and
clamp tiling only, the rest warns and draws the solid color); the plain
texture quad is the same path with a rect. The instance is 36 floats in
nine vec4 attributes.

Two edge-quality decisions followed the first renders. The coverage ramp
divided the distance by `fwidth`, an L1 norm, so a diagonal edge ramped
over about 1.4 device pixels and read softer than the quad edges' 4x
MSAA; the gradient's length makes every ramp one pixel wide, and
axis-aligned edges are unchanged. A rounded shape under a rounded clip of
the same radius (the overflow-hidden card) multiplied two ramps on the
shared edge and showed a light rim, 0.6 where main has 1.0; the clip now
composes as the minimum, which is the per-sample AND a stencil clip
gives on a shared edge (0.87 at that pixel, the single ramp against four
samples remains) and errs toward opaque by at most one pixel where two
independent edges cross. Impeller clipped rounded rects through the
stencil under MSAA; anvil's per-instance clip is hard-edged only where it
is a plain box.

| example | after stage 1 | rounded shapes | the two edge fixes | class |
|---|---|---|---|---|
| recurse | 114640 | 17294 (1452 at tolerance 16) | 15065 | rim ramps of 25 nested rounded squares |
| drag | 3438 | 7034 (1177 at tolerance 16) | 5808 | the rotated rounded outlines' rims |
| spin | 4244 | 3180 | 3180 | the stroke-and-fill outline main never painted |
| hello-world | 0 | 0 | 0 | identical |
| text-flow | 657 | 657 | 657 | unchanged |
| audio | 106 | 106 | 106 | unchanged |

Drag's raw count rose because the straight runs of its rotated rounded
rects moved from MSAA to the ramp; at tolerance 16 it fell. Known
limits: under a shrinking transform (a scale below 0.5) the pad covers
less than the ramp's outer half pixel and a sliver of double attenuation
returns (the pad should scale by the inverse axis scale); the rounded-box
distance is continuous only while each radius is at most half the box,
and CSS clamping admits an asymmetric pair (one corner at the full side,
its neighbour at one pixel) that would show a seam on the axis; no render
hits it.

### Stage 2: text straight into the frame (2026-11-18)

**The decision.** Recordings hold atlas cell placements, not glyph keys.
The text atlas takes a new GL texture name on every repack (an eviction
repack now goes through the same replace-at-id path a growth always
took), so a retained recording keeps drawing from the texture it
recorded against until it re-records, and the old name is freed by the
deletion queue when its last handle drops. A move (a growth, an
eviction, a font invalidation) bumps the atlas generation, which damages
the text nodes and requests a frame. Keys were rejected because they
need a raster-side cell table, a hash lookup per glyph per frame on the
TV's weak thread, and a miss path that breaks the complete-frame rule.

**The contract** gained one method, `draw_glyphs`: a run of quads from a
mask atlas under a coverage policy, with an optional gradient, the quads
and the gradient passed as shared `Arc`s so nothing is copied per frame.
The glyph vocabulary stays in `alloy/src/gpu/glyphs.rs` until stage 7
moves it. A `<text>` records its runs under a save, a translate to the
layer origin and a scale by the inverse raster density, so at rest a
glyph samples the atlas texels the layer sampled, once; the scale policy
(the raster density, the stretch while a scale moves, the hinted remake
at rest) is unchanged. The glyph pass and `Text::rasterize` stay for the
stats overlay's own text.

**In anvil** a glyph instance is `KIND_GLYPH` (slot 3; the rounded rect
is 4): the atlas's red channel as coverage, straight-alpha color
premultiplied in the fragment stage, the glyph pass's five coverage
modes and the gradient ramp (four tile modes) ported, the ramp texture
on a second unit rewritten per gradient batch. Batches key on the atlas,
the policy and the gradient; a gradient run is its own batch.
`alloy/examples/glyph_run.rs` draws a word through it and matches the
old glyph pass within 3 levels per channel in every mode. A glyph's
alpha multiplies the rounded clip's coverage (a texel under a stencil
clip scales by the sample fraction), where a shape composes with the
minimum.

**Renders.** hello-world is identical to main on all 60 frames.
text-flow's 13 rounding pixels became 7 (one fewer 8-bit rounding: no
rgba8 layer stands between the atlas and the frame). drag's rotated
labels differ by up to 50 levels because the coverage policy now applies
to bilinearly sampled atlas coverage per device pixel instead of to layer
pixels that were then resampled: one resampling fewer, the intended end
state. Text ink past the layer box is no longer clipped (the layer
clipped at its box plus one line of slack; CSS overflow is visible; no
example relied on it). Under atlas churn (a scratch app stepping its
font size every frame for 120 frames) no stale cell was ever drawn; the
atlas grew from 1024 square to 16384 by 8192 R8 under the device cap of
32768 (the pre-existing growth policy; eviction never happens on the
desktop), and for one frame the old texture lived beside the new one. A
static label differed by at most 2 levels after a growth past 8192 wide
(cell re-make or phase noise). An animating app's frames vary run to run
by up to 2 levels because the glyph worker lands cells at frame
boundaries on its own thread; `sol render --settle` is deterministic
because rest implies every cell has landed.

**The per-text run cache.** Without the layer's currency check every
build re-ran the per-glyph quad work; on `probes/text-prose-bench.tsx`
(40 lines, no repaint boundary, every frame rebuilt) the UI thread's
`paintMs` read 0.74 ms under anvil against 0.05 under main. A text now
keeps its recorded runs with the atlas generation and, while the layer's
old currency key and the generation hold, re-records them by reference
with no per-glyph work; a moving scale reuses the kept runs under the
composite transform; the run styles and line runs are built only when
the record is remade. The 120-frame raster release sweep is gone: the
record owns no texture, and a released text inside a standing recording
would have pinned an old atlas, since a later move never damaged it. An
eviction counts as a move (the packer repacks after evicting even when
it still answers full), so a cached text that no longer touches its
cells is refreshed with everything else and settles within two frames.

| prose bench, desktop, 60 fps | paintMs | UI p50 | p95 | cpu % |
|---|---|---|---|---|
| anvil, before the cache | 0.74 | 0.81 | 0.93 | 7.6 |
| anvil, with the cache | 0.09 | 0.13 | 0.17 | 3.4 |
| main | 0.05 | 0.12 | 0.14 | 4.6 |

Pixel output did not change with the cache (hello-world 0, text-flow
587, drag 7411, spin 3060 differing pixels against main).
`paintOps.draws` reads 38 for the bench under all three; `textLayers`
reads 0 under both renderers and tells nothing now (stage 7 retires or
renames it); `/gpu` lists resources, not per-frame draw calls, which is
why the measurement added its own counters.

**The glyph fill cost.** On the TV (Mali-T860, 1920x1080, scale 2) with
the window patch off, so every frame is a full render, the prose bench's
1646 glyph instances in one draw call cost 53.8 ms of GPU per frame
(186 frames per 10 s, 313 missed) where the dense mover's 801 rects cost
9.1 ms and Impeller's 27 text-layer quads cost 6.2 ms. On-device
experiments settled the cause: a dedicated minimal glyph program (five
attributes, two varyings, the atlas sample times the alpha,
premultiply, no discard, no branches) draws the same frame in 7.4 ms,
identical at mediump and highp; the same plus the full DirectWrite
coverage math in 12.0 ms. About 0.85 million glyph fragments a frame:
some 53 ns each through the one shape program, fifty times under the
fill rate; under 2 ns through the dedicated one; the DirectWrite math
adds about 5 ns. The cause is the shape program as the Midgard compiler
sees it, nine flat vec4 and four smooth varyings read per fragment, the
kind branches, a discard in both paths and the derivatives in
`footprint()`, which set the fragment thread occupancy; the glyphs are
the one draw with the area to show it. Glyph batches therefore draw
through their own program, which clips by multiplication and never
discards, with the coverage modes and the gradient ramp as one GLSL text
both programs share; a glyph run no longer shares a batch with shapes.
After it the prose bench's full frame costs 31.8 ms on the TV (the text
itself 7.6 ms, locked at 50 on its own; the remaining 24 ms is the
bench's full-window background and chrome through the shape program,
whose per-fragment cost on this GPU is the next thing to trim: a plain
rect fill pays for discards and derivatives it never uses), and the
desktop prose screen draws in two to three draw calls, text and the
shapes between it alternating.

### Stage 2: the measurement (2026-11-18)

**Counters.** `Renderer::draw` returns a `DrawReport` (instances written,
instanced draw calls issued); the raster thread sums the reports into
two cumulative counters beside the pass counters, and `/stats` reports
`rendererInstances` and `rendererDrawCalls` at the top level and
`rendererInstancesPerFrame` and `rendererDrawCallsPerFrame` in the
window block; the overlay shows an INST/DC line. Under the window's
partial repaint a frame's figure counts the patch the frame drew (a
mover is two quads in one call); the whole screen is read as the
cumulative delta around a root `/snapshot`, or on a multisampled window
such as the TV's with `SOLIDRT_WINDOW_PATCH=off`.

**Desktop** (release clients, 60 Hz, 1280x720, scale 1; main is the
reference client of `4922b6f4`; three reads of 10 s each, the mover
probes at clock scale 6, muted).

| probe | client | frames / 10 s | missed | paintMs | GPU ms / frame | inst / frame | dc / frame | cpu % |
|---|---|---|---|---|---|---|---|---|
| dense, 800 rects | anvil | 598 | 0 | 0.01 | 0.6 to 2.1 (low), 14.5 to 15.0 (high) | 546 | 1.0 | 3.8 |
| dense | main | 600 | 0 | 0.01 | 0.5 to 1.6 (low), 12.6 to 15.3 (high) | - | - | 16.5 |
| sparse, 50 rects | anvil | 599 | 0 | 0.01 | 0.6 to 2.6 (low), 5.2 to 5.5 (high) | 30 | 1.0 | 3.1 |
| sparse | main | 599 | 0 | 0.01 | 0.9 to 1.1 (low), 14.8 to 15.7 (high) | - | - | 4.1 |
| prose bench | anvil | 600 | 0 | 0.04 | 15.3 | 2.0 | 1.0 | 3.4 |
| prose bench | main | 599 | 0 | 0.05 | 15.2 | - | - | 4.8 |
| grid of 36 textures | anvil | 599 | 0 | 0.03 | 1.0 to 1.1 | 29.8 | 28.8 | 3.4 |
| grid | main | 600 | 0 | 0.04 | 1.0 to 1.1 | - | - | 4.4 |

The desktop's GPU timer falls into two groups for both clients: a high
group near the 60 Hz period, where the timer counts a wait for a free
window buffer rather than draw work, and a low group that is the work;
the prose bench reads 15.3 under both because its `onFrame` demands
every refresh. The mover rows measure a patch of about 1100 by 300
pixels, not the 20 px mover: the partial-repaint route builds the patch
as the union of this frame's damage with the ring of the last age-1
frames' damage, and on these probes the union reaches from the mover's
older left edge to the window's right edge and from the top of the
window to the mover's bottom, under both renderers (the stats' second
`damagePx` reads about 494,000 under main too). The 546 and 30 instances
per frame are the grid rects inside it; a regression test pins that the
walk itself rejects everything outside a patch clip. Why the union
reaches the top-right corner is an open partial-repaint question
(`alloy/src/raster/repaint.rs`, the route's fold of the ring; a fold of
an empty rect with a non-zero origin is the suspect), not an anvil one.

Whole screen, as the cumulative delta around a root `/snapshot`: the
dense probe is 801 quads in 1 draw call, the sparse one 51 in 1, the
prose bench about 2212 in 1, the grid one call per texture (36). The
UI thread is unchanged on every probe (`paintMs`, p95, 0 missed
presents), and anvil's process CPU is below main's everywhere.

**The scissor.** Impeller set a GL scissor from the list's clip, so a
patch frame paid GPU only inside the patch; anvil applied the clip per
fragment and cleared and drew the whole MSAA target. The walk now
reports the union of the emitted instances' device clip boxes and the
executor scissors its clear and its draws to it, leaving the scissor
test off afterwards as the callers expect. On the desktop the dense
probe's patch frame fell from 11 to 16 ms of GPU to main's level (0.6 to
2.1); on the TV it stayed at 8.9 ms against 8.7 before, since there the
fragments of the rects inside the patch are the cost, not the area
outside it (the full frame reads 9.1).

**Sampler array.** Not needed on the desktop: the 36-texture grid costs
anvil 1.0 to 1.1 ms of GPU and 1.1 to 1.5 ms of frame work, the same as
main, at one draw call per texture. The TV threshold the plan set (more
than 2 ms of raster time per frame on the grid) was not read on the TV;
the option stays open with its design ready.

**TV** (Philips TPM171E, Mali-T860, 1920x1080 at 50 Hz, armeabi-v7a,
the branch client of `64e51022`, cadence hold off, window patch on, 100 s
idle then three reads of 10 s). The device's installed client of
2026-11-12 predates `flux:haptics` in core and could not load today's
bundles, so main's figures are the recorded ones read on this TV with
these probes (2026-10-20 and 2026-10-08).

| probe | main (recorded) | required | anvil, three reads | GPU ms | inst / frame | dc / frame |
|---|---|---|---|---|---|---|
| dense mover, 800 rects, clock x5 | 264 / 10 s | 495 to 500, 0 missed | 496, 499, 495; 0 missed | 8.7 | 665 | 1.02 |
| sparse mover, clock x5 | 500 / 10 s | 500, 0 missed | 500, 500, 500; 0 missed | 3.4 | 40 | 1.01 |
| prose bench | 50 locked, GPU 6.2 ms | 500, 0 missed, dc at most 3 | 500, 500, 500; 0 missed | 0.37 | 2 | 1.01 |

The dense probe's `frameWorkMeanMs` sits around the 20 ms budget (17.8
to 24.6) while every refresh presents, since the figure includes the
raster thread's wait on the GPU; the raster thread uses 18 to 21 % of one
core, the UI thread 10 to 13 %. The prose bench's cold field
(`POST /debug?name=cold`) peaks at 9.3 ms of frame work where Impeller's
27 layers peaked at 83 ms. With the window patch off (every frame a full
render) the dense probe holds 499 frames at 9.1 ms of GPU with 801
instances in one call, and the prose bench drops to 186 frames at 53.8 ms
with 1646 instances in one call: the glyph fill cost above. No drift
across the three reads of any probe; no same-day main APK was needed
(the dense probe is locked, not in the 27 to 49 fps band).

Tool defects met on the TV: `sol android --census` matches no
SurfaceFlinger layer for `com.solidrt.player.dev` although it lists the
layer; `cadenceHold` reads 1 in `/stats` although `SOLIDRT_CADENCE_HOLD=off`
was passed in `sol_env` (throughput was read from the frame counts);
this Android 8's `top -H` prints only the main thread, so the per-thread
CPU came from `/proc`. The TV's dev client is the branch's from
2026-11-18 on.

### What stage 2 taught

- A wide fragment program costs every fragment on Midgard. The one shape
  program read nine flat vec4 and four smooth varyings per fragment,
  branched on the kind, discarded in both paths and took derivatives;
  on the Mali-T860 that put a glyph fragment at about 53 ns where a
  dedicated program spends under 2 ns, fifty times under the fill rate,
  and only the glyphs had the area to show it. Glyph batches now have
  their own program; trimming the shape program's varyings is the next
  read (its full frame of 801 rects is 9.1 ms against a floor near 6).
- A per-fragment clip is not a scissor. Impeller scissored its GL work
  to the list's clip; anvil cleared and drew the whole MSAA target per
  patch frame until the walk reported the union of its clip boxes. The
  desktop paid for it, the TV did not: there the fragments inside the
  patch are the cost.
- The partial-repaint patch on the mover probes is a third of the window
  under both renderers; the route's union of the damage ring is the
  place to look, and the mover rows of any measurement measure that
  patch.
- Under partial repaint the per-frame instance and draw-call figures
  count the patch; the whole screen is the cumulative delta around a
  root snapshot, or a patch-off run on a multisampled window.
- Stage 1's removal of Impeller missed the bare library name in the
  Android activity's loader list (`getLibraries()` in
  `lattice/android/app/src/main/java/com/solidrt/app/SolidRTActivity.java`):
  SDL's loader stopped at the missing `libimpeller.so`, `libmain.so`
  never loaded and the first JNI call killed the process. A grep for
  the crate name does not find a library name in Java.
- Gradle packages every library under `dist/android/<abi>/`, so a
  `libimpeller.so` staged by an earlier main build rode into the
  branch's first APK until it was deleted.
- A network `adb install` to the TV can stall for good; `adb
  kill-server` then `adb connect`, and a `timeout` per install, make the
  retry safe.
- The TV's installed client is only as new as its last install: a
  client that predates a core module (`flux:haptics`) shows an error
  screen on today's bundles, and a same-day baseline of main on the
  device needs main's own APK.
- Recordings over atlas placements are sound as long as the atlas never
  reuses a GL name while a recording may still draw from it; a new name
  per repack plus a generation that damages the text nodes gives that
  with no per-glyph work on the raster thread.
- The desktop's GPU timer counts a wait for a free window buffer on
  frames that demand every refresh; reads split into a high group near
  the frame period and a low group that is the work, and only the low
  group compares renderers.

### Stage 2: the fill program (2026-11-18)

Plain rect fills (kind rect, untextured, no stroke, no rounded clip) draw
through a third program, `fill`: it reads `vDevice`, `vColor` and `vClip`,
applies the clip box as a factor and never discards, as the glyph program
already did; no derivatives, no kind branch. A batch carries its program
(`BatchProgram` in `walk.rs`: fill, shape, glyph). A plain rect joins an
open fill batch, or rides along in an open shape batch when its device
area is below `FILL_FOLD_AREA` (65,536 device pixels, a tuning value to
re-read on the TV), so a list that interleaves rects with other shapes
does not multiply its draw calls; a large rect, or one with no shape
batch open (every app background), opens a fill batch. The area is the
unclipped quad's. Pixel output is identical: the six core examples
differ in no pixel from the branch's output before the change, and the
shape program's rect arm computed coverage 1 with the same clip
predicate, so discard against writing zero is the same under
premultiplied source-over. One case draws in two calls where it drew in
one: rects first, then a texture, then a rect (the leading rects are a
fill batch now).

TV (Philips TPM171E, Mali-T860, 1920x1080 at 50 Hz, release client of
the fill-program tree over `aed5ba5a`, cadence hold off, clock x5 on the
mover, three reads of 10 s after a 100 s idle; "before" is the stage 2
measurement above):

| probe | before: GPU ms / frame | after: GPU ms / frame | frames / 10 s | missed | inst / frame | dc / frame | frameWorkMean ms |
|---|---|---|---|---|---|---|---|
| prose bench, window patch off | 31.8 (186 frames) | 13.3 | 500, 500, 500 | 0 | 1646 | 2.0 | 22.4 |
| dense mover, patch on | 8.7 | 4.2 | 500, 500, 500 | 0 | 665 | 1.03 | 8.8 |
| dense mover, patch off | 9.1 | 3.4 | 500, 500, 500 | 0 | 801 | 1.02 | 6.9 |

The prose bench's full frame is now the text's 7.6 ms plus about 5.7 ms
of fills and compositor; the dense mover's full frame of 801 rects in one
call is 3.4 ms, below the patch-on frame's 4.2 (the patch route's resolve
and copy). The desktop is unchanged (zero pixel diff; draw calls per
frame as before but for the case above).

**Stage 2 is done.** Its Done criteria hold under the default partial
repaint and with the window patch off: the dense mover and the prose
bench run locked at 50 fps on the TV, and a screen of prose draws in two
or three batches. Open from the stage, into the backlog or later stages:
the partial-repaint patch that is a third of the window on the mover
probes under both renderers (`okf/backlog/repaint-patch-union.md`); the
sampler-array TV read, not taken (its design is above); `FILL_FOLD_AREA`
not yet re-read on the TV.

### What the fill program taught

- The fragment program's shape, not the instance count, set the Mali's
  cost: the same 801 rects in the same single draw call went from 9.1 ms
  to 3.4 ms by dropping discards, derivatives and unused varyings from
  the program they draw through. The mesh program and the direct
  shaders should start minimal and grow, not share an uber-shader.
- A batch's program is a property of the batch, and a cheap-program
  instance may ride along in an expensive-program batch (the shape
  program draws a rect correctly) when that saves a draw call: the
  decision is an area threshold, because the saving is per fragment and
  the cost is per call.
- The reference for a refactor that must not move pixels is the branch's
  own output before the change, not main: a zero diff at tolerance 0 is
  a stronger check than any diff against Impeller.

### Stage 3: the paint shader, and the desktop measurement (2026-11-19)

Steps 1 to 3 landed on 2026-11-18 and 19 (06dc74f5 anvil, 2010a13a
alloy, 799a07c6 flux and core, a026ebcd guides and backlog), each
reviewed before its commit. What the implementation settled beyond the
design above:

- The app's `main` is renamed by macro (`#define main solidrt_paint_main`)
  under `#line 1`, so a compile error reports the app's own line, and the
  epilogue multiplies `fragColor` by the clip factor (box and rounded
  clip), never discards. A source with a `#version` directive (the
  directive, not the word in a comment) is refused; a source without a
  `main` gets an error that says so instead of the GL log's hidden symbol.
- `uSize` and `iResolution` are set after the params, so a param cannot
  displace them; `uSize` is left out of the UI-side uniform table, and
  declared-but-inactive names get inactive slots, so params validate as
  `linkProgram`'s do. An input must be an active `sampler2D` over a 2D
  color texture the renderer holds a handle for; an unknown id throws as a
  shader target's binding does.
- The deletion queue holds textures and programs; a retained recording
  keeps its program alive past `destroyPaintShader`, and the example
  checks a list recorded before the destroy still draws. A paint shader
  the new context refuses on recovery is culled from the UI registry, so
  no stale program name is ever recorded.
- Sampler objects are per (filter, wrap), created on first use; a paint
  batch binds its inputs on units 0.. and the gradient ramp is rebound
  after it when the frame has glyph runs.
- `alloy/examples/paint_shader.rs`: the direct draw equals the same
  source through a shader texture at 128x128 with a largest channel
  difference of 0, and the rect clip, the rounded clip, a 45 degree
  rotation and the retained list behave.
- A `<shader>` whose input texture changes every frame re-records its
  node and the recordings above it (`Damage::Paint`), where `<texture>`
  takes the present-only path: accepted for the stage, in
  `okf/backlog/shader-input-content-change-rebuilds.md`.

**Desktop** (`probes/paint-shader-bench.tsx`, `route=paint|texture`,
36 animated 160x90 panels driven by `uTime` every frame; release client
of 799a07c6 at 60 Hz, 1280x720, scale 1; three reads of 10 s each after
15 s; `probes/paint-shader-bench.sh` is the runner):

| route | frames / 10 s | missed | draw calls / frame | offscreen passes / frame | pass GPU ms / frame | process CPU % |
|---|---|---|---|---|---|---|
| `<shader>` | 599 to 600 | 0 | 37 | 0 | 0 | 8.7 to 10.0 |
| shader texture + `<texture>` | 599 to 600 | 0 | 37 | 36 | 0.68 | 10.9 to 11.8 |

Both routes lock at 60 with 36 panels, so the desktop separates them
only structurally: the texture route runs 36 offscreen passes per frame
(0.68 ms of GPU in the pass timer, 0.29 ms of raster-thread issue time)
and holds 37 textures and 36 pipelines in `/gpu` against one paint
shader and one texture; the paint route's process CPU is about two
points lower. The desktop's GPU frame timer is bimodal here as the stage
2 measurement found (one run's paint read sat in the low group at 4.8 to
5.1 ms per frame, the next in the high group at 15.4 with the texture
route), so it does not compare the routes.

**TV** (Philips TPM171E, Mali-T860, 1920x1080 at 50 Hz, a 960x540
logical window at scale 2, armeabi-v7a; the branch client of 799a07c6,
cadence hold off; 100 s idle then three reads of about 13 s, the second
and third below as per-frame deltas; per-thread CPU from `/proc`, percent
of one core). The probe's default seven columns are 1184 logical pixels,
wider than the TV's window, so the TV read runs `columns=5 panels=30`:
every panel on screen, both routes drawing the same 30 (the last row
clipped by the window's bottom edge alike). With the default grid the
paint route culls the five off-screen panels while the texture route
still renders their 36 passes, so that read is kept only as a check.

| route | frames / s | missed / read | draw calls / frame | passes / frame | frame GPU ms | pass GPU ms | pass issue ms | frame work ms | raster thread | UI thread | process CPU | textures, programs |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `<shader>`, 30 | 50.0, 50.0 | 0, 0 | 31 | 0 | 14.3 | 0 | 0 | 24.7 to 26.1 | 14.7 % | 38 % | 51 % | 1 texture (the glyph atlas), 1 paint shader |
| shader texture + `<texture>`, 30 | 25.2, 25.2 | 345, 346 | 31 | 30 | 29.9 | 6.5 to 6.7 | 34.6 | 67.0 to 67.3 | 54.8 % | 20 % | 81 % | 31 textures, 30 pipelines |
| `<shader>`, 36 default grid (31 drawn) | 50.0, 50.0 | 0, 0 | 32 | 0 | 14.8 | 0 | 0 | 24.2 | 15.2 % | 44 % | 56 % | 1, 1 |
| texture, 36 default grid (31 drawn) | 22.8, 22.8 | 376, 380 | 32 | 36 | 31.4 | 9.3 | 38.7 | 72.0 to 72.3 | 56.8 % | 21 % | 86 % | 37, 36 |

The TV separates the routes where the desktop could not. The paint route
holds the refresh with 0 missed presents across the reads (4 in the first
100 s, the start-up); the texture route presents every other refresh:
its raster thread spends 34.6 ms per frame issuing the 30 offscreen
passes (about 1.15 ms of driver time per pass on this Mali, with 6.6 ms
of GPU behind them) and is the bottleneck, not the GPU. The paint route's
14.3 ms of frame GPU is near the full-frame floor stage 2 estimated
(about 6 ms) plus the plasma fragments themselves (the texture route
renders the same 1.73 million fragments in 6.6 ms of pass time), with
about 1.7 ms left for the 4x MSAA window target's own work on those
pixels; no per-pass cost shows. The UI thread's share is the 30 `params`
writes per frame on both routes (38 % at 50 frames per second against
20 % at 25, about the same per frame). Both routes hold their numbers
across the reads; memory sits at 102 MB against 126 MB. The 36-panel check reads the same way with the five culled
panels discounted.

### What stage 3 taught

- A shader panel's cost was never the shader: on the TV, 30 offscreen
  passes cost 34.6 ms of raster-thread issue time for 6.6 ms of GPU. The
  per-pass driver work (target bind, clear, program and uniform setup,
  the pass's own submission) is what the direct draw removes; the
  fragments cost about the same on both routes (shaded into the 4x MSAA
  window target on one, into rgba8 targets on the other).
- Culling by the window changes what a probe compares. A grid wider
  than the device's logical window drops panels on the direct route and
  keeps their passes on the offscreen one; a probe for two routes has to
  put the same work on screen for both (`columns=`), and the tree
  (`/tree`) is the check that it did.
- The GPU frame timer compares routes on the Mali where it does not on
  the desktop: the paint frame reads as floor plus fragments, with no
  bimodal split.

**Stage 3 is done.** Its Done criteria hold: the example renders; the GL
example diffs the direct draw against the shader texture with a largest
channel difference of 0; the rect clip, the rounded clip and a rotation
draw; params and inputs validate as `linkProgram`'s do; a retained
recording keeps a destroyed handle's program alive until it is dropped
and the program is deleted after; recovery relinks and culls a refused
shader from the registry; the probe's numbers are above for the desktop
and the TV. Open from the stage, into the backlog: a `<shader>` whose
input texture changes every frame rebuilds its recordings
(`okf/backlog/shader-input-content-change-rebuilds.md`).

### Stage 4: paths and gradients, and the desktop measurement (2026-11-19)

Steps 1 to 3 landed on 2026-11-19 (69d2a662 gradients, c20a7903 the
anvil half of paths, 7b90528e the alloy half), each reviewed before its
commit. What the implementation settled beyond the design above:

- A gradient's device rows compose with the inverse of the state's map
  for every op, a line's included: the gradient lives in the op's space,
  and the line's quad reaches device space through its own frame map and
  then the state's, so the state's inverse alone sends a device pixel
  back to where the gradient is defined. The brief had said the line's
  composed map; the review and the walk test (a line starting at x = 20
  reads t = 0.5 at device x = 50) settled it.
- A degenerate gradient source (a zero-length axis, a non-positive
  radius, a singular transform, no stops) warns once through a
  `warn_once!` beside `unsupported!`, since it is not a missing feature,
  and the shape draws its color. The miter limit is clamped to lyon's
  minimum of 1 once at record time, so the cache key, the walk's reach
  and the tessellator see one value (a NaN limit lands on 1 too).
- A tessellation error, and a path that yields no triangle, are cached
  as an empty mesh: the tessellator is deterministic, so the costly case
  is not repeated every frame; the miss flag still counts the run.
- The GL examples are two: `alloy/examples/gradient_fill.rs` (eight
  gradient cases against CPU-evaluated colors, the line case starting
  inside the target so a frame-space evaluation would fail it) and
  `alloy/examples/path_mesh.rs` (both fill rules, the miter tip against a
  bevel and a round join, the three caps, a dashed curve, gradient and
  image fills under a rect clip, a rounded clip and a rotation, a
  stroke-and-fill star, and the cache's 1/0/1 tessellations across a
  redraw and a doubled scale).
- The first desktop read of the static route tessellated 28.75 meshes
  per frame against the expected 0: a polyline (`kinds/line.rs`) built a
  fresh path on every rebuild of its list, and any repaint in the list
  rebuilt every polyline in it, so the mesh cache on the path never saw a
  second frame. The line kind now keeps its drawn paths (the fill's and
  the stroke's) across builds, cleared when the points or `closed`
  change, as the path kind keeps its `d`'s; the dashed path is still
  built per build, as a moving pattern is new geometry. After it the
  static route reads 0.
- A two-point line resolved its box gradient against no box and drew the
  stops' average: it now resolves against its endpoints' extent, a zero
  side grown by half the stroke width on each end.
- The pixel diff against main's reference client could not run: the
  reference binary in `target/main-ref/` predates stage 3's `flux:gpu`
  exports and refuses a bundle built from today's core. The reference
  needs rebuilding from a main checkout with its own core before stage
  7's diff; the path examples were checked by eye (`parse-svg`,
  `on-layout-connect`, `line-points`: every icon, curve, trace, dash and
  cap draws, no `[anvil]` warning), and every refactor step diffed at
  zero against the branch's own previous output.

**Desktop** (`probes/path-bench.tsx`, 30 panels in 5 columns, each a
stroked polyline of N points over a gradient rounded rect; `route=live`
rewrites every polyline each frame, `route=static` keeps them and
flickers a full-window veil so every frame repaints the whole window;
release client of 7b90528e at 60 Hz, 1280x720, scale 1; three reads of
10 s each after 15 s; `probes/path-bench.sh` is the runner):

| route | points | frames / 10 s | missed | draw calls / frame | meshes / frame | tessellated / frame | UI frame p50 ms | process CPU % |
|---|---|---|---|---|---|---|---|---|
| live | 64 | 600 | 0 | 61 | 30 | 30 | 1.42 | 15.5 to 16.4 |
| live, stroke-and-fill | 64 | 599 to 600 | 0 | 61 | 60 | 60 | 1.4 | 18.7 to 20.1 |
| live | 256 | 599 to 600 | 0 | 61 | 30 | 30 | 3.18 | 32.4 to 33.2 |
| static | 64 | 573 to 575 | 0 | 62 | 30 | 0 (28.75 before the line cache) | 0.23 | 7.4 to 8.3 (9.0 to 10.0 before) |
| static | 256 | 574 | 0 | 62 | 30 | 0 | 0.23 | 11.2 to 12.6 (15.3 to 16.3 before) |

Every route holds 60 with 0 missed presents. The desktop's GPU frame
timer sat in its high group (15.6 ms, the wait for a window buffer) on
every read and compares nothing. The process CPU is the figure that
moves: tessellating 30 strokes of 256 points per frame at 60 Hz costs
about 22 points of a core over the static route, about 0.12 ms per
stroke on this desktop, paid on the raster thread where the scale is
known; the static route's remaining cost is the per-frame transform and
upload of the cached meshes (30 strokes of 256 points are about 15,000
vertices, 600 KB a frame at 40 bytes each). The static route's 573 to
575 frames are the veil's 16 ms flicker coalescing with the refresh. The
61 draw calls are 30 sealed gradient batches alternating with 30 mesh
batches over the background: a gradient draw is a batch of its own (the
one ramp texture is rewritten per batch), so a grid of gradient panels
with content between them cannot merge; a ramp atlas would let gradient
batches join (`okf/backlog/gradient-ramp-atlas.md`).

**TV.** Not read in this session: the TV is reachable (`adb connect
192.168.2.11:5555`) but installing a client on it is the maintainer's
call per session. The bench takes `panels=30 columns=5` as the paint
shader bench did; the figures to read are the frame rate and missed
presents on the live route at 64 and 256 points (the armv7 tessellation
cost), the raster thread's CPU on the static route (the per-frame vertex
transform and upload, the case for a slot table or retained GPU meshes),
the mesh program's GPU frame time against the fill program's, and the
vertex counts `MESH_TOLERANCE` produces (0.25 device pixels, to re-read
there).

### What stage 4 taught

- A cache on the geometry only helps when the element keeps the geometry:
  the renderer's mesh cache was correct from the first commit and useless
  for polylines until the line kind kept its path across rebuilds. The
  count the stats carry (`rendererTessellatedPerFrame`) is what made it
  visible; a bench that repaints without rebuilding (the veil) is what
  reads the retained case at all, since a static screen demands no frame.
- Evaluating a gradient in device space, through rows composed in the
  walk, removed a varying from every program and made lines, rotations
  and nested transforms right for free; the composition is two 2x3
  products per op. The op's space, not the quad's frame, is where a
  gradient is defined, and the map to invert is the state's.
- Clamp a value once where it enters (the miter at record time), not in
  each consumer: a key that holds the unclamped value misses the cache on
  every animated frame and never matches a NaN.
- A reference binary ages: main's client could not load today's bundle
  two stages later. The reference for a refactor is the branch's own
  previous output (zero diffs at every step here); the reference for
  parity needs a main checkout built whole.

**Stage 4 is done on the desktop.** Its Done criteria hold there: every
`draw_path` the rendertree records draws (both fill rules, caps, joins,
the miter limit, dashes); gradients draw on rects, rounded rects, ovals,
lines, paths and text through one ramp path; both examples' checks hold;
a retained list's redraw tessellates nothing; the walk tests cover the
batch rules, the rows composition, the cache key and the scale bucket;
the desktop numbers are above. Open: the TV read (above), the ramp atlas
for merged gradient batches (backlog), and the main reference client's
rebuild for stage 7's diff.

### Where anvil lives (2026-11-18)

anvil is a module behind the seam, `alloy/src/renderer/anvil/` (the plan
named `anvil.rs`; it grew into a folder). Its name and Impeller's
precedent point at a renderer of its own, outside alloy, owning more
than Impeller did, not less. Three scopes are in play:

1. **anvil today** owns less than Impeller: the recording and the
   execution of one list. Impeller also owned its surfaces and render
   targets; for anvil those sit in `alloy/src/gl/`.
2. **Impeller's scope plus the GL toolkit.** anvil takes the renderer
   work below and the toolkit, and alloy's own GL code builds on that
   toolkit. That is more than Impeller, which never offered a GL layer
   for alloy's code to use.
3. **All of `gl/`.** Scope 2 plus the host work below.

The proposal is scope 2, with the open items below decided case by
case.

**Renderer work**, into anvil:

- `gl/draw.rs` and `gl/rig.rs`: a list drawn into a texture, a retained
  layer or the window, through the MSAA rig (Impeller's surface and
  render-target role).
- `gl/glyphs.rs`, the old glyph pass. Stage 2 ported it into anvil
  (`draw_glyphs`, `KIND_GLYPH`), so a `<text>` no longer uses it; it
  stays only because the stats overlay draws its text through it and
  `Text::rasterize`. Nothing in the plan removes it yet: the overlay
  moves to `draw_glyphs` and the pass goes. Stage 7 moves the glyph
  vocabulary out of `alloy/src/gpu/glyphs.rs` but does not say where;
  with the pass gone, its home is anvil.
- The GL toolkit anvil already leans on: texture creation and the
  sampler cache (`texture.rs`), upload staging (`staging.rs`), stage
  compile (`program.rs`), target readback (`readback.rs`).

**Host work**, staying in alloy:

- Context bootstrap over ore's GL surface and EGL (`context.rs`,
  `egl_headless.rs`): a renderer crate does not depend on ore.
- Robustness and context-loss recovery (`robustness.rs`): what to do on
  a reset is a policy, and policy is the host's.

**Open:**

- The app-facing `gpu` protocol executors, about 5000 lines: programs,
  passes, targets, buffers, storage, slicing, entry, timing. They carry
  out a plain vocabulary over GL with no policy of their own, and stage
  3 draws app programs straight into anvil's frame, so anvil is a
  defensible home, maybe the cleaner one. If they stay in alloy, they
  build on anvil's toolkit, never the other way round.
- The window tap (`tap.rs`): either side.

**The links that point the wrong way** for a cut today: `gl/draw.rs`
imports `raster::{DamageRect, WindowRoute}`, `gl/rig.rs` reads `backend`
and `egl_headless` for its probes, and anvil imports `gpu`'s glyph
types (`GlyphQuad`, `CoveragePolicy`, `GlyphGradient`, `RAMP_WIDTH`).
Each becomes a plain value passed in or a type that moves with anvil.

**Timing.** Not inside stage 2, whose rect fast path is next. Each of
the later stages adds code that reaches into `gl/`, so the cut is cheapest
before them: first a top-level `alloy/src/anvil/` with no `crate::`
import back into alloy (the contract, the vocabulary and the toolkit as
its surface), then the crate split, which by then is mechanical. Not
decided yet.

**Decided (2026-11-18).** anvil becomes a crate, `anvil/`, holding what
the seam holds today: the contract, the vocabulary, the op log (a cargo
feature, `oplog`, that alloy enables for its tests only, since
`cfg(test)` does not cross a crate boundary), the recording and the
execution, and the glyph vocabulary from `gpu/glyphs.rs`. It depends on
glow, euclid and log, nothing of ours. `alloy/src/renderer/mod.rs` stays
as a re-export so nothing outside it changes. Two corrections to the
section above led there: anvil uses none of the "toolkit" (`texture.rs`,
`staging.rs`, `program.rs`, `readback.rs`): its executor compiles its own
programs, owns its samplers and uploads its instance buffer itself; and
`program.rs` and `texture.rs` are not a neutral GL layer but the gpu
protocol's halves, built on `gpu::vocab` and `gpu::texture`, so pulling
them into anvil would make the renderer depend on the app-facing
protocol, the wrong direction. Scope 2 as written is therefore not a
cut that exists. `draw.rs` and `rig.rs` (the surface role, the MSAA
target) stay in alloy until stage 6's target pool gives anvil a target
abstraction of its own; that is when the question is asked again, with
`gl/draw.rs`'s `WindowRoute` and `DamageRect` as the values to pass in.
The old glyph pass goes now, not at stage 7: its one consumer, the stats
HUD's text (`Text::rasterize`), records its runs through `draw_glyphs`
like the tree walk and rasterizes the list through the general
list-to-texture path, and `GlyphGroup`, `RasterCmd::RasterizeGlyphs` and
`gl/glyphs.rs` leave with it. The app-facing gpu executors stay in alloy
(they carry out what lattice chooses, alloy's role).
