---
title: Nothing can set a default text style for a subtree
description: Every <text> carries its own props because the rendertree is flat per-element writes with no cascade, so an app dropping to a raw <text> repeats the color and a component from elsewhere cannot pick up the app's text style.
created: 2026-07-26
---

# Nothing can set a default text style for a subtree

What it looks like when you hit it: you set your app's text color in the theme,
drop to a raw `<text>` for one label, and it comes out unstyled. Or you pull in
a component from somewhere else and it cannot pick up the app's text style at
all.

`@solidrt/components` works around it by routing every `Text` through the theme
store, which is exactly why the raw element diverges.

Two shapes worth weighing:

- **A context-provided default for a fixed, small set of text properties**, read
  by `<text>` at build time. Opt-in, JS-side, no rendertree change.
- **Real inherited properties in the rendertree.** Expensive: inheritance has to
  resolve somewhere, and the flat property-write model plus repaint boundaries
  are exactly what makes updates cheap. This is the option that looks most like
  CSS and fits us least.

Whatever lands must stay per-element property writes - see
[what "something like stylesheets" already means](../notes/style-reuse-without-stylesheets.md)
for the constraint and for what already works today. Split from that item when
okf was restructured; the other half is
[state-variant-selection](state-variant-selection.md).

Not to be confused with the intra-paragraph cascade in
[text-inline-spans](../done/text-inline-spans.md), which resolves span overrides in
Rust at shape time and does not inherit across the tree.
