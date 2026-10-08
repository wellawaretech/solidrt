---
title: Decide whether flex items keep the web's automatic minimum size
description: A flex item's automatic minimum is its content size, so a child larger than its share pins every ancestor until someone writes minHeight 0; Yoga has no such minimum and React Native layouts never need the escape. Decide which default solidrt wants, with the text and scroll contracts measured against it.
created: 2026-10-08
---

# Decide whether flex items keep the web's automatic minimum size

Flexbox gives every flex item an automatic minimum size (`min-height:
auto`) of its content: a child larger than its share cannot be shrunk below
it, and the overflow propagates up through every ancestor that holds the
item until an explicit `minHeight={0}` breaks the chain. It is the single
most common flexbox trap on the web, and the same one here: the fill-leaf
case ([fill-leaf-intrinsic-size.md](../done/fill-leaf-intrinsic-size.md)),
the collapsed or overgrown scroll viewport, the editor field's root that
carries the workaround today.

Yoga, the layout engine React Native layouts are written against, does not
implement the automatic minimum: a flex item shrinks to zero unless it says
otherwise, and no React Native layout carries `minHeight: 0`. Through the
solidrt lens (a single known app, simplified semantics under standard
names) that is a defensible default. taffy supports both: a `min_size` of
zero on every flex item is the Yoga behavior.

Not decided, and not to be folded into another change: it alters what every
existing layout does when content outgrows its box. Text in particular:
today a text's min-content width is its longest word and that is what keeps
a word from being cut; under a zero minimum a text in a narrow row would be
shrunk past it and overflow or clip instead. The scroll contract
(`createScroll` warns about a collapsed viewport) assumes the current rule.

## Done when

- A decision, in a design document for layout (none exists yet; this would
  be the first entry), with the text, scroll and fill-leaf cases worked
  through under both rules.
- If the rule changes: the default applied in the engine, the `minHeight={0}`
  workarounds removed, the layout tests that encode the automatic minimum
  (the `split()` fixture in `alloy/src/tests/layout.rs` keeps stated heights
  with `flex_shrink: 0` for that reason) revisited, and the AGENTS.md
  layout guidance updated.
- If it does not: a note under `okf/notes/` saying why, so the question is
  not re-derived, and `contain="size"` documented as the escape.

## What it involves

A survey first: every place in the repo and the examples where a flex item
relies on its content as a minimum (texts in rows, lists of fixed-height
rows, scroll viewports), then a trial with the default flipped in a branch
to see what breaks. The engine change itself is small.
