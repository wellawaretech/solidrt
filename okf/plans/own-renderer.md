---
title: Replace Impeller with an own renderer, in stages
description: An own renderer (anvil) behind the renderer seam, built on a branch that deletes Impeller outright, staged so minimal apps run on it first (rects, ovals, lines, texture quads, text through its layers, transforms, rect clips) and the binary size is measured against main at the first commit; then batching and rounded shapes, paths, shaders drawn straight into the frame, full clips, layers and effects, and parity. The end state draws text and generator shaders straight into the frame and keeps offscreen passes only where a pass reads its own input.
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

## Stage 3: paths, lines and gradients

- Fill and stroke tessellation through `lyon_tessellation` (a new pinned
  dependency; same author and same path type as the `lyon_path` and
  `lyon_algorithms` already in tree), over the `lyon_path::Path` each path
  element already caches (`kinds/path.rs`). Caps, joins, miter limit and
  fill rule come from lyon. Dashes are ours already (`kinds/dash.rs`).
- The mesh is kept on the element, keyed on the geometry, the stroke
  parameters and the tessellation tolerance for the scale, and is redrawn
  without retessellating while those hold.
- Meshes draw through a second program and batch like quads.
- Linear and radial gradients with transform and tile mode, through a
  ramp texture as the glyph pass's (`RAMP_WIDTH`). Image color sources.
- Lines (`kinds/line.rs`) draw as paths, as today.

## Stage 4: shaders straight into the frame

A program drawn as a quad in the frame, under the frame's clip and into
its MSAA target, with no offscreen texture. It applies to generator
shaders, whose output depends only on position, time and uniforms.
Shaders that sample `uSource` or keep `uPrevious` stay offscreen, as
today.

This is a second shader contract beside the offscreen one (`NodeShader`,
`gpu/spec.rs`): local coordinates and the quad's size come from the
renderer (not `gl_FragCoord` and `iResolution`), and the output is
premultiplied. How an app declares it on a node is an API decision for
core, made at the start of the stage. A direct shader draw ends a batch.

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
  The Impeller mentions in `alloy/CLAUDE.md`, the root CLAUDE.md and the
  `impeller-*` upstream notes are updated or closed.

## Not in scope

- Vulkan or Metal backends. GLES stays the one backend
  ([graphics-backend-strategy](../notes/graphics-backend-strategy.md)).
- Coverage AA as a replacement for MSAA.
- Changes to the recording API the rendertree calls, beyond the stage 4
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
  the program they draw through. Stage 3's mesh program and stage 4's
  direct shaders should start minimal and grow, not share an uber-shader.
- A batch's program is a property of the batch, and a cheap-program
  instance may ride along in an expensive-program batch (the shape
  program draws a rect correctly) when that saves a draw call: the
  decision is an area threshold, because the saving is per fragment and
  the cost is per call.
- The reference for a refactor that must not move pixels is the branch's
  own output before the change, not main: a zero diff at tolerance 0 is
  a stronger check than any diff against Impeller.

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
  4 draws app programs straight into anvil's frame, so anvil is a
  defensible home, maybe the cleaner one. If they stay in alloy, they
  build on anvil's toolkit, never the other way round.
- The window tap (`tap.rs`): either side.

**The links that point the wrong way** for a cut today: `gl/draw.rs`
imports `raster::{DamageRect, WindowRoute}`, `gl/rig.rs` reads `backend`
and `egl_headless` for its probes, and anvil imports `gpu`'s glyph
types (`GlyphQuad`, `CoveragePolicy`, `GlyphGradient`, `RAMP_WIDTH`).
Each becomes a plain value passed in or a type that moves with anvil.

**Timing.** Not inside stage 2, whose rect fast path is next. Each of
stages 3 to 6 adds code that reaches into `gl/`, so the cut is cheapest
before stage 3: first a top-level `alloy/src/anvil/` with no `crate::`
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
