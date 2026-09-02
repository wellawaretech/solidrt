---
title: Impeller effects as element props (shadow, filter, backdropFilter)
description: Expose Impeller's built-in filters as three props - shadow on shape elements, filter on views, backdropFilter on views - closing the "where is box-shadow" gap without GLSL; folds in the former impeller-backdrop-filters item.
created: 2026-09-02
---

# Impeller effects as element props

Priority 6 of okf/notes/project-review-2026-08.md: Impeller has blur,
backdrop, shadows, color filters, path clips, masks; the rendertree exposes
none, and the escape hatch is custom GLSL, which is a power-user answer to
"I want a drop shadow". This item is deliberately the layer beneath the
shader tiers: built-in filters as plain props, no fragment programs, no
impellerc.

Folds in the former okf/backlog/impeller-backdrop-filters.md (created
2026-07-27); its open questions live on in stage 3 below.

## What is already there

The impellers 0.4.2 crate we link ships the full effect surface, none of it
called today:

- `MaskFilter::new_blur(BlurStyle, sigma)` with Normal/Solid/Outer/Inner.
- `ImageFilter::new_blur(x_sigma, y_sigma, tile_mode)`, `new_dilate`,
  `new_erode`, `new_matrix`, `new_compose`.
- `ColorFilter::new_blend(color, blend_mode)`, `new_matrix(ColorMatrix)`.
- `Paint::set_mask_filter` / `set_image_filter` / `set_color_filter`.
- `DisplayListBuilder::save_layer(bounds, paint, backdrop:
  Option<&ImageFilter>)` - we already call it for non-boundary group
  opacity (alloy/src/rendertree/composite.rs, one site) passing `None` for
  the backdrop. The backdrop feature is one argument away at a call site
  that already exists.
- `draw_shadow(path, color, elevation, ...)` - Flutter's elevation model,
  a possible later shorthand, not the base API.

Alloy today calls only clip_rect/clip_rounded_rect, blend modes, and that
one save_layer. These are built-in filters, not fragment programs, so the
toolchain question that rules out custom Impeller shaders does not arise.

Supporting machinery that already exists and is load-bearing here:

- `filter` was reserved for Impeller filters when the window/subtree
  `shader` props were named (okf/done/root-layer-effects.md,
  subtree-effects.md). This item spends that reservation.
- The paint-envelope machinery for painted-bounds-exceed-layout-bounds
  exists: `own_extent` in cull.rs already folds in AA and stroke outsets,
  and NodeShader `outset` set the precedent for a declared margin. Blur,
  offset and spread become new outset contributors, which matters now that
  partial repaint unions envelopes into damage rects.
- Every painted kind carries a `PaintState`; group opacity on views is the
  save_layer template stage 2 rides on.

## The placement argument

Shadows in practice (cards, modals, dropdowns, buttons, tooltips) are cast
by a surface - an opaque rounded rectangle - never by subtree content. CSS
conflates surface and container because a div paints its own background;
here views paint nothing and the surface of a card is already a `<rect>`,
so shadow-on-rect IS the container pattern in this tree. Flutter draws the
same line: boxShadow lives on BoxDecoration, the painted background, not on
the widget subtree. A view-level shadow would have no shape to cast (the
clipRadius box silently mismatches the background rect's radius, and views
without backgrounds would cast solid shadows behind transparent content).
The genuinely different feature, CSS drop-shadow of a subtree's alpha
silhouette, is expensive (subtree as texture plus blurred redraw), rare,
and already served by the snapshot-boundary `shader` escape hatch.

Filters get the opposite answer from the same test: real uses are
subtree-semantics (blur this panel, grayscale this disabled region), so
`filter` goes on views. One honest exception noted under deferred: CSS
filter on `<img>` is common (grayscale avatars, thumbnails), and a color
filter on the texture element's own PaintState is free (no save_layer)
where wrapping each thumbnail in a filtered view costs one offscreen per
image.

## The API

Object props throughout (one API shape, web object form); validation
throws per the dev/prod policy; blur lengths are logical px, converted with
a named BLUR_RADIUS_TO_SIGMA constant.

1. `shadow` on shape elements (`rect`, `oval`, `path`, `svg`, and `d-`
   forms): `{ x?, y?, blur?, spread?, color }`, CSS box-shadow field
   semantics. Named `shadow`, not `boxShadow`: a `<path>` has no box, and
   one name lets `text` take the same prop later where CSS needed a second
   name. Single shadow; an array form is additive later.
