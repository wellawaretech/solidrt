---
title: A renderer seam in alloy, the first step to replacing Impeller
description: The rendertree, the flux plugins and the lattice overlay call Impeller's display-list builder directly, and the raster side calls Impeller and passes its textures around (60 files name impellers); own vocabulary types plus opaque builder, list, path and texture types owned by one cfg-selected renderer module confine Impeller to that module with no visible change and no added per-op cost, so a replacement can be built as a second implementation and Impeller deleted.
created: 2026-10-19
---

# A renderer seam in alloy

## Goal

Replace Impeller with an own renderer
([replacing-impeller](../notes/replacing-impeller.md)). A build compiles
exactly one renderer; two renderers never share a runtime. This item is the
first step: it draws the line the replacement implements, with Impeller as
the only implementation, and changes no pixels and no per-frame cost.

## Problem

Impeller is woven through both halves of drawing.

**Describing a frame.** The rendertree's element kinds call
`impellers::DisplayListBuilder` directly, repaint boundaries cache
`impellers::DisplayList` values, path elements cache `impellers::Path`,
and the flux plugins parse props straight into Impeller enums. Counted
2026-10-19, outside tests:

| Where | Files naming `impellers` |
|---|---|
| `alloy/src/rendertree/` | 27 |
| `alloy/src/raster/` | 9 |
| `alloy/src/gl/` | 5 |
| `alloy/src/context/` and other alloy | 9 |
| `flux/src/alloy_plugins/` | 8 |
| `lattice/src/` (overlay, lib) | 2 |

Most of these imports are vocabulary: `Size`, `Point`, `Rect`, `ISize` and
`Matrix` (euclid aliases), `Color`, and the paint enums. The real coupling is
16 files that build display lists: the kinds (rect, oval, line, path,
texture, view, window), text and its decorations, `composite.rs`,
`boundary.rs`, `frame.rs`, `rendertree/mod.rs`, plus `gl/draw.rs`,
`raster/offscreen.rs` and `lattice/src/overlay.rs`.

**Executing it.** Impeller is called at only three entry points, but its
types travel everywhere:

- context creation: `ImpellerContext::new_opengl_es` in `gl/context.rs`
- texture adoption, five raw sites: `gl/context.rs` (the `adopt_texture`
  wrapper the raster targets call), `gl/draw.rs`, `gl/glyphs.rs` and twice
  in `raster/offscreen.rs`. Adoption hands Impeller ownership of the GL
  texture name, and only works for RGBA8888, which is why a texture entry
  (`gpu/texture.rs`) holds an `Option<Texture>`.
- drawing into a framebuffer, three `wrap_fbo` sites in `gl/draw.rs`: the
  window (FBO 0), the MSAA rig, and offscreen targets
- `impellers::Texture` is the texture handle in `raster/`, `context/`,
  `gpu/`, and on the public surface in `event.rs` and `backend.rs` (the
  context-restored event carries the re-adopted handles), and
  `impellers::DisplayList` rides in the raster commands (`raster/cmd.rs`)
  and the capture paths

So a replacement has no line to stand behind. It would have to edit every
one of these files, changing the description and the execution in one
move, and a pixel diff could not tell which half broke. `alloy` also
re-exports the whole crate (`pub use impellers` in `lib.rs`), so the
dependency leaks into flux and lattice.

## The seam is the builder, not a data list

Two shapes were weighed. An own plain-data op list (an op enum the
rendertree fills, translated into Impeller calls at draw time) was the
first draft, and it is the wrong seam for two reasons.

The builder already runs on the UI thread (`frame.rs` records the frame,
`context/mod.rs` sends the finished list to the raster thread). The
translation would have run on the raster thread, and
[display-list-op-cost](display-list-op-cost.md) measured that thread's
per-op walk as the TV's bottleneck: a second walk of every op lands on
exactly the thread that cannot afford it.

And the replacement does not want a data list. Its win on the TV, per
[replacing-impeller](../notes/replacing-impeller.md), is a retained
structure with meshes kept on the element. An op list rebuilt per frame is
the structure that note says cannot escape the per-op cost, so the second
renderer would consume the list by walking it once more.

What both renderers can honestly implement is the recording interface:
the builder calls the emit sites make today. Impeller implements it by
forwarding each call; the replacement implements it by retaining what it
needs. No translation pass, no cache of translations, no eviction rule,
and the path cache per element stays as it is.

## What done looks like

- A `renderer` module in alloy (`alloy/src/renderer/`) that owns the
  contract and selects one implementation at compile time: `mod.rs`
  states the interface, a cfg picks the implementing submodule, and type
  aliases export its types. No trait objects, no runtime switch.
- The contract is stated as traits with one implementation each, consumed
  through the aliases. That costs nothing at runtime and gives the second
  renderer compile errors where it falls short, instead of a list to keep
  by hand.
