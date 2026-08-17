---
title: Default font weight should follow display scale
description: Text defaults to Medium so that small type stays readable on 1x desktop displays, which over-thickens every label on the 2-3x phone screens that never needed it.
created: 2026-08-14
---

# Default font weight should follow display scale

The default is `FontWeight::Medium`
([alloy/src/rendertree/kinds/text.rs:52](../../alloy/src/rendertree/kinds/text.rs)),
chosen for the worst case: a 1x desktop monitor, where roughly 14px and below
renders as hairlines that are hard to read on dark backgrounds. Impeller
antialiases text in grayscale only, with no subpixel rendering, so the thin
antialiased edge bleeds into the background instead of being reconstructed by
the RGB stripe the way ClearType-style rendering would.

Mobile never had the problem. A device pixel ratio of 2-3x rasterizes the same
logical size at 2-3x the physical pixels, and strokes come out visibly thicker
regardless of weight. So the workaround for desktop is paid on every phone,
where it makes the default type heavier than intended.

## Fix

Select the default weight from the display scale at text build time: `Regular`
when `display_scale >= 2.0`, heavier below. The scale is already available to
the rendertree - `platform.display_scale()`
([alloy/src/rendertree/platform.rs:151](../../alloy/src/rendertree/platform.rs)) -
and is already read at build/composite time elsewhere, so this is a read at
build, not new plumbing.

Two things to settle while doing it:

- `Text::default()` runs before any platform context is in hand, so the choice
  has to happen where the paragraph is built (or the default becomes "unset"
  and resolves later). The `ParaKey` fingerprint that guards the paragraph
  cache must include the resolved weight, or a display-scale change on a window
  moved between monitors will not re-lay-out the text.
- an explicit `fontWeight` prop keeps overriding, unchanged.

Source: root KNOWN_ISSUES.md, migrated 2026-08-14.

Longer term the workaround goes away with an own rasterizer behind the
shaper trait ([text-own-rasterizer](text-own-rasterizer.md), grown out of
[text-layout-owned](../done/text-layout-owned.md)), which is where
gamma or contrast compensation for light-on-dark text can actually be done.
