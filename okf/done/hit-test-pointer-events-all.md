---
title: pointerEvents="all" captured every point in the window
description: The hit test only applied its in-bounds gate to pointerEvents auto, so an "all" node outside the pointer descended into its children and fell through to a hit; any list with Icons (which set "all") routed every tap to the last icon in the tree. The gate now applies to auto and all.
created: 2026-08-22
completed: 2026-08-22
---

# pointerEvents="all" captured every point in the window

## Symptom

In a scrolling list of pressable rows that each carry an `Icon`, taps on
rows stop working as soon as the list is long enough to overflow, with a real
mouse as well as synthetic input. Rows in a short list press fine, and the
row that does work is whichever one comes last in the tree. A pointer-down
logged on the list wrapper reports `target` = the `path` inside the LAST
row's icon, laid out hundreds of pixels below the viewport.

## Mechanism

`alloy/src/rendertree/hit.rs` `hit_recursive` gated on bounds like this:

```rust
if pointer_events == PointerEvents::Auto && !is_in_bounds(local) { return false }
path.push(..);
if pointer_events == PointerEvents::All && is_in_bounds(local) { return true }
// ... recurse into children ...
true
```

For an `All` node the miss case is not handled: it is pushed, its children
are visited (they inherit `All`, so they are never rejected either), and the
function returns `true` at the end. An `All` node therefore "hits" for every
point in the window, and since children are tested last-first, the last such
node in the tree wins over everything laid out before it.

The `PointerEvents::All` doc says "captures all pointer events within
bounds" - the implementation just never checked the bounds on the miss path.
`@solidrt/components` `Icon` sets `pointerEvents="all"` (so the whole icon
box is the target rather than the path geometry), which is why it surfaced
as "lists with icons", not as "all".

## Fix

The gate applies to `Auto` and `All`; only `None` descends regardless (so a
click-through container's children can opt back in):

```rust
if pointer_events != PointerEvents::None && !is_in_bounds(local) { return false }
path.push(..);
if pointer_events == PointerEvents::All { return true }
```

Verified with the console app's explorer list (folder rows with icons, 30+
entries, scrolled): taps land on the row under the pointer, before and after
wheel scrolling, through `/__control__/input`.

## Why it hid for so long

Every `Icon` in the tree was a candidate, but only the last one won, and it
was usually inside a Button whose own box was where people tapped anyway.
It needs an `All` node laid out AFTER the thing you tap and not under it -
a long list of icon rows is the first such screen.
