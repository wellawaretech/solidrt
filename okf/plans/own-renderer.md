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
