---
title: Replacing Impeller with an own renderer
description: What Impeller still does for us, what an own GL renderer would cost (call-surface inventory, crate map, sizing, the parity tail), and what it would buy; iOS is the event that turns Impeller from nearly free into a Flutter-engine build we maintain.
created: 2026-09-07
---

# Replacing Impeller with an own renderer

The question that started this: we already took paragraph layout out of
Impeller, the 3d backend renders into a texture Impeller only adopts, paths
and lines are broken down before they reach it, and effects are the last
big thing it does alone. Could it be replaced entirely?

Analysis, not a decision. Nothing here is scheduled. The reason to write it
down is that the pieces existed in five places and the synthesis in none:
[graphics-backend-strategy](graphics-backend-strategy.md) has the interop
analysis but assumes Impeller stays,
[text-own-rasterizer](../backlog/text-own-rasterizer.md) covers the text
half, [display-list-op-cost](../backlog/display-list-op-cost.md) has the
performance motivation without naming a replacement, and the three
`upstream/impeller-*` files each record one unfixable defect without adding
them up.

## What Impeller still does

The import surface is misleading: about 60 files name `impellers::`, but
most take only vocabulary types (Color, Rect, Point, Size, Matrix). The real
dependence is five things.

1. **Executes the display list.** The whole frame is one DisplayList drawn
   through `wrap_fbo` on FBO 0 or the MSAA rig (`alloy/src/gl/draw.rs`).
2. **Rasterizes shapes.** rect, rounded rect, oval, path fill and stroke
   with caps, joins, miter and fill rules. Dashes are already ours
   (`kinds/dash.rs`). Anti-aliasing is already ours too: 4x MSAA through the
   rig, not Impeller coverage AA.
3. **Paint and state.** solid color, linear and radial gradients with
   transform and tile mode, blend mode, opacity, clip rect / rounded rect /
   oval, save, restore, 2d and perspective transforms.
4. **Layers and effects.** `save_layer` for opacity groups and color
   filters, backdrop capture, gaussian blur, mask blur for shadows, color
   matrix (`rendertree/composite.rs`, `kinds/filter.rs`, `kinds/shadow.rs`).
5. **Text.** Shaping through the typography context and glyph rasterization
   through `draw_paragraph`, plus font discovery and fallback including
   emoji. The owned layout already narrowed this to two calls,
   `WordCache::get_or_shape` and `draw_paragraph`.

Texture adoption does not need replacing, it disappears. Without Impeller a
texture is a GL name, which retires the RGBA8888-only adoption limit, the
`unadopted` bookkeeping in `raster/targets.rs`, the wrapped-target flip
conventions, and the ANGLE cross-context item in the backlog.

### The op vocabulary to reimplement

Counted from the emit sites in `rendertree/` and `raster/`, this is the
whole contract:

- draws: rect, rounded rect, oval, path, line, texture rect, paragraph,
  nested display list
- state: save, restore, save_layer, clip rect / rounded rect / oval,
  translate, scale, transform
- paint: color, color source (linear and radial gradient), draw style,
  stroke width / cap / join / miter, blend mode, color filter, image
  filter, mask filter

That is roughly a dozen draws and a dozen paint fields. Impeller is a few
hundred thousand lines of C++ and we need none of its generality: no
arbitrary blend modes, no runtime effects, no path ops, no coverage AA.

## What would be built

Estimates, not measurements.

| Piece | Lines |
|---|---|
| Own display list type and builder | 500 |
| Executor: batching, transform stack, scissor and stencil clips, layers | 2000 |
| lyon fill and stroke glue with per-element mesh caching | 500 |
| Effects: blur, color matrix, shadow mask, backdrop passes | 800 |
| Text engine: shaper glue, atlas, word cache rework, fallback, emoji | 2500 |
| Pixel parity harness | 1000 |

Call it 7 to 8 thousand lines of Rust and GLSL, plus mechanical edits in the
24 non-test files that emit Impeller calls today. For scale, `alloy/src/gl/`
is 5300 lines and the owned text layout was about 2200.

### Crate map

What exists already, and what would be new. Checked against Cargo.lock
2026-09-07.

| Need | Crate | In tree? |
|---|---|---|
| Path building, bounds, hit test | lyon_path, lyon_algorithms, lyon_geom | yes |
| Fill and stroke tessellation | lyon_tessellation | no, same author and same path type |
| Segmentation and line breaking | unicode-segmentation, unicode-linebreak | yes |
| Font table reading | ttf-parser | yes |
| Bidi | unicode-bidi | no |
| Shaping | rustybuzz or harfrust | no |
| Glyph rasterization | swash | no, proven in the spike |
| Font discovery and fallback | fontdb, or fontique for per-script policy | no |
| Glyph atlas packing | etagere | no, used by WebRender's texture cache |
| SVG normalization | usvg | yes |

