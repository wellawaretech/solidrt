---
title: What "something like stylesheets" already means here
description: A plain object spread into props is the answer to most of what the stylesheet question asks, and the constraint on anything more is fixed - no cascade, no selectors, or the reactive seam and repaint-boundary model stop holding.
created: 2026-08-13
---

# What "something like stylesheets" already means here

The question comes up ("could we have something like stylesheets?") and most of
what it usually means already works, with no feature at all. A plain object
spread into props is React Native's `StyleSheet.create` minus the ceremony:

```tsx
const CARD = { padding: 16, gap: 8, flexDirection: "column" } as const

<view {...CARD}>...</view>
```

Reusable, composable, typed, and it needs nothing from the runtime. Teach this
as the answer rather than implying there is none.

**The constraint on anything beyond it: no cascade and no selectors.** Whatever
lands has to stay per-element property writes, or the reactive seam - one signal
read subscribes one native property - and the repaint-boundary model stop
holding. That rules out most of what "stylesheets" implies, and is why the
question deserves a considered answer rather than a port of CSS.

Two things genuinely missing, both scoped by that constraint:
[scoped-text-defaults](../backlog/scoped-text-defaults.md) and
[state-variant-selection](../backlog/state-variant-selection.md).

Extracted 2026-08-13 from a deferred item that mixed this answer with the two
gaps. Deliberately not flagged on the website.
