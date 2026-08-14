---
title: Every widget hand-wires its own hover/pressed/disabled variants
description: Button picks fill/hover/label with a switch over its variant and derives the background from press state by hand, and every other widget repeats the pattern; a helper that selects a prop bundle from state would collapse it.
created: 2026-07-26
---

# Every widget hand-wires its own hover/pressed/disabled variants

Hover, pressed, disabled and size variants are hand-wired signals in every
widget. `Button` picks fill/hover/label with a `switch` over its variant and
derives the background from press state by hand; every other widget repeats the
same shape.

A helper that selects a prop bundle from state is pure userland and probably
belongs in `@solidrt/components` rather than core.

Deliberately not urgent: worth doing only once the same shape has been written
three or four more times, so the abstraction is derived from real repetition
rather than guessed. Whatever lands must stay per-element property writes - no
cascade, no selectors - see
[what "something like stylesheets" already means](../notes/style-reuse-without-stylesheets.md).

Split from that item when okf was restructured; the other half is
[scoped-text-defaults](scoped-text-defaults.md).
