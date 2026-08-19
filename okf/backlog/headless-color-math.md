---
title: Color math is unreachable headless
description: parseColor/mixColors/brightness live only on flux:rendertree (gui feature), so site tooling, tests, and theme builders cannot call them; the components theme presets hardcode precomputed mix results as a workaround.
created: 2026-08-19
---

# Color math is unreachable headless

`@solidrt/core/color` (parseColor, mixColors, brightness) forwards to
`flux:rendertree`, which only exists in gui builds of flux. Any headless
consumer - the website token build, unit tests, a theme-building script -
throws at import resolution. The Rust implementation (`alloy/src/color.rs`:
CSS grammar via csscolorparser, oklab mix, YIQ brightness) has no GUI
dependency; only its binding placement does.

Workaround in place: `packages/components/src/theme.ts` precomputes its two
`textMuted` oklab mixes as literals (formula in a comment) so the theme module
stays importable headless. That is drift-prone by construction - edit the
inputs, remember to recompute - and every future headless color need pays the
same tax.

Done looks like: the color functions callable from a plain flux binary, one
Rust owner unchanged. Shapes, roughly in order of fit:

- Expose the color slice on a module that is not gui-gated (the functions are
  three pure string/number bindings; alloy's color module compiles without
  the rest of alloy today - check feature boundaries before assuming).
- Or compile just these bindings into non-gui flux behind the existing
  `flux:rendertree` name resolving to a color-only module when gui is off -
  keeps the import path stable for core/color.ts.

Non-goal: a JS reimplementation. One owner for the CSS grammar and the
perceptual math is the point (okf/done/css-colors-in-rust.md reasoning);
duplicating oklab in TS reintroduces the drift the Rust move removed.

When this lands, replace the precomputed textMuted literals in
packages/components/src/theme.ts with live mixColors calls again.
