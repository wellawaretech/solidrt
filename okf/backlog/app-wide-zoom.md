---
type: backlog-item
title: App-wide zoom
description: Browser-style whole-UI zoom (pinch, ctrl+wheel) as a root-level runtime affordance that re-lays out at scale instead of magnifying raster output, needing no app cooperation.
status: deferred
timestamp: 2026-07-23T00:00:00Z
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
  presses below it. This is also the expected motivation for promoting
  the recognizer core out of the components package into core
  (runtime-owned, framework-independent).

Prerequisites: component-gestures arena (stage 3) plus its platform
items (multi-touch device verification, pinch input per platform).
