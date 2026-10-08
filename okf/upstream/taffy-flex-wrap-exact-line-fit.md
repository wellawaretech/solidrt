---
title: taffy flex-wrap line fit is exact, so a content-sized wrapping row wraps its last item
description: A wrapping flex row sized to its own max-content, through a padded parent, gets an inner width a float ulp under the sum of its items, and the line collection's exact `line_length > main_axis_available_space` then pushes the last item onto a second line.
project: taffy (github.com/DioxusLabs/taffy)
versions: taffy 0.14.0
status: unfiled
link:
created: 2026-10-08
---

# taffy: flex-wrap line fit is exact, so a content-sized wrapping row wraps its last item

The same arithmetic drift as okf/done/shrink-wrapped-text-breaks-own-word.md,
one level up, inside taffy. A `flex_wrap: Wrap` row with no width of its own
inside a padded, content-sized parent (itself an item of a row) measures its
max-content as the sum of its items. The row's final inner width is that sum
re-derived through the parent's border box (plus padding, minus padding
again, through the flex free-space of the near-zero difference) and lands a
float ulp under it. Line collection (`compute/flexbox.rs`, 0.14.0 line 1155)
then runs

```rust
line_length > main_axis_available_space && idx != 0
```

exactly, and the last item fails by an ulp and starts a second line. The
row is two lines tall where its own measure promised one.

Observed 2026-10-08 through alloy's taffy adapter with leaf children
measured at fractional widths (shaped text): of 165 content-sized wrapping
rows of 2 to 12 words at 5 font sizes and 3 weights, 3 wrapped their last
word. Which rows depends on the bits of the item widths, so it looks
arbitrary. Children with whole-pixel widths never show it, since their sums
are exact.

Not worked around on our side: the comparison is inside taffy's container
algorithm, which our adapter calls with the inputs taffy hands it. Our own
text breaker carries a fit slack (`rendertree::layout::FIT_SLACK`) for the
same reason; the equivalent in taffy is a tolerance in the line fit, or
deriving the container's max-content from the same `line_length` sum so
the two agree bit for bit.

## Draft report

A `flex_wrap: Wrap` container with no definite width, sized by its content
inside a padded parent, wraps its last item onto a new line when the items
have fractional widths. The container's max-content width is the sum of the
items; after the parent's padding is added and removed again the inner
width comes back one f32 ulp under that sum, and `collect_flex_lines`
compares `line_length > main_axis_available_space` exactly. Repro: a row
root holding a padded view (padding 12) holding the wrapping row, with
leaf children whose measure returns widths like 44.724003 and 28.847002;
a few percent of such rows end up two lines tall. A tolerance in the line
fit (a fraction of a pixel, as browsers' fixed-point layout units give
them) or computing the max-content size with the same accumulation as the
line length would fix it.
