---
title: Component transitions cannot reach a control's internal paint
description: The components forwarded `transition` only to their root view and style rects, so a control's own parts (Switch knob, Checkbox mark, ...) snapped. Fixed by built-in default motion (motion.tsx) driven by theme.motion and gated on policy.motion, plus named-part routing (knob/indicator/fill) through partTransition.
tags: [components, transitions, animation]
created: 2026-08-22
---

# Component transitions cannot reach a control's internal paint

## Symptom (as filed)

`<Switch transition="200ms ease-out" />` animated the switch's opacity or
transform, but the knob jumped from one side to the other; same for the
Checkbox mark, Radio dot, ProgressBar fill, SegmentedControl active segment.
The moving properties were on nodes the component builds for itself, and
nothing in the `transition` vocabulary named them.

## What was done (2026-09-02)

Went further than the filed shape: instead of only making parts addressable,
the components now ship default motion so they animate without any props.

- `theme.motion` timing tokens (`fast` 100 / `base` 150 / `slow` 250 ms) and
  `packages/components/src/motion.tsx`: every built-in transition draws its
  spec from these helpers, gated on `policy.motion` ("reduced" keeps the
  color/opacity fades, snaps travel/scale; "none" snaps all).
- Defaults everywhere: themed colors fade (which makes `setTheme` a
  cross-fade of the whole UI), presses shrink the free-standing controls on
  a quick spring (`pressScale`/`scaleFeedback`) and fade the overlay tints
  (the shared `PressFeedback` rect), marks pop in/out via `from`/`exit`
  (`markMotion`), parts travel on springs, popups (Modal, Tooltip, Select,
  ContextMenu) fade in/out (`popupFade`/`popupFadeOut`).
- Named parts, routed by `partTransition`/`partTransitionEnd` (types.ts) and
  excluded from the root split: Switch `knob` (x), SegmentedControl
  `indicator` (x; the active segment became one sliding rect under the
  labels, placed from measured segment boxes), ProgressBar `fill` (w,
  determinate only - indeterminate writes per frame). A caller entry (or
  `all`/shorthand) retimes a part; `transition={null}` suppresses all
  built-ins; `withTransitionDefaults` merges defaults under caller entries.

Deliberate non-goals: Slider parts (thumb/fill track the drag 1:1; a
transition would rubber-band it), Checkbox `mark` / Radio `dot` as caller
vocabulary (their enter/exit is built-in; retime via `theme.motion`),
per-control transition slots in `theme.components` (the shared timing scale
covered the need).

Trap for later work: an overlay's hidden state must be the tint at alpha 0
(`withAlpha(tint, 0)`), never `"transparent"` - fading from transparent
black darkens a dark scheme's white tint midway.

## Follow-up

The OS reduce-motion preference is still not read anywhere; the components
only honor a manually set `policy.motion`. Split out to
okf/backlog/reduce-motion-preference.md.
