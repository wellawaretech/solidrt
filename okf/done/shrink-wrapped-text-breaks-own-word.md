---
title: Shrink-wrapped text must fit its own measure
description: A single word in a content-sized box sometimes wraps onto a second line inside itself ("Capsul / e"), because the width the box gets back can come out a float ulp under the max-content width the text measured, and the line breaker's fit test is an exact comparison.
created: 2026-10-08
---

# Shrink-wrapped text must fit its own measure

A `<text>` inside a content-sized parent should lay out on exactly the lines
its max-content measure promised. Some words do not: the box is sized to the
word, and the word still breaks inside itself, "Nets" as "Net / s",
"Capsule" as "Capsul / e". With `maxLines={1}` the last letter is clipped
instead ("Spher"). Which words break looks arbitrary: "Sphere", "Cone" and
"Torus" broke in one screen, their neighbours did not.

```tsx
<view flexDirection="row">
  <view paddingLeft={12} paddingRight={12}>
    <text fontSize={12} fontWeight={400}>Capsule</text>
  </view>
</view>
```

Seen on 0.0.68: the text measures 44.72 px wide, lays out on two lines and
reports 32.69 px tall. The two-line height means layout asked the text for
its height at a known width just under the 44.72 it measured.

The player shows it too, on the settings screen's About block: the
Capabilities card lists each flux capability as a pill (`CapabilityChip` in
[settings-panel.tsx](../../apps/player/src/parts/settings-panel.tsx)), a
padded content-sized `View` around a `Text` in a wrapping row, and some
capability names break inside their pill.

## Not part of it

- The split inside the word is `overflowWrap`'s default, "anywhere"
  (documented in core's types.d.ts): a unit wider than its line is split at
  grapheme boundaries. It is the visible symptom, not a second bug.
- The same word without `fontWeight` measured wider (45.41 px) on 0.0.68
  because the default weight was Medium then. Since 7c6acb8a the default is
  Regular, so the two now measure alike.

## Cause (confirmed 2026-10-08)

The width a shrink-wrapped text is laid out at is not the width it
measured. The test below reproduces it headless: of 180 word, size and
weight combinations laid out inside a padded, content-sized view through
taffy, 24 wrap inside the word. Comparing bits, the width taffy hands back
is exactly one float ulp off the max-content measure in every one of 40
sampled cases, half above and half below, and the ones that wrap are
exactly the ones an ulp below. Every word is a single wrap unit, so the
breaker's `pen + ink` and the measure are the same bits: the second
candidate (a different summation order) plays no part.

The fix is a named slack in the two places a width is compared against a
measured width: the unit fit in `layout_capped` and the ellipsis fit in
`trim_for_ellipsis` (`FIT_SLACK`, 1/64 px: a browser layout unit, far above
any ulp accumulation, far below a visible overhang). The balance cap stays
an exact comparison: it is derived from the same layout's own placed edges.
Rounding the measure up to a layout grid was the alternative; it would fix
the taffy path only and change every measured width, where the slack
changes nothing a measurement can see.

## Done when

- A text laid out at its own max-content width (or a width an ulp under it,
  as taffy hands back) never wraps or splits, at any font size, weight or
  padding.
- A test in `alloy/src/tests/text_layout.rs` that lays out a sweep of words
  at sizes and weights inside a padded, content-sized parent through taffy,
  and asserts one line each (`shrink_wrapped_text_fits_its_own_measure`;
  24 of 180 failed before the fix).
- Every capability pill on the player's About block sits on one line
  (read from the tree on a release client: "fs" and "process" were 42 px
  tall before, every chip is 21 px after).

## What it involves

Confirm with the test first, then make the fit test tolerant: compare with a
named slack (a fraction of a pixel, far below anything visible) in
`layout_wrap`, the ellipsis fit and anywhere else a width is compared
against a measured width. Alternatives to weigh: round the max-content
measure up to a layout grid, as browsers do with fixed-point layout units,
so a box is never narrower than its content in the first place.
