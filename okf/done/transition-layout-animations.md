---
title: A node cannot animate from its previous laid-out box to its new one
description: Layout writes the solved box straight through, so every reflow is a jump - siblings snap closed behind an exiting node, snap apart around an inserted one, and a reordered list teleports; done means a node declaring a layout transition slides from the box it had to the box it gets, the piece every peer pairs with exit pop-out.
created: 2026-09-10
completed: 2026-09-10
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
animation, Compose's `animateItem`. Without it, an app has one route to a
smooth list: hand-animate `y` on every row and give up flex layout for the
list.

## Cause

`layout_phase` writes each node's solved box into `computed` and the paint
walk reads `location()` from it (composite.rs). Transitions animate
properties through `anim_value`/`set_anim_value`; the layout box is not a
property, and nothing remembers the box a node had before the solve.

## The contract

A node declaring a layout transition animates from the box it held before a
layout to the box that layout gave it:

```tsx
<view transition={{ layout: { duration: 250, curve: "ease-out" } }} />
<view transition={{ all: "300ms", layout: true }} />
```

`layout` is the peers' word and carries their meaning: the whole box,
position and size (Framer's `layout`, Reanimated's `LinearTransition`,
SwiftUI's implicit layout animation, Compose's `animateBounds`). It takes
what a property entry takes: a spec object (a spring unless `curve` names a
tween), a shorthand string, `delay`. `true` borrows the `all` entry and is
an error without one. `from` and `exit` under it are refused, there is
nothing to seed. `all` on its own never covers layout: a bare
`transition="300ms"` on every button must not make buttons slide on every
reflow, and no peer does that either.

The contract:

- A declaring node whose solved location changed under the same parent
  slides from where it was painted to the new location, on that motion.
  Its first layout, a reparent, a `display: none` toggle and a node not yet
  painted animate nothing.
- A reflow mid-slide retargets: a spring keeps position and velocity, a
  tween restarts from the current painted point.
- The slide is parent-relative. A sliding parent composes with a sliding
  child; a non-declaring parent's move carries its children instantly, as
  in every peer where each level declares its own.
- One position for everything: the node is painted, hit-tested and reported
  (bounding box, tree dump) at the painted location. What you see is what
  you tap. The offsetLeft-style layout box query keeps the solved box.
- `onTransitionEnd` fires with property `"layout"` at the settle.
- Consequences, not features: an exiting row keeps its last box (the
  pop-out) and the rows under it slide up past it; an insert slides the
  siblings apart while the new node appears in place with its own `from`; a
  `<For>` reorder is a detach and an insert under the same parent, so it
  slides; a window resize reflows everything, so every declaring node slides
  then, as under Framer.

This stage implemented the position half of that contract; the size half
landed as okf/done/transition-layout-size.md. The spelling did not change
between the stages, which is what made the size stage additive.

## What landed

