---
title: App-wide zoom
description: Browser-style whole-UI zoom (pinch, ctrl+wheel) as a root-level runtime affordance that re-lays out at scale instead of magnifying raster output, needing no app cooperation.
created: 2026-07-27
---

# App-wide zoom

Browsers let the user pinch-zoom any page (visual viewport zoom) unless
a site actively disables it; native mobile apps almost never offer it -
pinch is per-view (photos, maps) and the rest of the app is fixed
scale. Apps should be more powerful than the web here, not more
limited. No app framework provides this because none of them own the
compositor; we do.

Idea: a runtime-owned root-level zoom affordance, no app cooperation
needed.

- Input: pinch on touch; ctrl+wheel on desktop (the established
  convention; `WheelEvent` already carries `ctrlKey`).
- Unlike the web's raster magnification, we can zoom by re-laying-out
  at a larger scale - essentially a live UI-scale/textScale slider,
  plugging into the existing env -> policy scaling machinery of the
  design system.
- Architecture: a root-level recognizer sitting on the gesture arena of
  okf/plans/component-gestures.md, arbitrating with app-level pans and
  presses below it.

Prerequisites: mostly met as of 2026-08-09. The recognizer core was
promoted out of components into core (the promotion this item predicted
- @solidrt/3d's pinch-to-zoom got there first): the arena is exported
from @solidrt/core, and createTransform is the merged pan/pinch/rotate
recognizer a root zoom would build on. Multi-touch is verified in
practice (the trails example paints with multiple fingers). Remaining:
the trackpad-pinch survey (desktop pinch likely arrives as ctrl+wheel,
which unifies with the desktop convention and needs no recognizer) and
the root-recognizer design itself (where it attaches, how it feeds the
env -> policy scaling machinery).
