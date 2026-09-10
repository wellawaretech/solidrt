---
title: A node cannot animate from its previous laid-out box to its new one
description: Layout writes the solved box straight through, so every reflow is a jump - siblings snap closed behind an exiting node, snap apart around an inserted one, and a reordered list teleports; done means a node declaring a layout transition slides from the box it had to the box it gets, the piece every peer pairs with exit pop-out.
created: 2026-09-10
---

# A node cannot animate from its previous laid-out box to its new one

## Symptom

Exits pop out of the layout flow at exit start
(okf/done/exit-transitions-subtree.md), so the siblings of an exiting node
reflow immediately. In a column that reflow is a jump: the rows below snap
up in one frame while the exiting row fades where it was. The same jump
shows on an insert (siblings snap apart to make room, then the new node
enters) and on a `<For>` reorder (every moved row teleports). None of it is
wrong, it is just what a plain removal without any animation does, and it
reads as unfinished next to the exit and enter that do animate.

Every peer pairs exit and enter animation with a sibling layout animation,
and it is the pairing that makes their pop-out feel complete: Framer
Motion's `layout` prop, Reanimated's `layout={LinearTransition}`, Vue's
`v-move` class, Svelte's `animate:flip`, SwiftUI's implicit layout
animation. Without it, an app has one route to a smooth list: hand-animate
`y` on every row and give up flex layout for the list.

## Cause

`layout_phase` writes each node's solved box into `computed` and the paint
walk reads `location()` from it (composite.rs). Transitions animate
properties through `anim_value`/`set_anim_value`; the layout box is not a
property, and nothing remembers the box a node had before the solve.

## Done looks like

A node declaring a layout transition animates from the box it held before a
layout to the box that layout gave it:

```tsx
<view transition={{ layout: { duration: 250, curve: "ease-out" } }} />
```

Stage 1 is position only: a node whose solved location changed slides from
the old location to the new one. Its first layout (the mount) animates
nothing, and a node that changed parent animates nothing (the old box is in
another space). Stage 2, if wanted, adds size, which is the harder half
(children lay out against the animating size, or against the final one and
get clipped). A `layout: true` shorthand can take the element's `all` spec.

## What it involves

The FLIP shape, in the tree rather than the DOM: after `layout_phase`, for
each node with a layout transition whose location moved, start a track on a
new layout-delta lane holding the old-minus-new offset and animate it to
zero; the paint walk adds the lane to the node's translation. The delta is
parent-relative, so a parent that is itself sliding composes with a child
that is also sliding, and a transform animation on `x`/`y` (a view's
translation lanes) stays a separate lane added alongside. Damage is the
moving node's extent, the same as any translation write.

What has to be remembered is one previous location per declaring node, set
at the end of every layout; the diff runs only over declaring nodes, so a
tree without layout transitions pays nothing. Retargeting mid-flight (a
second reflow while the slide runs) is the ordinary retarget on the lane:
the remaining offset carries over, which is exactly what FLIP libraries do.

Observable from the MCP, as part of done: the `x`/`y` the tree dump reports
(`get_render_tree`, `/tree`) must include the layout-delta lane, so a
sliding row reads at its painted position, not at the solved box it is
heading for; the same node with `props` shows the lane's remaining offset.
The check is: freeze the clock, remove a row, step a frame at a time, and
read the rows below at fractional positions on their way up. If the dump
showed the solved box, a slide and a jump would be indistinguishable, which
is the trap that hid the missing enter for an afternoon
(okf/done/enter-from-template-children.md).

## Related

- okf/done/exit-transitions-subtree.md, whose layout pop-out is what makes
  this visible; the done record names it as the missing companion.
- transition-per-direction-curves.md and transition-delay-catch-up.md, the
  two smaller gaps against the same peers.