The one design decision: the lane is the painted location, not an offset.
A track whose value is the node's painted location in parent space and
whose target is the solved location makes a reflow exactly the retarget
that existed (`Transitions::retarget`, alloy/src/rendertree/transitions.rs):
the target moves, a spring keeps its state, a tween restarts from the
current point. A delta lane (old minus new, animated to zero, the FLIP
libraries' shape) would have needed a new "displace a running track"
operation for the mid-flight case, care around the second layout the
paint phase re-runs each frame, and it points the size stage at Framer's
transform hack (size as scale, with child correction) instead of at
layout. The position lane reads straight into the dump, which is the
observability requirement below.

The pieces, each documented where it lives:

- The slide lane: `AnimProp::Layout` with the two-lane `AnimValue::Point`
  (alloy/src/rendertree/transitions.rs). Not a writable property: the JSX
  name table never maps to it, `entry_for` never matches it, it is never
  an exit prop; named `layout` for the end event. `TransitionConfig`
  carries the `layout` entry beside `all` and `stagger_ms`. One id rather
  than two scalar lanes: one track, one settle, one dump entry.
- `Slide` on `Element`, on declaring nodes only: the parent the node was
  last laid out under (the reparent check) and the painted location while
  a slide runs, absent when the node sits on its solved box. The lane's
  write reports `Damage::Compose`, a translate write's damage: the parent's
  walk places the child, so the parent's recording is what goes stale.
- `Element::placement`, the one accessor for where a node is (the painted
  location, else the solved one), behind the paint walk, the envelope, hit
  testing and the bounding box, so they cannot disagree (the padding-box
  rule, okf/done/padding-box-divergence.md). `LayoutData::location` stays
  the solved box for the offsetLeft-style `layout_box` query.
- The diff, `start_layout_slides` (alloy/src/rendertree/tree/transitions.rs),
  fed from the one seam a solved box changes at (`set_unrounded_layout`,
  layout/context.rs) and drained by `layout_phase` after the taffy run,
  which both the frame builder and the paint phase call. Its rules and why
  are on the function. `reconcile_slide` keeps the state in step with the
  declaration on every edit; `slide_remaining` is the dump's read.
- flux decodes the `layout` key (`decode_layout`, properties/transition.rs)
  and names the end event; lattice's dump adds `slide`; core's types.d.ts,
  AGENTS.md and the examples README, the MCP tool description and the
  debugging guide say the contract; packages/core/examples/layout-slide.tsx
  is the live example; probes/layout-slide-probe.tsx the driven one.

## Verified

Rust: alloy/src/tests/layout_slides.rs drives the real layout phase with
the headless context: a remove slides the rows below up from their old
boxes, halfway at half the tween and settling with a `layout` end event;
an insert slides the siblings apart and never the new node; a `<For>`
reorder (detach plus insert under the same parent) slides every displaced
row; a node never painted snaps; a reparent snaps and anchors under the
new parent, whose next reflow slides; a hidden row snaps out and back in
from the empty box while its neighbours slide both ways; a reflow mid-slide
retargets from the painted point with no jump; clearing the declaration
snaps with no stale track; a delayed slide holds at the old box until due;
an exiting row keeps its box while the rows below slide; the hit test finds
a sliding row where it is painted and misses its solved box; the snapshot
reports the painted position and the remaining offset.
flux/src/tests/properties.rs: the `layout` forms (object with delay,
shorthand, `true` borrowing `all` in either key order, `true` without `all`
refused, `false` and a bare `all` or shorthand declaring none, `from`/`exit`
refused) and `anim_prop("layout")` staying None.

Live, over the control API with the clock frozen
(probes/layout-slide-probe.tsx, a linear 300 ms slide, one `clock?step=1`
per read): after `remove`, the reflow frame reports the exiting row at its
box with `exiting` and every row below at its OLD y with `slide.y: -60`;
the next frames read 96.73, 83.43, 63.62 for the row that ends at 40, the
remaining offset shrinking in step, and by frame 20 every row sits on its
solved box with no `slide`. A `rotate` (last row to the front) reads the
moved row painted at 160 heading for 40 and the two displaced rows at their
old y heading +60; the footer, which did not move, stays put. An `add`
slides the rows below the new one apart while the new row reads at its box
from its first frame. Running live through eight reset/remove/rotate/add
cycles: `missedPresents` 0 over a 10 s window, 100 frames, no slow frames,
no fence timeouts, no errors logged.

## Not done, on purpose

- Size: the lane covers position; a size change snaps. Shaped as its own
  item, transition-layout-size.md in the backlog (the lane widens from a
  point to a box; the child model is its open question).
- Shared-element moves (`layoutId`) and per-axis slide motion: lines in
  okf/tiny.md, both additive on this entry.
- Slides on a window resize are the declaration doing what it says, as
  under Framer; nothing suppresses them.

Fixed on the way: after a `<For>` reorder the renderer's JS mirror of a
parent's children kept the moved node's old entry (`insertNode` in
packages/core/src/renderer.ts never removed a same-parent node's previous
slot, while the native tree does), so `getNextSibling` handed Solid stale
anchors and later inserts landed in the wrong place - in the probe, a row
appended after a rotate landed at the top and the footer drifted into the
list. The slides were right for the order the tree had; the order was
wrong. The mirror now unlinks a moved node from wherever it was, as the
native insert does; reset, remove, rotate and add sequences in the probe
read the expected order after it, with the slides intact.

## Related

- okf/done/exit-transitions-subtree.md, whose layout pop-out is what made
  this visible; the done record names it as the missing companion.
- okf/done/transition-per-direction-curves.md and
  okf/done/transition-delay-catch-up.md, the two smaller gaps against the
  same peers, closed.
- okf/done/spatial-node-exit-transitions.md: no arena equivalent, arena
  nodes have no layout.