- Opaque renderer types: `DisplayListBuilder`, `DisplayList` (cheap to
  clone, shareable across threads, nested by reference so the boundary
  cache keeps reusing unchanged subtrees), `Path` and `PathBuilder` (the
  dash walker's `Pen` target and the per-element path cache), and
  `Texture`. For Impeller each is a newtype forwarding to the
  `impellers` type.
- Own vocabulary: geometry stays euclid under alloy's own aliases, `Color`,
  `Paint`, the paint enums (draw style, cap, join, fill type, blend mode,
  tile mode, clip operation, texture sampling) and the text enums
  (`FontWeight`, `FontStyle`, `TextAlignment`, see ideas.md) are alloy's
  own. Effects are described by what we use, not by Impeller's filter
  objects: gaussian blur image filter, blur mask filter, color matrix.
  Sources: color, linear and radial gradient with tile mode, image.
- The renderer context surface is what the raster side calls today:
  - create the renderer on the raster thread's GL context
  - import a GL texture as a renderer texture, fallible: the interface
    returns an `Option` because Impeller adopts RGBA8888 only; the
    replacement answers `Some` for every format and the `Option`
    bookkeeping in `gpu/texture.rs` and `raster/targets.rs` can then go
  - draw a display list into a framebuffer: the window, the MSAA rig, an
    offscreen texture
- Recording is allowed on both threads, as today: the rendertree records
  on the UI thread, the raster thread records the rig patch's clip wrapper
  (`gl/draw.rs`) and the offscreen flip (`raster/offscreen.rs`).
- Texture handles are the renderer's business. For Impeller the handle
  owns the GL name; the replacement defines its own lifetime. Callers hold
  and drop handles; the interface promises nothing only one implementation
  can keep.
- The Impeller submodule is the only code that names `impellers`.
  Everything else in alloy, flux and lattice names only `renderer` and the
  own vocabulary, and `pub use impellers` is gone.
- No visible change: the snapshot suites pass with identical pixels.
- No regression on the TV: the animated-frame cost in
  [display-list-op-cost](display-list-op-cost.md) is re-measured before
  and after, on a release client. By construction nothing per-op changes
  (an enum conversion and a newtype call), so this is a check, not a
  budget.

## What it involves

The recording vocabulary in use today, counted from the emit sites. It is
smaller than the list in
[replacing-impeller](../notes/replacing-impeller.md), because text now
draws texture rects from our own glyph layers (`text/mod.rs`) and needs no
paragraph op:

- draws: rect, rounded rect, oval, path, line, texture, texture rect with
  sampling, nested display list with opacity
- state: save, restore, save_layer with bounds, paint and backdrop filter,
  clip rect, rounded rect and oval with intersect or difference, translate,
  scale, transform
- paint: color, linear and radial gradients and image sources with tile
  mode, draw style, stroke width, cap, join and miter, fill type, blend
  mode (all 31 are reachable from JS), color filter, image filter, mask
  filter

Estimated size:

| Piece | Lines |
|---|---|
| Own vocabulary: `Color`, `Paint`, sources, filters, enums | ~250 |
| Contract traits, aliases, cfg selection | ~100 |
| Impeller newtypes: builder, list, path, texture, conversions | ~250 |
| Renderer context: create, import, draw | ~150, mostly code moving out of `gl/draw.rs` and `gl/context.rs` |

Under a thousand new lines, plus mechanical edits across the files above
that come out close to net zero: the builder keeps the method names the
emit sites call today (`draw_rect`, `save_layer`, `clip_rounded_rect`,
...), so those sites change their imports and little else. The flux
plugins parse props into the own enums instead of Impeller's.

Decisions to take while building it:

- **Geometry units.** `impellers::Point` and friends are euclid types with
  Impeller's unit. If alloy's aliases keep the same unit the newtypes
  convert for free; if not, every conversion is a transmute-free copy of
  two floats. Decide on the first conversion, not up front.
- **Text enums.** They move now. ideas.md already says to decide them
  together with this item, and the rendertree only becomes free of
  `impellers` if they move. `fontWeight` carrying the CSS number instead
  of a nine-step enum is a separate change and stays in ideas.md.
- **Headless and tests.** The alloy tests under `alloy/src/tests/` and the
  snapshot harness name Impeller types in places; they move to the
  renderer aliases like everything else, so a replacement build runs the
  same suites.

## Not in scope

The replacement itself: own executor, shape rasterization and effects,
stages 2 and up of [replacing-impeller](../notes/replacing-impeller.md). It
becomes a second implementation of this interface, selected by a build
feature while it reaches parity, verified by running the snapshot suites
against an Impeller build and a replacement build; then Impeller and the
feature go. This item changes where Impeller is named, not what draws the
pixels.

An own plain-data op list is also out of scope. It becomes worth having
only if something consumes it, such as a culling pass that stops handing
the renderer the full scene every frame
([display-list-op-cost](display-list-op-cost.md)); that pass would run on
the UI thread, where the damage is already known, and is its own item.
