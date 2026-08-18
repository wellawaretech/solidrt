---
title: Rich text editor
description: There is no way to edit styled text - TextInput edits a string, so bold/italic/links, inline atoms and paragraph attributes cannot be authored in-app; build a separate editor over the same buffer/geometry layers, starting with prepareText over styled runs so caret geometry knows about run boundaries.
created: 2026-08-18
---

# Rich text editor

## Symptom

An app that wants users to write formatted text (notes, messages with
emphasis and links, a document with headings and lists) has nothing to
build on: `TextInput` edits one string in one style. Rendering rich text
is solved (`<text>` with `<span>`, atoms, floats), authoring it is not.

## Why

`TextInput`'s value is a string by contract and should stay that; a rich
value is a document (text plus styled ranges, inline atoms, block
attributes), a different component. But everything under the skin is
shared and already exists: the offset-based selection and edit primitives
(`createTextBuffer`, packages/core/src/text-input.ts), the line/caret
geometry (`createTextEditorLayout`: lines, caret stops, offsetAtX, lineAtY,
scroll), focus and text-session plumbing, grapheme stepping, per-line
`d-text` drawing. What is missing is style awareness in the geometry, a
document model, and the component.

## Done looks like

A `RichTextEditor` component (own package, not `@solidrt/components`;
toolbar-less: the app renders controls and calls `toggleMark`, `setBlock`,
`insertAtom` on the editor) that edits a document value: styled runs
(weight, style, color, underline, links as a mark), inline atoms, block
attributes (heading level, list); selection, caret and scrolling as
`TextInput`; per-line rendering with `d-text` + `<span>` children.

## Steps

1. **`prepareText` over styled runs** (engine, alloy `prepare_units` + flux
   marshalling + flux-types). Today `prepareText` is single-style; `<text>`
   already shapes multi-run text through the same word cache, with a unit
   straddling two runs as glued pieces. Expose that: input is the plain
   text plus runs `{ start, end, style }`, output the same units with
   per-run metrics and caret stops that respect run boundaries (a caret
   stop at every run seam), line height from the tallest run. Independent
   of the rest, also what a decorated `TextInput` (display-only styling:
   mentions, syntax colors) would use.
2. **Document buffer** (core): Quill-Delta-like flat model, not a tree -
   one text string with `\n` between blocks, marks as attributed ranges
   over offsets, inline atoms as U+FFFC with attributes, block attributes
   per paragraph. Offsets stay string offsets, so `createTextBuffer`'s
   selection and edits carry over; toggling a mark is range arithmetic.
3. **Component**: lines from `createTextEditorLayout` fed with the runs,
   each line one `d-text` with `<span>` children for its slice of runs;
   selection highlight from the line stops. Inline atoms are the open
   engine question (atoms need laid-out children, which `d-text` cannot
   host today).

## Prerequisite

[TextInput range selection](text-input-selection.md): applying a mark
needs a range, and the highlight drawing is the same code.
