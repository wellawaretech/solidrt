---
title: A node whose box changes size snaps to it while its position slides
description: The layout slide covered position only, so a row that grew or shrank (an expanding card, a re-wrapped line, a column that widened) jumped to its new size while its neighbours glided; `layout` now covers the whole box, the meaning it has in every peer, with the children laid out against the painted box every frame of the motion.
created: 2026-09-10
completed: 2026-09-21
---

# A node whose box changes size snaps to it while its position slides

## Symptom

`layout` on an element (okf/done/transition-layout-animations.md) slides
it from the box it had to the box a layout gives it, position only. A card
that expands on tap, a text that re-wraps to more lines, a column that
widens when a sibling leaves: the node's neighbours glide to make room,
and the node itself snaps to its new size in one frame, so the reflow
still reads as a jump wherever size is what changed. Every peer's
`layout` covers size: Framer's `layout` (scale plus child correction),
Reanimated's `LinearTransition` (width and height on the native view),
SwiftUI's implicit animation and Compose's `animateBounds` (real layout
against the interpolated frame).

The related gap is size-driven sibling follow: Flutter's `AnimatedSize`
and Compose's `expandVertically`/`shrinkVertically` keep the neighbours
moving continuously because the leaving or arriving node's real size
animates. Layout size is not animatable here on purpose
(okf/done/native-transitions.md: it would relayout every frame), so an
exiting row pops out and its neighbours slide instead; a size lane is the
one place that idiom could land.

The paint-side workaround stops at clips and backdrops ([[quartz-heron]]
4c): an app can paint a card's background as a `d-rect` with transitioned
`w`/`h` fed from `onLayout`, but `backdropFilter` and the `clipRadius`
clip follow the layout box, so a frosted card's blur area jumps to its new
box with square-cut corners while its painted outline is still animating.
Whichever child model lands, the backdrop region and the clip must follow
the painted box, not the solved one.

## Cause

The slide lane is a point (`AnimValue::Point`, `Slide::at`): the diff at
`set_unrounded_layout` pushes a node only when its location moved, and
`Element::location` is the one painted-vs-solved seam. Size has no lane,
no state and no reader.

## Done looks like

`layout` keeps its spelling and covers the box: a declaring node whose
solved size changed animates from the size it was painted at to the new
one, alongside its position, on the same motion; a reflow mid-animation
retargets both. `onTransitionEnd` still fires once with `"layout"`.

## What landed

The lane is the painted box. `AnimValue::Box` (four lanes: x, y, width,
height) replaces the point, `Slide::at` is a rect, the diff at
`set_unrounded_layout` fires on a size change as on a move, and
`slide_remaining` reports offset and growth (`slide: { x, y, w, h }` in
the tree dump). `Element::painted_box` is the one accessor for the box a
node is drawn at, with `placement`, `painted_size`, `frame_size` and
`content_box` as its projections; the paint walk, the overflow clip and
`clipRadius`, the backdrop region, the design-size fit, culling, hit
testing, the snapshot raster and the bounding box all read through it, so
a frosted card's glass and corners follow the animated box (the
[[quartz-heron]] 4c case). `LayoutData::solved_box` is the solved box
alone, for the offsetLeft-style `layout_box` query. A size change in the
lane reports `Damage::Paint` (the node's own fill and clip redraw), a move
alone `Damage::Compose`, as before.

The child model is the real-layout one, SwiftUI's, Compose's, Flutter's
`AnimatedContainer`'s and a CSS width transition's, not Framer's. Framer
fakes the size with a scale transform and corrects the children because
the DOM cannot relayout a subtree cheaply mid-animation, and
`layout="position"` exists as the escape from the artifacts that causes;
owning layout has nothing to escape. `LayoutContext::animated_layouts`
(alloy/src/rendertree/layout/context.rs) runs after the taffy pass and the
slide diff: for every node whose painted size is off its solved one
(`RenderTree::resizing`, fed by the diff and the advance) it performs the
node's layout uncached with the painted size as the known dimensions in
ContentSize mode, so the children are laid out against the painted box
while the parent keeps placing the node at its solved one. Text re-wraps
per frame. The last run, at the solved size, lands the children on their
solved boxes; clearing the declaration mid-motion runs it once more.
Boxes written by a sub-layout are the animation, not a reflow: they start
no slide and drop the slide of any declaring node they place, so a
resizing ancestor carries its children as one motion (a SwiftUI
transaction) and a child's own slide resumes only on a reflow of its own.
Cost: one subtree layout per resizing node per frame, on the children's
own caches, for the length of the motion; text shaping is the expensive
part of that, and it is the price every peer pays.

Not added, on purpose: a position-only mode (`size: false`). It is
Framer's escape hatch from Framer's artifacts; SwiftUI and Compose have
no such flag. If a subtree ever turns out too expensive to relayout per
frame, the flag is additive on the entry.

Out of scope, a different feature: the Flutter `AnimatedSize` and Compose
`animateContentSize` idiom, where a node reports its painted size to its
parent so the siblings follow continuously. That is animation state
driving the parent's layout, and it conflicts with the per-node slide
diff (every sibling would re-diff each frame); it needs its own design if
a consumer asks for it.

Tests: alloy/src/tests/layout_slides.rs (grow, carried children, clear
mid-resize, hit test by painted size); the live recipe is
probes/layout-slide-probe.tsx `grow` under a frozen clock.

## Related

- okf/done/transition-layout-animations.md, the position half and the
  design this widens.
- Shared-element moves (`layoutId`) in okf/tiny.md build on the box lane:
  a node inheriting an exiting node's last box inherits its size.
