---
title: Padding makes paint and hit size against different boxes
description: Paint used to hand every laid-out element its content box as ctx.size while hit testing passed the border box, so padding made the two sides size against different boxes. Settled in two rounds - a View's own matrices resolve against the border box on every path (2026-08-08), and per-kind geometry (2026-08-21) - shapes default to the border box, text sizes AND places against the content box, both derived from one LayoutData::content_box on paint and hit alike.
created: 2026-08-08
completed: 2026-08-21
---

# Padding makes paint and hit size against different boxes

Found 2026-08-08 while fixing [[overflow-viewbox-clip]]: the two sides of the
renderer resolve "the element's size" differently as soon as the element has
padding.

- **Paint**: `record_node`'s child walk sets `ctx.size` to the child's border
  box minus the child's own padding (the content box) before recursing.
- **Hit**: `hit_recursive` passes the child's full `layout.size()` (the
  border box).

Without padding the two are equal, which is why nothing ever surfaced.
Detached children are consistent on both sides (they inherit the parent's
border box, or the design size under a viewBox parent).

## The View half - settled and fixed (2026-08-08)

A View's own matrices used to read `ctx.size`: the transform center
(`resolve_center`), the viewBox fit (`fit_matrix`), and the boundary-hoisted
user chain all resolved against the content box in paint, while hit resolved
the same matrices against the border box. On a padded view, a rotation
pivoted around a different point than it hit-tested, and a viewBox fit used a
different scale - the same divergence shape as [[overflow-viewbox-clip]].

Settled: **a View's own matrices, clip and scroll are box properties and
resolve against the border box on every path** (the transform center of a
padded card is the visual card center; the overflow clip rect is the border
box already). `composite::own_matrix` and `record_node`'s fit now read
`layout.size()` (falling back to the inherited frame for detached views),
matching what hit always passed. At the time `ctx.size` stayed the content
box for kinds' `build()`; the kinds round below replaced that with the
size/content split. Pinned by
`view_box_fit_resolves_against_the_border_box_when_padded` in
`alloy/src/tests/hit.rs`.

## The kinds half - settled and fixed (2026-08-21)

Non-View kinds used to default their geometry to `ctx.size` in `build()` (the
content box) while `is_in_bounds` received the border box: a padded `<rect>`
painted smaller than it hit. Text was worse - its words drew from the border
box origin while `place_atoms` inset its inline atoms by padding+border from
the same line layout, so on a padded text the words and the atoms were offset
against each other by exactly the inset; and `hit_run` looked the cached
layout up by border width while paint cached it at content width, so span hit
testing on a padded text found nothing (or, with an explicit width, a
stale measure-time layout at the wrong wrap).

Settled per kind:

- **Shapes (rect, oval, line, texture) default to the border box.** CSS
  backgrounds fill the border box, and it is what hit always measured - the
  hit side did not move, paint stopped shrinking.
- **Text sizes AND places against the content box** - wrap width (taffy
  measures against available space minus padding+border, so anything else
  diverges from layout) and origin both, which is what aligns words with
  inline atoms and matches CSS. `hit_run` takes the same box.

Both sides now derive the boxes from one place: `LayoutData::content_box()`
(border box inset by padding+border, origin included - the same inset
`place_atoms` uses, closing a latent padding-only-vs-padding+border mismatch
too). `BuildContext` and `HitContext` carry `size` (border box) and `content`
side by side; `cull::own_extent` mirrors the split.

One more member of the family fell out of verification: taffy passes
border-box `known_dimensions` to the leaf measure closure, but text wrapped
at that width raw - so a padded text with an explicit width measured its
height from a wrap wider than it painted. The closure in
`layout/context.rs` now insets `known` by padding+border before building the
`MeasureContext`.

Pinned by `padded_rect_hits_its_border_box` and
`content_box_insets_padding_and_border` in `alloy/src/tests/hit.rs`, and
`text_painted_extent_starts_at_the_content_origin` in
`alloy/src/tests/cull.rs`, next to
`view_box_fit_resolves_against_the_border_box_when_padded` from the View
round.

Visible change: a padded `<text>` now indents its lines by the padding
(previously they started at the border-box corner), matching CSS and its own
inline atoms.
