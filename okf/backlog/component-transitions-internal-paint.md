---
title: Component transitions cannot reach a control's internal paint
description: The components forward `transition` to their root view and to the background/border rects drawn for `style`, but a control's own parts - the Switch knob, Slider thumb, Checkbox mark, Radio dot, ProgressBar fill, Spinner, Icon paths, and the chrome of NavShell/ContextMenu/Field - are not addressable, so the motion users most want on a control (the knob sliding) still snaps.
tags: [components, transitions, animation]
created: 2026-08-22
---

# Component transitions cannot reach a control's internal paint

## Symptom

`<Switch transition="200ms ease-out" />` animates the switch's opacity or
transform, but the knob still jumps from one side to the other. The same
for a Slider thumb, a ProgressBar fill, a Checkbox mark appearing, a Radio
dot, the SegmentedControl's active segment. The properties that move are on
nodes the component builds for itself (d-oval `x`, d-rect `w`, d-path
opacity), and nothing in the `transition` vocabulary names them.

## Where it stands (2026-08-22)

`packages/components/src/types.ts` defines the component-level declaration
(`TransitionProps`, `splitTransition`, `transitionEndFor`): view-level names
go to the root, `backgroundColor`/`borderColor`/`borderWidth`/`borderRadius`
to the style rects, `Text` routes `color` to its text node, `ScrollView`
routes `scrollX`/`scrollY` to its viewport. Core's own names are rejected by
the types so nothing is silently dropped. The controls listed above take
the view-level entries only and say so in docs/types.md.

## What done looks like

Each control names its moving parts in its own `TransitionProps` parameter
and routes them, the way ScrollView does for scroll: Switch `knob` (the
oval's `x`) and `track` (color), Slider `thumb`/`fill`, ProgressBar `fill`
(rect `w`), Checkbox `mark` (path opacity/scale), Radio `dot`,
SegmentedControl `indicator`. Plus one decision to make first: whether a
control should carry a DEFAULT transition from the theme (a Switch that
never animates its knob is a worse Switch), with the prop overriding it,
which would move most of the value into `theme.components`.

## What it involves

- Per control: a name-to-node table next to its render, a `split` like
  ScrollView's, and the end-event name mapping.
- A shared shape for "named part" declarations so the per-control tables
  stay one-liners; likely a generalization of `splitTransition` taking a
  `{ partName: { node, coreProp } }` map.
- Theme defaults: a `transition` slot per control in `theme.components`,
  applied when the prop is absent.
- Respect the OS "reduce motion" preference. Nothing in the stack reads it
  today (no counterpart to `systemTheme()` in core/environment.ts). It
  belongs at the same level as the theme default: alloy reports the fact
  (SDL has no API for it, so per platform: GTK/portal setting on Linux,
  `NSWorkspace.accessibilityDisplayShouldReduceMotion` on macOS,
  `SPI_GETCLIENTAREAANIMATION` on Windows, `Settings.Global
  ANIMATOR_DURATION_SCALE` on Android, `UIAccessibility
  .isReduceMotionEnabled` on iOS), core exposes it next to `systemTheme`,
  and the component layer drops its default transitions to snaps (or a
  plain short fade) when it is set. App-declared transitions are the app's
  business, but the components should give them a way to honor it too -
  likely a single `policy.motion` switch like `policy.focusRing`.
- Note that the rects for `style` are only mounted when the style sets them
  (`hasBackground`), so a transition on `backgroundColor` with no
  background set animates nothing; document or mount-on-declare.
