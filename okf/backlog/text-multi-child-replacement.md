---
title: Interpolated text may replace the whole text node on update
description: Updating one interpolation in a multi-child <text> was observed to replace the entire text node rather than the changed part; unverified since the Solid 2.0 bump, so the first step is a repro.
created: 2026-08-14
---

# Interpolated text may replace the whole text node on update

Observed while writing `<text>Hello, {count()}!</text>`: when `count()`
changed, the whole text node appeared to be replaced rather than just the
interpolated segment. Never root-caused. Two candidate layers: the renderer's
child-update path (packages/core/src/renderer.ts) treating a text element's
children as one opaque value, or Solid's own text-child reconciliation.

Cost if real: a `<text>` mixing static and reactive segments churns a native
node per update instead of editing one. That is a node create/destroy and a
fresh text layout per frame for anything animating a counter, timer or score
inside a sentence - the exact shape apps reach for first. It would also
invalidate the layout cache entry for that node every frame.

## First step is a repro, not a fix

The observation predates the Solid 2.0.0-rc.0 bump (269660a) and the
detach/deferred-sweep change to node lifetime, either of which could have
changed the behavior or the way it presents. Do not start from the write-up
above.

To reproduce: render `<text>Hello, {count()}!</text>` against a signal ticking
on onFrame, then watch node identity across updates - `get_stats` node counts
over time via MCP (a leak or steady churn shows up as nodes created and
destroyed per tick), and `get_render_tree` to see whether the text element
keeps its id. If node identity is stable and only the string content changes,
the observation was wrong or is fixed, and this item is dropped.

If it does reproduce, the questions to answer before proposing a fix:

- Does it happen with a single interpolation and no static siblings
  (`<text>{count()}</text>`), or only when static and reactive children mix?
- Is the replacement at the JS renderer boundary (a `removeNode` +
  `createNode` pair) or inside a single native `set_text` write?
- Does the same shape in a `d-text` behave differently?

Source: root TODO.md, migrated 2026-08-14.

[text-inline-spans](../done/text-inline-spans.md) makes multi-child `<text>` the
normal case; do this repro as part of (or before) its stage 2.
