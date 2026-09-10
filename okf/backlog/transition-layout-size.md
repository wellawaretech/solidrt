---
title: A node whose box changes size snaps to it while its position slides
description: The layout slide covers position only, so a row that grows or shrinks (an expanding card, a re-wrapped line, a column that widens) jumps to its new size while its neighbours glide; done means `layout` covers the whole box, the meaning it has in every peer, on the child model that owning layout makes a choice rather than a constraint.
created: 2026-09-10
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

## What it involves

The lane widens from a point to a box (four lanes, the color lane count),
`Slide::at` becomes a painted rect, the diff pushes on a size change too,
and `slide_remaining` reports width and height beside x and y.

The open question is the child model, and owning layout makes it a choice:

- SwiftUI's: the children lay out against the interpolated box every
  frame, correct at every frame, at the cost of a real layout of the
  subtree per frame of the animation (the per-node cache keeps it to the
  animating subtree).
- Framer's: the children lay out against the final box and the painted
  box clips (or scales) them during the animation; cheap, and the source
  of Framer's scale-correction artifacts.

A position-only mode (Framer's `layout="position"`) lands with this, as a
field on the entry, for the cases where size should snap.

## Related

- okf/done/transition-layout-animations.md, the position half and the
  design this widens.
- Shared-element moves (`layoutId`) in okf/tiny.md need the box lane
  first: a node inheriting an exiting node's last box inherits its size.
