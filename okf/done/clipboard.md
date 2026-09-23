---
title: Clipboard (navigator.clipboard)
description: Landed 2026-09-02: navigator.clipboard readText/writeText (text only, gui builds only) over SDL's clipboard, and Ctrl/Cmd+C/X/V in EditorField so TextInput and RichTextEditor copy, cut and paste.
created: 2026-09-02
completed: 2026-09-02
---

# Clipboard (navigator.clipboard)

## Symptom

Text selection exists, but copying the selection out of a `TextInput` (or
pasting into one) is impossible: no runtime API reaches the OS clipboard,
and the fields bind no clipboard keys. Text moves between solidrt apps and
the rest of the desktop by retyping it.

## Done looks like

- A `navigator` global (flux's first) with the web shape:
  `navigator.clipboard.readText(): Promise<string>` and
  `writeText(text): Promise<void>`. Solidrt lens: text only, no
  permissions model, no clipboard events; readText resolves "" on an
  empty clipboard; both reject on platform failure. Exists only on gui
  builds - absence is the availability check.
- `EditorField` binds Ctrl/Cmd+C (copy selection), X (cut), V (paste,
  single-line fields flatten line breaks), so `TextInput` and
  `RichTextEditor` both get clipboard for free.

## Roughly

SDL's clipboard belongs to the video subsystem's thread, so alloy grows
two `AlloyCommand`s carrying engine-free boxed responders
(`Set`/`GetClipboardText`), serviced in the app.rs command drain next to
`SetTextInputActive`; latency is bounded by the event wait's tick period
like every command. The JS surface is `standards_plugins/clipboard.rs`
(placement rule: web standard regardless of backing; the one gui-gated
standards module), installed from `gui::install` with a clone of
`alloy_cmd_tx`, each call bridging responder -> tokio oneshot ->
`with_pending` promise. flux-types `standards/clipboard.d.ts` documents
the contract.

## Landed (2026-09-02)

As shaped: `navigator.clipboard` in `flux/src/standards_plugins/clipboard.rs`
(the one gui-gated standards module, installed from `gui::install`; each
call is an `AlloyCommand` whose responder feeds the promise), the contract
in `packages/flux-types/standards/clipboard.d.ts`, and Ctrl/Cmd+C/X/V in
`EditorField` (copy and cut act on a range only; single-line paste flattens
line breaks).
