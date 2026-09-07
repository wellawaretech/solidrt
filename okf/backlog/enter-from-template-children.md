---
title: Enter animations (from) fire only on template roots
description: A transition entry's `from` runs at insert, but JSX inserts a template's children before the effect that writes their props, so a child's transition config is not there yet when it attaches and its enter animation never plays; only the template root (inserted after its props effect) gets one. Done means a `from` on any element plays on its first frame regardless of where it sits in the template.
created: 2026-09-07
---

# Enter animations (from) fire only on template roots

## Symptom

`apply_enter_transitions` (alloy/src/rendertree/tree/transitions.rs) runs
at `insert_node`, reading the node's transition config and the mounted
value. Solid's universal codegen builds a template as create-all,
insert-children, then one effect that writes every dynamic prop, and only
then returns the root for its parent to insert. So:

- the template ROOT attaches after its props effect ran: config and mounted
  values are in place, `from` plays. Every `from` the components package
  ships sits on a root (popupFade on a modal surface, markMotion on a
  `<Show>` wrapper), which is why it works today.
- a template CHILD (`<view><d-rect transition={{ opacity: { from: 0 } }} /></view>`)
  attaches with no config: `entered` is set, the config lands a moment
  later through the property path, and nothing runs. The element simply
  appears at its mounted value.

The mount-time write guard is not the cause (writes before the first paint
snap by design; an in-flight enter track is retargeted, not cancelled), but
it is the reason the enter pass cannot simply be re-run when the config
lands: the mounted value has not been written yet at that point either, so
there is no target to animate toward.

## Done

A `from` entry plays on the element's first painted frame wherever the
element sits in its template, animating from `from` to the value the
element holds at that first paint.

## Involves

Defer the enter pass from insert time to the first paint (the walk stamps
`Element::painted`; the frame's advance, or the walk itself, can start the
tracks for nodes entered this frame that declare `from`), or run it when a
config with `from` lands on an attached, unpainted node and let the
retarget rule carry the later mounted-value write. Either way the test
`enter_from_animates_first_attach_only` ("attach snaps to from" right after
insert) moves to a first-frame assertion. Stagger delays index per frame
already, so a deferred pass keeps the cascade.