Correction worth keeping: rustybuzz and fontdb are **not** already in the
tree. `usvg` is pulled with `default-features = false` precisely to drop the
text and font stack (forge/Cargo.toml says so), so the shaper and the font
database would be genuinely new dependencies. `unicode-linebreak` and
`unicode-segmentation` are in, from the owned text layout.

Deliberately not on the list: parley and cosmic-text bundle shaping,
fallback and line layout together, and we own line layout already, so they
fight the seam instead of filling it. vello_hybrid is the only thing
offering coverage AA without MSAA, but it targets wgpu and WebGL2 rather
than a GLES context we drive.

lyon's provenance, since it gets overstated (this note previously did):
lyon is by a Mozilla graphics engineer who worked on WebRender, but lyon is
**not** used in WebRender or Firefox, which rasterizes paths on the CPU
through Skia. Its production use is the Rust GUI ecosystem: iced, ggez,
Bevy's vector plugin. etagere, by the same author, **is** in WebRender.

## The text half and the swash spike

`spikes/swash-text/` (on this machine only, `spikes/` is gitignored, which
is why its result is recorded here) answers the question
[text-own-rasterizer](../backlog/text-own-rasterizer.md) raised. Two
renderers behind one toggle, against a SolidRT app rendering the same lines
for an Impeller baseline, tiled side by side in `compare-1x.png` and
`compare-2x.png`:

- **swash**: CPU-rasterized glyph masks in an R8 atlas, 1/3-px subpixel
  positions, optional emboldening
- **slug**: no atlas, quadratic outline curves in an RGBA32F texture,
  coverage computed analytically in the fragment shader

Four coverage-to-color modes (naive sRGB as the Impeller-equivalent
baseline, linear-light, polarity-aware contrast remap, and a hybrid), gamma
and embolden tunable, at pixel sizes 6 to 16 on both polarities. Eight
screenshots are on disk. This note records no verdict on which mode wins,
because that is a visual judgment; what the spike settles is that the
mechanism works and the coverage-to-color step becomes ours to shape, which
is the whole reason the Medium-weight default exists
([dpi-aware-default-font-weight](../backlog/dpi-aware-default-font-weight.md)).

What the spike does **not** cover, and what therefore remains the work:
shaping (it maps chars through the charmap and advances by glyph width, so
no kerning, ligatures or complex scripts), font discovery and fallback
across five platforms, colour emoji, hinting policy, and integration with
the word cache and the raster thread's GL ownership.

## What it would buy, ranked

1. **Per-op CPU cost on slow devices.** The measured TV ceiling is a
   display-list op-count problem: 800 static rects run at 26.7 fps, 50 run
   vsync-locked at 50, with damage constant at 660 px
   ([display-list-op-cost](../backlog/display-list-op-cost.md)). An
   immutable display list rebuilt per frame cannot escape that. A retained
   tree with per-element damage can keep tessellated meshes on the element
   and redraw with a uniform transform, which is the only item on this list
   that changes product performance.
2. **Text quality and semantics.** The spike's result, plus glyph positions
   for O(n) caret stops instead of prefix re-shaping, cross-word shaping,
   and fallback as our policy.
3. **One owner of GL state.** No save/restore around Impeller's cached
   state, no wrapped-target flip conventions, no adoption. Any texture
   format composites directly, which HDR and rgba16f UI content want.
4. **Fixability.** Three upstream defects are permanent today because we
   consume a prebuilt: the color matrix translation column, the unusable
   GlyphInfo bounds, and `ImageFilterCreateMatrixNew` returning null for
   identity, which is why `kinds/filter.rs` substitutes a sub-pixel blur to
   trigger backdrop capture. Owning the renderer makes each a normal fix.
   Backdrop blur cost on Android becomes ours to tune as well.
5. **Build simplicity, iOS, binary size**, in that order. See below.

What it does **not** buy: GPU-bound work gets no faster, and anti-aliasing
parity becomes a risk to manage rather than a win.

## Cost: the parity tail

The line count is the cheap part. The expensive part is that correctness is
currently defined as "what Impeller drew":

- Stroke joins on degenerate paths, dash phase, miter limits, rounded clips
  under transforms, nested save_layer bounds with blur outsets, gradient
  tile modes at edges. Dozens of small diffs, each a day.
- Damage and partial repaint were tuned against Impeller's output, so
  patched regions must match pixel for pixel or the rig and buffer-age
  machinery gets re-verified.
- Five platforms. Impeller absorbed driver quirks for us; the Windows
  present-fence and GPU-crash episodes show what one platform can cost.
- Text lands last, so the interim state runs two engines side by side.

Honest estimate: first correct pixels on Linux in weeks; parity across the
suite and platforms with text migrated and Impeller deleted is a
quarter-scale project, most of it verification rather than writing.

## Binary size, measured

