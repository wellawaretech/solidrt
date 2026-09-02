---
title: Bidirectional text in the owned layout
description: The owned text engine places wrap units on a line in logical order and treats "start" as left, so RTL rich text spanning styled runs on one line, RTL paragraph alignment and mixed-direction line breaks come out wrong; feed UAX #9 levels into the breaker and placer.
created: 2026-08-17
---

# Bidirectional text in the owned layout

## Symptom

`<text>` in an RTL locale, or Latin with embedded Hebrew or Arabic phrases,
lays out wrong in specific ways: an RTL sentence that crosses styled runs
(`<span>`) on one line reads in the wrong order, `textAlign` "start"/"end"
resolve to left/right regardless of paragraph direction, and a line broken
in the middle of a mixed-direction stretch reorders its units incorrectly.
Text inside one wrap unit is fine: a unit is a real Impeller paragraph, so an
RTL word, or a whole RTL sentence in one style, shapes and reorders
correctly within itself. What is missing is bidi *between* units.

## Why

The owned engine ([text-layout-owned](../done/text-layout-owned.md))
segments text into wrap units and places them left to right in logical
order; bidi was deliberately out of its stages (postponed 2026-08-16) so the
LTR functionality could land first. Word-level segmentation was chosen
partly so this door stays open.

## Done looks like

- Paragraph direction (auto from first strong character, plus an explicit
  `direction` prop) resolves `start`/`end` alignment and the default
  alignment.
- Resolved embedding levels (`unicode-bidi`) per unit feed the breaker
  (breaks happen in logical order, as now) and the placer (visual reorder
  per line, UAX #9 L2), including justification and ellipsis on RTL lines.
- Atoms and floats: `float="left"|"right"` stays physical; a `start`/`end`
  form is a follow-up.
- Span hit testing and `layoutNextLine`'s cursor stay logical; visual order
  is the placer's concern.
- Probe rows: RTL paragraph, mixed Latin/Hebrew line, RTL rich text across
  spans, RTL alignment; a whole-text Impeller paragraph (built ad hoc in the
  probe; the in-tree paragraph engine was deleted 2026-09-02) as the
  reference to compare against, since Impeller handles bidi natively.

## Involves

`alloy/src/rendertree/text/layout.rs` (levels on `Run`, reorder in
`close_segment`), `shape.rs` (compute levels once per prepare, part of the
cache key), `mod.rs` (direction prop), the flux `text` properties, types
and docs. Roughly a session; the breaker's data model does not change.
