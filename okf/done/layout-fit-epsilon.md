---
title: One fit slack for every measured-size comparison
description: The inventory and fix that followed the shrink-wrapped text bug: every "does this fit" decision between two derived layout floats now compares with one named slack, in the Rust breaker and the three JS breakers, and the sites that need none are listed with why.
created: 2026-10-08
---

# One fit slack for every measured-size comparison

okf/done/shrink-wrapped-text-breaks-own-word.md fixed one instance of a
class: a size derived from a measure through someone else's arithmetic,
compared exactly to make a discrete decision. An ulp on a painted width is
nothing; an ulp at a yes/no threshold flips the answer, and which inputs
flip it depends on their bits, so it reads as arbitrary. This item made
the slack one shared value and walked every site of the class.

## Sites that compare and now share the slack

- `rendertree::layout::FIT_SLACK` (1/64 px, a browser layout unit) is the
  rendertree's one tolerance; the text breaker's unit fit and ellipsis fit
  use it ([layout.rs](../../alloy/src/rendertree/text/layout.rs)).
- core's `layoutNextLine`, the public JS breaker, carries `FIT_SLACK` with
  the same value ([core.ts](../../packages/core/src/core.ts)); the text
  input's `splitWide` (a unit wider than the line) uses it.
- 2d's sprite-font breaker is its own copy of `layoutNextLine` (the package
  does not depend on core) and carries the same constant
  ([text-layout.ts](../../packages/2d/src/text-layout.ts)); its test pins
  that a width a hundredth under the ink still fits and a tenth breaks.

## Sites checked and left exact, with why

- The breaker's balance cap: a strict less-than against edges from the same
  layout's own placed runs, the same bits by construction.
- The hit test's overflow clip (`box_local.x >= size.width`): a pointer on
  the exact edge, no measure involved, nothing visible either way.
- The scroll range (`content - viewport`): an ulp of range feeds only the
  fling clamp, scrolling by a millionth of a pixel; no indicator or enable
  reads it.
- Menu, select and tooltip placement ("fits below, else above"): both
  outcomes are valid placements at an exact fit.
- Pixel snapping: a value on a half-pixel boundary rounds either way by an
  ulp. Inherent to rounding, not fixable by a tolerance (it only moves the
  boundary); both edges of a shared boundary go through one grid map.
- taffy's own flex-wrap line fit has the bug, confirmed (3 of 165
  content-sized wrapping rows of text): upstream, see
  okf/upstream/taffy-flex-wrap-exact-line-fit.md.

## Trail

Every rendertree comparison between two derived floats was grepped
(`<=`, `<`, `>`, `>=` on widths, heights, extents, inks and pens), the JS
components and core for the same, and each site classified as above. The
fixed-point alternative (layout units everywhere, as browsers do) was
weighed and declined: a rewrite of every width in the layout path for a
property one tolerance restores.
