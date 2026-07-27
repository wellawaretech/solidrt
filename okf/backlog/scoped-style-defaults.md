---
type: backlog-item
title: Scoped style defaults and variant selection
description: "The two real gaps behind \"something like stylesheets\": no scoped text defaults and no state/variant selection, both constrained to stay per-element property writes."
status: deferred
timestamp: 2026-07-26T00:00:00Z
---

# Scoped style defaults and variant selection

"Could we have something like stylesheets?" came up 2026-07-26 while
writing the website's Core concepts. Most of what that question usually
means already works, so this note is mainly about naming the two parts that
do not. Deliberately NOT flagged on the website; this file is the record.

## What already works, with no feature at all

A plain object spread into props is React Native's `StyleSheet.create`
minus the ceremony:

```tsx
const CARD = { padding: 16, gap: 8, flexDirection: "column" } as const

<view {...CARD}>...</view>
```

Reusable, composable, typed, and it needs nothing from the runtime. The
concept page should teach this as the answer rather than implying we have
none.

## Gap 1: no scoped defaults (no inheritance)

Nothing can say "everything in this subtree defaults to this text color and
font". Every `<text>` carries its own props, because the rendertree's model
is flat per-element property writes with no cascade.

`@solidrt/components` works around it by routing every `Text` through the
theme store, which is why an app that drops to a raw `<text>` has to repeat
the color, and why a component from somewhere else cannot pick up the app's
text style.

Shapes worth weighing:

- A context-provided default for a fixed, small set of text properties,
  read by `<text>` at build time. Opt-in, JS-side, no rendertree change.
- Real inherited properties in the rendertree. Expensive: inheritance has
  to resolve somewhere, and the flat property-write model plus repaint
  boundaries are exactly what makes updates cheap. This is the option that
  looks most like CSS and fits us least.

## Gap 2: no state or variant selection

Hover, pressed, disabled and size variants are hand-wired signals in every
widget. `Button` picks fill/hover/label with a `switch` over its variant and
derives the background from press state by hand; every other widget repeats
the pattern.

A helper that selects a prop bundle from state is pure userland and
probably belongs in `@solidrt/components` rather than core. Worth doing
only once the same shape has been written three or four more times, so the
abstraction is derived from real repetition.

## The constraint on any answer

No cascade and no selectors. Whatever lands has to stay per-element
property writes, or the reactive seam (one signal read subscribes one
native property) and the repaint-boundary model stop holding. That rules
out most of what "stylesheets" implies, and is the reason the question
deserves a considered answer rather than a port of CSS.

## Why deferred

The spread covers the common case; gap 1 touches the rendertree's property
model, which is load-bearing; gap 2 wants more evidence first.