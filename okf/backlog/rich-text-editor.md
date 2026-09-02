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

A `RichTextEditor` component (in `@solidrt/components`, next to `TextInput`;
toolbar-less: the app renders controls and calls `toggleMark`, `setBlock`,
`insertAtom` on the editor) that edits a document value: styled runs
(weight, style, color, underline, links as a mark), inline atoms, block
attributes (heading level, list); selection, caret and scrolling as
`TextInput`; per-line rendering with `d-text` + `<span>` children.

## Steps

1. **`prepareText` over styled runs** - DONE 2026-08-19 (uncommitted).
   `prepareText(text, { ...font, runs: [{ start, end, fontFamily?,
   fontSize?, fontStyle?, fontWeight?, lineHeight? }] })`, JS offsets,
   sorted and disjoint (throws otherwise), gaps in the base font. A wrap
   unit crossing a run boundary comes back as one `TextUnit` per run with
   `glue: true` on the continuation pieces and `run` = range index (mirrors
   alloy's `Run.glue`), so caret stops fall on every seam and `lineHeight`
   is per unit; `layoutNextLine` never breaks before a glued unit; the
   editor layout's `splitWide` carries `glue`/`run`. alloy
   `prepare_units(platform, text, style, runs: &[PreparedRun], carets)`,
   marshalling in flux `prepared_runs`. Independent of the rest, also
   what a decorated `TextInput` (display-only styling: mentions, syntax
   colors) would use.
2. **Document buffer** (components) - DONE 2026-08-19 (uncommitted).
   `packages/components/src/rich-text-document.ts` (exported from the package index; core only gains the `onReplace` hook, the mechanism): `Document = { text, runs: {start, end,
   attributes}[], blocks: Attributes[] }` (Delta-like, flat; block
   attributes in a parallel per-paragraph array rather than on the `\n`,
   so `text` needs no trailing newline), attributes opaque key/values.
   `createDocumentBuffer` = `createTextBuffer` contract (controlled/
   uncontrolled, selection, `step`, `maxLength`) composed over the text via
   the new `onReplace` hook (every text-buffer edit is one `replace(start,
   end, text)`), plus `document()`, `attributes()`, `format(patch)` (range
   or pending typing attributes), `formatBlock(patch)`, `insertAtom(attrs)`,
   `setDocument`. Typed text inherits the char before the caret; `\n`
   splits a paragraph keeping its attributes, deleting one merges. No undo,
   no compose/transform (additive later). Headless checks: probes/doc-test-probe.ts
   via probes/multiline-probe.tsx `doctest`.
3a. **Shared shell** - DONE 2026-08-19 (uncommitted). `EditorField`
   (packages/components/src/editor-field.tsx, internal): everything of
   `TextInput` that does not know what the value is (focus, blink, nav
   action, keys, text session, tap-to-position, editor layout, viewport
   height/scroll, placeholder, caret, border), taking `buffer(step)`,
   `runs()` and `renderLine`. `TextInput` is the string wrapper. Core:
   `TextEditorLayoutInput.runs` explicit, `unitInk` (glue-aware, used by
   `layoutNextLine` and `splitWide`).
3b. **Component** - DONE 2026-08-19 (uncommitted). `RichTextEditor`
   (packages/components/src/rich-text-editor.tsx): `Document` value
   (controlled/uncontrolled), `editorRef` hands the app the document buffer
   as the formatting API (no toolbar). Lines from the shell fed with
   geometry runs derived from the document; each line one `d-text` with a
   `<span>` per style interval (document runs cut at paragraph boundaries).
   Drawn vocabulary: bold, italic, underline, code (mono), color, link
   (primary + underline) inline; heading 1-3 block. Other attributes are
   carried, not drawn. Verified live: styled render, format on a range,
   typing/caret through mixed sizes (probes/rich-text-probe.tsx).

   Open here: inline
   atoms draw as U+FFFC only - real atoms need laid-out children in
   detached text, the open engine question; lists (`list` block attribute)
   need a left-margin marker and indent in the shell.

## Prerequisite

[TextInput range selection](../done/text-input-selection.md) - DONE
2026-09-02 in the shared shell: applying a mark has a range, and the
selection highlight draws in `EditorField` for both components.
