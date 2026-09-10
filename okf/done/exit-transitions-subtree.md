---
title: An exit transition reaches only the removed node, never its descendants
description: begin_exit runs from detach_node, and the renderer detaches only the node it was asked to remove, so a panel whose children should animate out has to hand-roll a closing signal, a held mount and a settle cue; done means a detached subtree's declared exits all play and gate the root's free.
created: 2026-09-10
---

# An exit transition reaches only the removed node, never its descendants

## Symptom

`exit` values sit beside `from` values on the same transition entries, so
the obvious spelling for "these cards leave the way they arrived" is an
`exit` on each card. Nothing animates: the cards hold their mounted values
until they are freed, measured with `get_render_tree props:true` (the
enclosing overlay animates its own `exit`, the cards read `rotateY: 0,
opacity: 1` all the way down).

Wrapping each child in its own `<Show>` with the same condition does not
help. Disposing the outer branch does not route the inner removals through
`removeNode` first, so the children are still swept away as descendants.

What works instead is a second mechanism for the same intent: a `closing`
signal that writes every child back to its entry values, the parent kept
mounted past the phase flip so a disposed subtree does not swallow the
writes, `pointerEvents="none"` on it meanwhile so the app underneath is
live again, and an `onTransitionEnd` on the slowest property to unmount
when the motion settles. About fifteen lines and two pieces of state to
restage what the runtime already does for a detached node, and it drags in
its own problem: the entry's single `curve` now serves both directions, so
an `ease-out` enter run backwards spends its whole fade in the first few
frames (see the shared-curve item below).

So an app either ships a lesser animation on the way out or reimplements
the lifecycle. Neither is visible as a choice from the docs: nothing says
the declarative form will not reach.

## Cause

`removeNode` (packages/core/src/renderer.ts) detaches exactly one node -
one `tree.detachNode` call - and `begin_exit` runs from `detach_node`
alone (alloy/src/rendertree/tree/mod.rs). A descendant is never detached;
it is freed with its ancestor. The behaviour is already stated in
`destroy_node`'s doc comment ("Descendants of a destroyed node never
exit-animate on their own"), so it is deliberate and known engine-side and
unstated everywhere an app author reads.

`examples/stagger.tsx` is the doc for this feature and its exits work,
because its animating rows *are* the removed nodes - the one arrangement
where they do. An overlay whose contents leave, which is what the stagger
ladder exists to serve on the way in, is the arrangement that silently
does nothing.

## Done looks like

A detached subtree's declared exits all play, staggered as they are on the
way in, and the root's free waits for the last of them. An overlay that
animates its contents out is then the same five specs with `exit` values
beside their `from` values, static props, no signal, no cue and no pointer
handling: an exiting node is already hit-test invisible and already frees
itself when its tracks settle.

## What it involves

`detach_node` walks the subtree and calls `begin_exit` on every descendant
that declares one, and the root's `doomed` free is gated on all of them
rather than on its own tracks. The pieces are there: `exit_props` already
names the track set that gates a node's free, `stagger_delay_for` already
climbs to the nearest ancestor carrying `stagger_ms` so the cascade is
subtree-shaped, and `abandon_exit` gives the move case its undo (a re-insert
of the root has to abandon the whole subtree's exits, not just the root's).

Two ordering questions to settle while doing it: a descendant whose own
tracks settle first must stay painted until the root frees, and a second
removal arriving mid-exit must not start a second cascade.

Failing all of that, the fallback is one sentence in the `exit` doc comment
(`TransitionSpring`/`TransitionTween` in packages/core/src/types.d.ts) and
in core's AGENTS.md animation paragraph, plus an example with an overlay
whose contents leave. That is a documented limitation, not a fix, so it
belongs in notes/ rather than here if it is where this lands.

## Related

Open work against the peers (Framer Motion, Reanimated, Vue, Svelte,
SwiftUI), shaped after this landed: transition-layout-animations.md
(siblings slide instead of jump, the companion to the pop-out),
transition-per-direction-curves.md (the shared curve below) and
transition-delay-catch-up.md (a held write starts late under a hitch).

The shared curve. A transition entry carries one `curve`, and the same
entry serves the property in both directions. `ease-out` is the natural
enter and is wrong played backwards; `ease-in-out` reads acceptably both
ways and is a compromise in each. `from` and `exit` are already per-entry,
so the API distinguishes the directions and only the curve cannot differ:
a `curveIn`/`curveOut` pair, or letting `from`/`exit` take
`{ value, curve, duration }`, would close it. The workaround that
expresses it today is `transition={{ rotateY: closing() ? OUT : IN }}`:
the enter pass now runs at the mount frame's advance, so a dynamic spec
no longer costs the enter (see
[enter-from-template-children](../done/enter-from-template-children.md)).

## Layout pop-out

Landed with it: an exiting root leaves its parent's layout flow at exit
start instead of at the settle. With the panel as the removed node, a
re-mount mid-exit stacked the new panel under the old one for the length of
the cascade and the column jumped closed when it freed. Now siblings reflow
at once and the exiting subtree is painted at its last box relative to its
parent, so it follows the parent's moves but not the parent's own reflow -
the same trade Reanimated's exiting animations and Framer Motion's popLayout
make. An insert whose anchor is out of the flow (an exiting node, a d-*
element) lands before the next sibling that is in it.

## Findings

Kept beside the code they govern, per okf/README.md: the membership model
and why track tags were rejected (`exit_root_of` in
alloy/src/rendertree/tree/transitions.rs), the worklist that lets an inner
root's finish free the outer in the same pass (`advance_transitions`), the
enter pass spending the enter of a node already leaving
(`apply_enter_transitions`), and a held write activating at the first
advance that finds it due rather than at its scheduled time
(`Transitions::schedule` in alloy/src/rendertree/transitions.rs).
