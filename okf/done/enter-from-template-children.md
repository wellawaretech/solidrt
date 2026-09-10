---
title: Enter animations (from) are lost whenever the transition config lands after the insert
description: A transition entry's `from` runs at insert, and `entered` is set before the config is even looked at, so any element whose config arrives a step later never animates in - a template child always, and a template root whose `transition` prop the compiler classified as dynamic. Done means a `from` plays on the element's first frame wherever it sits and however its spec was written.
created: 2026-09-07
---

# Enter animations (from) are lost whenever the config lands after the insert

## Status (2026-09-10): landed

The enter pass moved from insert time to the frame's advance. `insert_node`
queues a not-yet-entered node (`Transitions::entering`), and
`advance_transitions` drains the queue first thing
(alloy/src/rendertree/tree/transitions.rs `apply_enter_transitions`): after
the tick's script work, so a template child's props effect and a dynamic
`transition` prop have both landed and snapped, and before the paint, so
the node's first painted frame is at `from`. The mount-tick writes all snap
now; the write guard's any-running exception stays for writes made between
the advance and the paint (a transition-end handler's). A node detached or
destroyed again before the advance is skipped and enters at its next
attach. Stagger indices come from the same per-frame counter, so a batch
mounted in one tick still cascades in insertion order. The fallback
one-liner from "Involves" was dropped: without a re-run trigger it fixes
nothing, and re-running from the config write is order-dependent (a
property with no default reads as unreadable when `transition` precedes it
in the props effect).

The same gap existed one level down and closed with it: node transitions in
the spatial arena had no `from` at all ("element lifecycle conveniences,
not node vocabulary" - but a sprite popping in from scale zero is node
vocabulary). `NodeTransitionConfig` carries `enter_position` /
`enter_rotation` / `enter_scale`; `create` queues the node and the arena's
`advance_transitions` drains (`start_enter_transitions`), snapping to
`from` and animating to the transform it holds then - the created one, or
the target of a write the creating tick already made. flux:spatial
`setTransition` decodes `from` on a component entry (the component's
lanes; rejected on `all`). @solidrt/2d takes 2d units (`from: [x, y]`,
an angle, `[w, h]` or a group's `s`) and lifts them into the plane;
@solidrt/3d passes through and replays at every scene enter, since each
enter creates the core node anew.

Deliberate non-goals: delay, exit and stagger on nodes stay absent. A node
inserted from a transition-end handler is painted before its first
advance, so its first frame shows the mounted value; that needs a handler
that mounts during the end dispatch and is one frame.

Verified by alloy/src/tests/transitions.rs (the two enter tests moved to a
first-advance assertion, plus late-config and skip tests) and
alloy/src/tests/spatial_transitions.rs (four enter tests).

## Symptom

`apply_enter_transitions` (alloy/src/rendertree/tree/transitions.rs) runs
at `insert_node`, reading the node's transition config and the mounted
value. Its guard sets `entered` BEFORE the config is looked at:

```rust
let Some(el) = self.nodes.get_mut(&node_id) else { return };
if el.entered {
  return;
}
el.entered = true;
match &el.transitions {
  Some(t) => /* ... */,
  None => return,
}
```

So a node inserted without a transition declaration is permanently marked
as having had its enter, and a declaration arriving a moment later can
never fire one. The flag's documented job is only "fires on the first
attach only, never again on a move or reorder"
(alloy/src/rendertree/mod.rs), which it still does; it also swallows the
ordinary case of a config that lands one step late.

Solid's universal codegen builds a template as create-all, insert-children,
then one effect that writes every dynamic prop, and only then returns the
root for its parent to insert. Two ways to land late follow:

- a template CHILD (`<view><d-rect transition={{ opacity: { from: 0 } }} /></view>`)
  attaches with no config: `entered` is set, the config lands a moment
  later through the property path, and nothing runs. The element simply
  appears at its mounted value.
- a template ROOT whose `transition` prop is DYNAMIC. A root normally
  attaches after its props effect ran, which is why every `from` the
  components package ships works (popupFade on a modal surface, markMotion
  on a `<Show>` wrapper). But the split is made by the compiler, not by
  position: static props are handed to `createElement` and applied there,
  dynamic ones go through the `setProperty` hook from an effect that runs
  after `insertNode` (packages/core/src/renderer.ts). A call expression
  cannot be hoisted into the template - it may have side effects - so
  `transition={cardIn(-0.5, 0)}` is dynamic and lands one step after the
  node is in the tree, while an object literal whose members are constant
  references is hoisted and lands before.

The second one costs an afternoon to find. Factoring a shared spec into a
helper is the ordinary refactor - two mirrored cards want the same shape
written once - and it silently turns a working enter into no enter, with no
error, no warning and `srt check` passing. The bisect that finds it is
three views with identical specs in one row, two inline and one from the
helper; only the helper one fails, and moving the helper moves the failure.

It is invisible from every angle an app author has: `get_render_tree
--props` reports the settled target values, which is also what a
never-started animation looks like, and a capture shows the settled state
(get_snapshot cannot photograph a transition in flight either).

The mount-time write guard is not the cause (writes before the first paint
snap by design; an in-flight enter track is retargeted, not cancelled), but
it is the reason the enter pass cannot simply be re-run when the config
lands: the mounted value has not been written yet at that point either, so
there is no target to animate toward.

## Done

A `from` entry plays on the element's first painted frame wherever the
element sits in its template and however its spec was spelled, animating
from `from` to the value the element holds at that first paint. The two
prop classes behave the same, which is the part an app author has no way to
reason about otherwise.

## Involves

Defer the enter pass from insert time to the first paint (the walk stamps
`Element::painted`; the frame's advance, or the walk itself, can start the
tracks for nodes entered this frame that declare `from`), or run it when a
config with `from` lands on an attached, unpainted node and let the
retarget rule carry the later mounted-value write. By first paint the
tick's effects have run, so a dynamic `transition` prop has landed too.
Either way the test `enter_from_animates_first_attach_only` ("attach snaps
to from" right after insert) moves to a first-frame assertion. Stagger
delays index per frame already, so a deferred pass keeps the cascade.

The narrower one-liner, if the deferral proves expensive: only set
`el.entered = true` when the node actually has a transition config. It
fixes the common case and leaves a smaller window (a config arriving after
the node has been moved would enter late).

Either way, a dev-profile warning when a `transition` declaring `from`
arrives on a node already marked `entered` turns the silent failure into
one line of log output, and is worth having even after the fix.

## Why it blocks more than the enter

A reactive spec swap - `transition={{ rotateY: closing() ? TURN_OUT : TURN_IN }}`,
the obvious way to give the two directions different curves - makes the
whole `transition` prop dynamic and so costs the enter entirely. Measured
side by side in one frame, three frames after mount, with only the left
card's spec made conditional:

| card | spec | rotateY | opacity | scale |
|---|---|---|---|---|
| left | `closing() ? OUT : IN` | 0 | 1 | 1 |
| right | static | 0.62 | 0.15 | 0.94 |

So per-direction curves are currently unreachable rather than merely
awkward, and fixing the enter timing unblocks them on its own. See
exit-transitions-subtree.md, which carries that half.

## Docs regardless

Core's AGENTS.md animation paragraph and `examples/stagger.tsx` should say
it: a transition spec belongs in module constants read from an inline
object literal in the JSX, never built by a function call. The example
already does the right thing; it does not say that doing otherwise
silently breaks.
