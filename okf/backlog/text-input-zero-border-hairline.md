---
title: A TextInput with borderWidth 0 still draws a hairline border
description: style borderWidth 0 leaves a faint outline in the theme border colour, visible on coloured cards; done means a zero width draws no border, on TextInput and on any other element whose stroke width can be 0.
created: 2026-09-21
---

# A TextInput with borderWidth 0 still draws a hairline border

## Symptom

`<TextInput style={{ borderWidth: 0 }}>` still shows a thin outline in the
theme border colour: a visible line on a coloured card, and very clear
when the field's box snaps to a new size under an animated background.
Workaround: `borderColor: "transparent"`, which also turns off the focus
ring. Reported in [[quartz-heron]] item 4a.

## Cause (probable)

editor-field.tsx passes `props.style?.borderWidth ?? ...` straight through
as `strokeWidth`, so 0 reaches the renderer as a zero-width stroke. In
Skia's model, which Impeller follows in places, stroke width 0 means a
one-device-pixel hairline, not "no stroke". Unverified: confirm with a
snapshot of a lone `d-rect`/view with `strokeWidth={0}`.

## Done looks like

A zero stroke width draws nothing. Decide the layer: the rendertree's
stroke path skips the stroke at width 0 (fixes every element, and matches
CSS where `border-width: 0` is no border), rather than EditorField
special-casing it. Check whether anything relies on 0 as a hairline.
