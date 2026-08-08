---
type: backlog-item
title: Padding makes paint and hit size against different boxes
description: Paint hands every laid-out element its content box (border box minus its own padding) as ctx.size, while hit testing passes the border box. For a View's own matrices (transform center, viewBox fit) that made paint and hit disagree whenever padding combined with a transform or viewBox - settled and fixed 2026-08-08, border box on every path. For non-View kinds the divergence is still open, a padded rect paints its content box but hit-tests its border box.
status: partial
timestamp: 2026-08-08T00:00:00Z
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
matching what hit always passed. `ctx.size` stays the content box for kinds'
`build()`. Pinned by `view_box_fit_resolves_against_the_border_box_when_padded`
in `alloy/src/tests/hit.rs`.

## The kinds half - open

Non-View kinds default their geometry to `ctx.size` in `build()` (rect/oval
fill extent, line endpoints, texture quad, text wrap width) - the content box
- while their `is_in_bounds` receives the border box. A padded `<rect>`
paints smaller than it hits.

Needs a settlement, likely per-kind rather than one rule:

- Text wrap width MUST stay the content box: taffy measures text against
  available space minus padding, so a border-box wrap would diverge from
  layout.
- Shape/texture default extent could go either way: CSS backgrounds fill the
  border box (suggesting border box), but padding on a leaf shape is a
  marginal authoring pattern to begin with. Whichever wins, hit's
  `is_in_bounds` sizing must move to the same box as paint.

Padding on leaf kinds is rare enough that nothing has been reported; fix on
demand or when touching the hit/paint size plumbing anyway.
