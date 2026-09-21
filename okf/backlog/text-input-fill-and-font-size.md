---
title: A TextInput cannot fill its space or set its font size
description: A multiline TextInput becomes a fixed scrolling box only with an explicit height, so filling a flex parent needs the undocumented flexGrow 1 plus height 0, and its font is always the theme body size; done means flexGrow alone fills and scrolls, and the field takes a font size like Text.
created: 2026-09-21
---

# A TextInput cannot fill its space or set its font size

## Symptom

Reported in [[quartz-heron]] items 10 and 11.

- A multiline `TextInput` with `flexGrow: 1` grows with its content and
  overflows its parent instead of filling it and scrolling. What works is
  `{ flexGrow: 1, height: 0 }`, which nothing documents.
- There is no per-field font size: the field is always the theme's body
  size times `policy.textScale`, and font fields in `layout` are ignored,
  unlike `Text`. An app whose text scales with its container cannot do it.

## Cause

- `viewportHeight()` in packages/components/src/editor-field.tsx treats
  the field as a fixed viewport only when `layout.height != null`;
  otherwise it sizes to its content. `height: 0` plus `flexGrow` works by
  accident of that test.
- `fontSize` in editor-field.tsx is `theme.text.body.size *
  policy.textScale` with no prop input.

## Done looks like

- The field is a viewport whenever its box is constrained by layout, not
  only by an explicit height: a multiline field with `flexGrow: 1` in a
  sized parent fills it and scrolls. One way is to measure the solved box
  (`onLayout`) instead of reading the declared height; the content-sized
  default stays for a field with no constraint. Document which layouts
  give which behavior in the text-input docs.
- `TextInput` takes font props the way `Text` does (the font fields in
  `layout`, per the layout-vs-style rule in components/src/types.ts), with
  row height, caret and scroll math following the chosen size;
  `textScale` still applies on top.
