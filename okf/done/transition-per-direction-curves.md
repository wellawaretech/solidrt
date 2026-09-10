---
title: An exit plays the enter's curve backwards
description: A transition entry carries one curve and serves the property both ways, so an ease-out enter runs its exit with all the motion in the first frames; done means `exit` (and `from`) can carry their own curve, duration and delay, the shape every peer uses.
created: 2026-09-10
---

# An exit plays the enter's curve backwards

## Symptom

`ease-out` is the natural enter: fast start, gentle landing. Played on the
way out it spends the whole fade in the first few frames and then crawls,
which is the opposite of what a leaving element should do. The stagger
example's `FADE` entry (packages/core/examples/stagger.tsx) is written that
way because there is no other way to write it: `from` and `exit` are per
entry, the curve is per entry too, and one entry serves both directions.
`ease-in-out` reads acceptably both ways and is a compromise in each.

Every peer separates the directions. Framer Motion puts a `transition`
inside each variant, so `exit: { opacity: 0, transition: { ease: "easeIn" } }`
is the idiom. Reanimated's `entering` and `exiting` each carry their own
easing and delay. Vue and Svelte use separate enter and leave classes or
`in:`/`out:` directives. SwiftUI has `.transition(.asymmetric(insertion:removal:))`.
Ours is the odd shape.

The workaround, `transition={{ opacity: closing() ? OUT : IN }}`, works now
that enter animations survive a late config
(okf/done/enter-from-template-children.md) but needs a signal that knows
the panel is closing, which is the state the exit mechanism exists to
remove.

## Cause

`TransitionEntry` holds one `spec` plus `delay_ms`, and `from`/`exit` are
bare values. `start_exit_tracks` and `apply_enter_transitions`
(alloy/src/rendertree/tree/transitions.rs) both use the entry's spec.

## Done looks like

`exit` and `from` accept either the value they take today or an object that
overrides the entry's motion for that direction:

```tsx
const FADE = {
  duration: 350, curve: "ease-out",
  from: 0,
  exit: { value: 0, curve: "ease-in", duration: 200 },
} satisfies Transition
```

Any field left out falls back to the entry (`delay` included, so the
stagger example needs no change). The same object form on `from` keeps the
two directions symmetric. Springs take `bounce`/`duration` the same way.

## What it involves

`TransitionEntry` gains an optional spec and delay per direction (an
`exit_spec: Option<TransitionSpec>` and `exit_delay_ms: Option<f32>`, and
the `from_` pair). The property decoder in
flux/src/alloy_plugins/properties/transition.rs parses the object form,
reusing the existing shorthand parsing for the spec fields. The two track
starters pick the direction's spec when present. `types.d.ts` documents the
union on both `TransitionSpring` and `TransitionTween`, and the stagger
example switches its fade to an `ease-in` exit, which is the visible proof.

Observable from the MCP, as part of done: the tree dump with `props` reports
the property mid-flight, so freezing the clock, removing the panel and
stepping a frame at a time reads the exit's curve directly (an `ease-in`
exit moves little in the first steps and most at the end, the reverse of
the entry's curve on the way in). The dump should also name the spec in
force for an exiting node, so a wrong override is a one-line read rather
than a curve-fitting exercise on stepped values.

## Findings

Built as `Endpoint { value, spec, delay_ms }` on `from`/`exit`, fully
resolved by the decoder rather than as optional overrides the runtime falls
back through: the merge happens once, at the raw fields
(`decode_endpoint` in flux/src/alloy_plugins/properties/transition.rs), and
the two track starters read the endpoint's motion and nothing else. The one
rule worth knowing is the kind: naming a `curve` or a `bounce` on an
endpoint decides tween or spring outright, with the entry's duration when
none is given, so an ease-in exit on a spring entry is not a clash.

Named curves moved into alloy (`Curve::named`/`name` in
alloy/src/motion.rs) so a spec can print itself the way it was declared;
`TransitionSpec`'s Display recovers a spring's perceptual pair from
omega/zeta. The tree dump (lattice `node_json`) uses it: `exiting: true` on
the unmounted root and `exit: { <prop>: "200ms ease-in delay 70ms" }` on
every node of the cascade, which also closes the tiny.md item on `exiting`.

## Related

- Reverse stagger. Framer has `staggerDirection: -1` per variant and GSAP
  `stagger: { from: "end" }`, so a list can enter first-to-last and leave
  last-to-first. Ours cascades exits in tree order only. If it comes, it is
  a field beside `stagger` (`staggerExit: "reverse"` or an object form for
  `stagger`), not part of this item.
- transition-layout-animations.md, transition-delay-catch-up.md: the other
  gaps against the same peers.