| Measure | Size |
|---|---|
| libimpeller.so, prebuilt, stripped | 8.6 MB |
| libimpeller.a, unstripped static archive | 61 MB |
| Impeller and its deps by demangled symbol bytes in the linked Linux binary | 4.5 MB of 41 MB |
| solidrt-go, Linux release | 68 MB |
| Android arm64 APK | 50 MB |

Dead-stripping already trims most of the archive, and lyon plus swash plus
the executor cost a megabyte or two back. Net gain is a few megabytes on a
binary whose bulk is elsewhere. Not a reason on its own.

## Build complexity, and why iOS changes the arithmetic

We never compile Impeller. The impellers crate's build script curls a
prebuilt zip from a third-party GitHub release pinned to an engine SHA. That
is nearly free until it is not:

- We cannot patch it, hence the three permanent defects above.
- 32-bit armv7 needs live bindgen against the real target.
- The build reaches the network, so offline and reproducible builds depend
  on a release asset staying up.
- The build script panics on any target OS outside windows, macos, linux and
  android. **iOS is not supported.**

That last line is the tipping point. iOS means building the Flutter engine
ourselves with depot_tools, gn and ninja, then vendoring a patched crate,
and redoing it as Xcode, SDK minimums and submission rules move. Once we
maintain a Flutter engine fork, the distance to maintaining a renderer we
wrote is small, and the renderer is the one that also fixes the TV frame
cost and the text quality.

Dropping Impeller does not remove iOS native build work entirely: the tower
still needs ANGLE, because Apple's GLES is deprecated and the strategy is
GLES-over-ANGLE everywhere. But ANGLE is a fraction of the Flutter engine,
has independent prebuilt options, and is a dependency four platforms already
carry. iOS goes from two foreign native builds to one.

There is no half measure: iOS without Impeller means the replacement anyway,
and keeping Impeller on four platforms with something else on iOS breaks the
one-contract principle
([graphics-backend-strategy](graphics-backend-strategy.md)).

## Two arguments that turn out not to bite

**"Impeller compiles shaders ahead of time, we would reintroduce jank."**
Skia's problem was combinatorial: shader source built at draw time from an
open set of paint, effect, blend and clip combinations, so a new combination
compiled mid-animation. Impeller's fix is a closed catalogue compiled up
front, and on GLES there is no offline format anyway, so Impeller itself
compiles its GLSL at context creation. Our catalogue is about ten programs
(solid, gradient, textured quad, SDF rect and oval, glyph, blur, color
matrix, stencil cover, composite blit), all known at build time, so the same
trick applies: compile on the raster thread at context creation, cache the
driver binaries on disk via the program-binary extension, and issue one
warm-up draw per pipeline to flush lazy driver specialization. The gpu
module already treats compilation as a blocking call at creation, never at
draw.

**"Impeller does the work on the GPU, lyon is CPU."** Impeller flattens
curves on the CPU every frame, strokes on the CPU every frame, and generates
rect and rounded-rect vertex strips on the CPU every frame; its GPU
contribution to geometry is stencil-then-cover for fills, plus rasterization
and blending. lyon's split is the same for strokes and more CPU-side for
fills, but we would run it once per geometry change rather than once per
frame. The intended mix is hybrid, as Impeller's is: SDF quads for rects,
rounded rects and ovals (which is most UI geometry, and gives analytic edges
independent of MSAA), cached lyon meshes for static paths and strokes, and
stencil-then-cover as the fallback for large fills that change every frame.

## Bearing on Vulkan and Metal

[graphics-backend-strategy](graphics-backend-strategy.md) names Impeller
texture interop as the one hard blocker for any non-GL backend: adoption and
extraction are GL-only in both directions, and GL adoption cannot exist on a
Metal context. Removing Impeller dissolves that blocker completely.

It does not overturn the decision. The strategy rests on a second,
independent argument: one GLES contract translated by ANGLE makes the Linux
dev box continuous conformance testing for every platform, and per-platform
native paths break that structurally. That holds with or without Impeller.
Two things do change: the Metal contingency shrinks from "GL inversion
rehearsal, self-built libimpeller with an extraction getter, alloy Metal
lowering, naga" to just the last two, and iOS stops being double work.

## What would de-risk this next, cheaply

- **A lyon spike beside the swash one.** Tessellate the paths the trails and
  relay demos actually draw, time it per frame on the TV-class CPU, and
  compare against the measured display-list op cost. That is the one number
  this note asserts rather than knows.
- **Stage 1 is worth doing on its own merits**: an own display list type
  plus a translator to Impeller's builder. No visible change, Impeller
  collapses to one file, and the rendertree stops importing `impellers::`,
  which the engine-independence rule in CLAUDE.md already asks for.
- **Decide what happens to `spikes/`.** It is gitignored, so the swash work
  exists on one machine and is referenced nowhere else.
