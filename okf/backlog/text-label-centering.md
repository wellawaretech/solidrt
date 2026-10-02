---
title: Center single-line labels by their cap band
description: Labels sit visibly low in their boxes at some font sizes (1.5px at a 15px base, 0 at 14px), because Impeller splits line leading in proportion to ascent and descent and the baseline snaps to whole pixels; a half-leading placement only moves the error around, so the candidate is cap-band centering, to be simulated before it is built.
created: 2026-09-27
---

# Center single-line labels by their cap band

In a 36px Button with a 15px base, the label's ink has 14px above and 11px
below: 1.5px low, with the layout itself centered (a 22px text box, 7px of
padding each side). The offset is inside the text box. Measured offscreen
(`sol render`, scale 1) on a stock Button, varying only the theme's font
family and base size; the offset is (space above the ink - space below) / 2,
positive is low:

| Base size | sans | mono | serif |
|---|---|---|---|
| 12px | 0 | | |
| 13px | +0.5 | | |
| 14px | 0 | 0 | 0 |
| 15px | +1.5 | +1.5 | +1.5 |
| 16px | +1.0 | | |
| 18px | +0.5 | | |

The family drops out because the three bundled fonts have identical vertical
metrics (UPM 1000, ascent 1069, descent 293, cap height 714, x-height 536,
USE_TYPO_METRICS). The stock themes happen to sit at 14px, which lands
centered; a theme at 15px is the worst case.

## Cause

1. **Proportional leading.** Text is shaped as an Impeller paragraph
   (alloy/src/rendertree/text/words.rs) with the line height applied through
   `set_height`, and the interop API (`impellers` 0.4.2 exposes only
   `set_height` on `ParagraphStyle`) gives no way to choose how the extra
   space splits. Flutter's default splits it in proportion to ascent and
   descent, so the baseline sits at L x A / (A + D), 0.785 L for Noto; the
   CSS half-leading model puts it at (L - (A + D) s) / 2 + A s. At a line
   height of 1.5 they differ by 0.039 s: 0.6px at 15px.
2. **Box rounding.** 15 x 1.5 is 22.5px, the text node's box is 22px; the
   half pixel comes off the bottom.
3. **Baseline snapping** dominates: glyphs draw on a whole-pixel baseline,
   so where the continuous offset lands decides whether the label reads 0,
   0.5, 1 or 1.5px low.

## Tested and rejected: half-leading placement

Simulated without touching the runtime (a look-alike Button whose label is
shifted by the proportional-minus-half-leading difference, after checking
the look-alike reproduces the stock Button exactly at every size):

| Base size | today | half-leading |
|---|---|---|
| 12px | 0 | 0 |
| 13px | +0.5 | -0.5 |
| 14px | 0 | -1.0 |
| 15px | +1.5 | +0.5 |
| 16px | +1.0 | 0 |
| 18px | +0.5 | -0.5 |

It fixes 15 and 16px, breaks 14px (the stock size) and flips 13 and 18px
high. Snapping dominates, so the errors move rather than vanish. Not worth
changing every app's text placement.

## Tested and rejected: cap-band centering

Simulated the same way (probes/label-centering-probe.tsx, 2026-09-27):
baseline at L/2 + (cap height x s)/2, as a fractional shift and as a
whole-pixel one, over 12-20px and all three families. The stock column
reproduces the table above exactly; the cap-band column, identical for
sans and mono, serif within 0.5px:

| Base size | today | cap band |
|---|---|---|
| 12px | 0 | -1.0 |
| 13px | +0.5 | -0.5 |
| 14px | 0 | -1.0 |
| 15px | +1.5 | +0.5 |
| 16px | +1.0 | 0 |
| 17px | +1.0 | 0 |
| 18px | +0.5 | -0.5 |
| 20px | +1.5 | +0.5 |

It moves every size up by about a pixel and leaves the spread at 1.5px
(-1.0 to +0.5), so it fails the 0.5px bar too. The bias comes from the
label's ascenders (the `l` and `t` of "Delete" stand above the cap
height), the spread from the snap: whichever continuous rule places the
baseline, rounding it to a whole pixel scatters the result by up to a
pixel across sizes.

## Next step

The snap is the lever, not the placement rule. Two candidates:

- **Snap toward the center.** Place the baseline continuously (any of the
  rules above), then choose the whole-pixel baseline whose resulting ink
  box is closest to centered in the line box, rather than the nearest
  pixel. It keeps text on the pixel grid and should bound the error at
  0.5px by construction; a per-line decision, so paragraphs are unaffected
  unless the rule is limited to single-line text.
- **A fractional baseline** (no snap) removes the scatter outright but
  costs stem crispness on low-DPI displays, which the light-on-dark work
  already fights for.

Simulate the first with the same probe (the look-alike's shift chosen per
size to center the ink), then build it as a runtime rule for single-line
text or as a text prop single-line labels opt into (CSS's
`text-box-trim`), set by Button, SegmentedControl, Badge and the like.

To measure: find each button by its fill color, then the first and last
pixel rows in its middle two thirds of columns that differ clearly from
the fill (a summed RGB difference over 150, so faint anti-aliased edge rows
do not count); the margins are the distances from those rows to the
button's top and bottom rows.

Side finding to check while there: `y` on a laid-out `<text>` did nothing
in the probe, while the same `y` on a wrapping `<view>` moved it. Either the
prop should be rejected there or it should apply.

Workaround until then: base sizes that round well (12 and 14 measured
centered). Luck of rounding, not a fix; another size or display scale can
drift.
