---
title: A trailing line break leaves the caret on the previous line
description: After Enter at the end of a multiline TextInput the caret stays after the last character until more text is typed, because prepareText does not flag a break that ends the text; done means a text ending in a break has its blank last line and the caret sits on it.
created: 2026-09-21
---

# A trailing line break leaves the caret on the previous line

## Symptom

In a multiline `TextInput`, type `abc` and press Enter: the caret stays
drawn after the `c`. Typing `d` puts the `d` and the caret on the second
line. Two Enters leave the caret one line too high the same way. Reported
in [[quartz-heron]] item 4e.

## Cause

The native `prepareText` (`flux:rendertree`) sets `hardBreak` on a break
unit only when text follows it:

| text | lines |
|---|---|
| `"abc\nd"` | `[0,4,hardBreak]`, `[4,5]` |
| `"abc\n"` | `[0,4]` |
| `"abc\n\n"` | `[0,4,hardBreak]`, `[4,5]` (two lines, not three) |

`createTextEditorLayout` (packages/core/src/text-input.ts) adds the blank
last line for the caret only when the final line ended in a hard break
(`if (out.length === 0 || hardBreak)`), so `"abc\n"` has one line,
`lineOf(4)` falls back to line 0 and `xAt` puts the caret after `c`.

The native side contradicts `TextLine.hardBreak`'s own doc ("the line ended
at a hard break rather than by running out of width"): the last line of
`"abc\n"` did end at a hard break.

## Done looks like

`prepareText` flags a trailing break unit (`\n`, `\r\n`, U+2028, U+2029),
so `"abc\n"` yields a hard-broken line and the editor layout's existing
rule adds the blank line; `"abc\n\n"` has three lines. The editor needs no
text sniffing of its own.

Involves: the segmenter in alloy's text layout (where `hard_break` is set),
a case in alloy/src/tests/text_layout.rs for a break at the end of the
text, and a check that `Text` rendering (not only the editor) is unchanged:
a trailing blank line must not add height to a plain `<text>` if it does
not today, or that difference is a separate decision.
