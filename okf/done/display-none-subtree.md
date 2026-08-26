---
title: display "none" hides a node but not its subtree
description: Display::None returned LayoutOutput::HIDDEN without taffy's hidden-layout pass, so descendants kept their last boxes and kept painting; now the pass runs, and paint, hit and envelope walks skip hidden subtrees through one Element::is_hidden gate.
created: 2026-08-26
completed: 2026-08-26
---

## Symptom

Setting `display: "none"` on a view that has children zeroed that view's own
box and nothing else. Its descendants kept whatever boxes they had at their
last real layout and went on painting there, so the "hidden" content stayed on
screen - and, because its own box was zero, it painted outside any container,
over whatever now occupied that space.

Seen while collapsing a list pane: the pane narrowed from 320 to 44 px, and
the list drew across the detail pane beside it. The tree during the bug:

```
id 123  view  width 0    height 0     <- display: "none" applied here
  id 127  view  width 320  height 51   <- header, keeps its old box
  id 164  view  width 320  height 637  <- scroll region, keeps painting
```

## Cause

The zero box on the hidden node is written by its parent's container
algorithm: taffy's flexbox/block/grid pass finds a child with
`box_generation_mode() == None`, sets its layout to zero, then calls
`perform_child_layout` on it with `RunMode::PerformLayout`. Our dispatch in
`alloy/src/rendertree/layout/context.rs` answered that call with

```rust
Display::None => taffy::LayoutOutput::HIDDEN,
```

which is only the return value. taffy's `compute_hidden_layout` does the work
the case needs - clear the cache, write a zeroed layout, and recurse into every
child with `LayoutInput::HIDDEN`. We never recursed, so the descendants'
`computed` boxes stayed as they were, and paint and hit read `computed`
directly (there is no separate rounding copy that could have caught it).

The recursion is only half of it. A child reached that way arrives with
`run_mode == RunMode::PerformHiddenLayout`, and the implementer must honour it
before looking at the child's own display, or a Flex child of a hidden node
lays out normally. taffy's reference `compute_child_layout` opens with that
check. Ours did not - and it had a second trap: the measured-leaf branch came
first, and `compute_leaf_layout`'s `PerformHiddenLayout` arm is
`unreachable!()`, so a text or rect under a hidden view would have panicked
once the recursion existed.

Layout alone would not have been enough either. "Zero box" is not "do not
paint": a text with known width 0 still lays out and inks (one word per line),
`Line` and `Path` have unbounded extents and draw in their own coordinates,
detached `d-*` children inherit the frame but draw where told, and a stroked
zero rect still strokes. CSS says a `display: none` element generates no box
at all, and the walks have to say the same.

## What was done

- `Display::None` calls `compute_hidden_layout`, so the subtree is zeroed and
  its caches cleared (which is also what makes un-hiding relayout cleanly).
- `compute_child_layout` short-circuits on `RunMode::PerformHiddenLayout` as
  its first statement, ahead of the cache wrapper and the leaf branch.
- `Element::is_hidden()` (`style.display == None`; detached elements have no
  style and are never hidden) is the one predicate, consulted by the three
  child loops that must agree about what exists: the paint walk
  (`composite::record_node`), the hit walk (`hit::hit_recursive`), and the
  cull envelope (`cull::compute_envelope`). The envelope matters as much as
  paint: an unbounded `Path` inside a hidden pane would otherwise make the
  visible parent's envelope unbounded and defeat culling.
- `layout_children` still contains hidden nodes on purpose: the parent
  container is what zeroes them and triggers the hidden pass.
- Tests, one per module under `alloy/src/tests/`: `layout.rs` lays the
  collapse case out through taffy with a headless `Context` (hidden pane,
  header and rect body all zero, detail takes the width, everything comes back
  on `Flex`); `composite.rs`, `hit.rs` and `cull.rs` exercise the walk gates
  against hand-placed stale boxes (not painted per `PaintStats::nodes_painted`,
  not hit, envelope bounded).

## Why it matters beyond the one symptom

`display: "none"` is the natural way to hide a region without unmounting it,
which is what preserves scroll position, selection and any subscriptions
inside. Until now that pattern silently corrupted the layout, so every collapse
had to unmount through `<Show>` and lose that state instead. Components can
now grow a `collapsed` prop that keeps pane state.
