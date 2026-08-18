---
title: Line-through, overline, decoration color and styles
description: A price cannot be struck through and a link cannot get a dashed or colored underline; textDecoration knows only "underline" in the run's own color, solid. Extend it to a CSS-style list with line-through/overline, textDecorationColor and dashed/dotted/wavy/double, on the same self-drawn per-line mechanism.
created: 2026-08-18
---

# Line-through, overline, decoration color and styles

`textDecoration` accepts `"none" | "underline"` only, drawn solid in the
run's paint ([text-underline](../done/text-underline.md)). Missing, and what
it looks like when hit:

- A struck-through price or done item: no `line-through`. Same mechanism;
  position and thickness come from `ttf_parser::Face::strikeout_metrics()`
  (OS/2 `yStrikeoutPosition` / `yStrikeoutSize`) instead of the `post`
  table, so `FontMetricsTable` grows a strikeout entry and `Underline`
  becomes a decoration with a kind. `overline` sits at the ascent with the
  underline's thickness. `textDecoration` becomes a space-separated list as
  in CSS (`"underline line-through"`).
- A decoration in a color other than the text's: `textDecorationColor`,
  one more override on text and span; Impeller's paragraph path takes a
  Color already.
- Dashed, dotted, wavy, double: free on the Impeller paragraph path
  (`TextDecorationStyle`); the owned path draws them itself (dashes and
  dots as rects, wavy as a path, double as two rects). Wavy is the
  spell-check idiom, otherwise rare in apps.

Not in scope: skip-ink (needs glyph outlines; a documented limitation).