2. `filter` on views: keys are the CSS filter-function names - `blur` plus
   the color family (`grayscale`, `saturate`, `brightness`, `contrast`,
   `sepia`, `invert`, `hueRotate`, `opacity`). All color keys multiply into
   one ColorMatrix; application order is fixed and documented (color ops,
   then blur) since object form cannot express author ordering and these
   do not need it.
3. `backdropFilter` on views: same object shape and decode as `filter`,
   wired to the save_layer backdrop argument. Frosted glass with correct
   see-through semantics, the case a subtree effect fundamentally cannot
   provide (the subtree's texture does not contain the pixels behind it).

## Stages

1. **`shadow` on shapes.** Self-contained: a ShadowState on the shape
   kinds, decode in flux/src/alloy_plugins/properties/, one extra draw of
   the same shape (spread-inflated, offset) with a
   `MaskFilter::new_blur(Normal, sigma)` paint before the main draw -
   Impeller has a fast path for blurred rrects, so the common card shadow
   is cheap. Envelope contribution in `own_extent` (offset + spread + blur
   outset), `Damage::Paint`.
2. **`filter` on views.** `Option<FilterState>` on View next to `opacity`;
   non-boundary views put the filters on the existing save_layer paint,
   snapshot boundaries get them nearly free on the composite quad's paint.
   Envelope inflation for blur, `Damage::Compose`. Cost is honest and
   documented: like opacity, a filter on a non-boundary view forces an
   offscreen.
3. **`backdropFilter`.** Smallest code delta, most semantic risk, so last.
   Carried over from the folded item, still to work out:
   - Repaint-boundary trap: a backdrop read sees the current target, so a
     snapshot boundary between panel and backdrop puts them on opposite
     sides of an offscreen - same trap documented for blend modes in
     okf/done/texture-element-compositing.md. Warn, or refuse the
     combination.
   - Cost: forces an offscreen plus a read of the current target at that
     point in the display list. Region-sized, but measure on Android
     before offering it as a casual style prop.
   - Bounds and edge: save_layer takes explicit bounds; the natural choice
     is the node's layout box, but a blur samples beyond its own edge, so
     tile mode and bounds together decide what the panel edge looks like.

## Findings (stages 1+2 implementation)

- **The Impeller color-matrix translation column is normalized 0..1 in
  practice**, despite impeller.h documenting 0..255 - offsets per the doc
  render invert as solid white and contrast 2 as solid black. Details and
  evidence: okf/upstream/impeller-color-matrix-translation.md. Filters
  without a translation term hide the discrepancy.
- **An outer shadow must clip its own box out** (CSS semantics): drawn
  naively, a stroke-only or translucent shape shows the shadow through its
  interior. rect/oval wrap the shadow draw in a Difference clip of the
  casting shape; path deliberately does not (its shadow mirrors the drawn
  silhouette, like CSS drop-shadow).
- Blur image filters use Decal tiling so a blurred panel fades at its edge
  instead of smearing clamped border pixels.
- The filter rides the exact mechanics group opacity already had: the
  save_layer paint at the non-boundary record site, a save_layer wrap
  around a recording boundary's cached replay, the quad paint on a
  snapshot boundary. `Damage::Compose` for the same reason as opacity.
- Envelope growth: shape shadows union their offset+spread+blur reach into
  `own_extent`; a view filter's blur inflates the subtree envelope after
  the clip cut and widens the cull rect by the same reach so just-offscreen
  content still feeds the blur.
- **Captures exclude the node's own filter**, like opacity and the boundary
  shader (a capture records the subtree with composite-time effects
  hoisted out): `/snapshot?node=` of a filtered view returns unfiltered
  content; sample through a parent or a window crop instead. Pre-existing
  capture semantics, kept consistent on purpose.
- Stages 1+2 verified 2026-09-02 on desktop Linux (probes/effects-probe.tsx,
  release client): pixel assertions for grayscale r=g=b, exact invert of the
  reference panel, blur-blended stripe boundary, and recording-vs-snapshot
  boundary equality all pass; magnified crops show the under-box shadow clip
  and soft falloff. Static probe idles clean (missedPresents 0); the slow
  paint frames seen during verification correlate with full-window capture
  readbacks, not with effect painting. The stage 3 Android cost measurement
  still stands.

## Deferred (all additive)

- Color filters on `texture` via its PaintState (the img-filter reflex,
  free of save_layer) - do when a real list hits the wrap-in-view cost.
- `filter` on other shape kinds.
- `shadow` on `text`; shadow arrays; `elevation` shorthand on rect via
  `draw_shadow`.
- `clipPath` / masks; `dilate` / `erode`.
- Native transitions on effect fields.

Not in scope: anything the GLSL tiers already own (window `shader`,
snapshot-boundary `shader`).
