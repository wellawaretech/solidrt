---
title: A spatial node cannot animate out, and its enter cannot own its motion
description: The 2d and 3d node transitions carried `from` but no `exit`, no `delay` and no endpoint object form, so a dying enemy or a collected coin was kept alive, closed and freed by hand, and the two trees disagreed on what "remove" meant; done means arena nodes carry the element tree's lifecycle vocabulary minus stagger, both trees destroy through one verb the exit rides on, and a node animating out is a ghost to every hit test and query.
created: 2026-09-10
completed: 2026-09-10
---

# A spatial node cannot animate out, and its enter cannot own its motion

## Symptom

Animating out is the default in games, not the exception: an enemy that
shrinks and fades on death, a collected coin flying to the HUD, a popped
bubble, a cleared match-3 tile, a card leaving a hand, a particle at the
end of its life. In every one of those the app kept the sprite alive
itself, wrote a closing target, listened for the settle event and only
then called `removeSprite` / `remove`. That is exactly the hand-rolled
dance the element tree's `exit` removed
(okf/done/exit-transitions-subtree.md), and it was worse here: while the
sprite was dying it still answered `raycast`, `overlap`, `sweep` and the
view's pointer pick, so a bullet hit a corpse unless the app also flipped
`setVisible` or moved the node off-world.

`from` was already node vocabulary (okf/done/enter-from-template-
children.md revisited the "element lifecycle conveniences, not node
vocabulary" rule of okf/done/spatial-node-transitions.md), but it stopped
there: a bare lane array, no `delay`, and none of the endpoint object form
the element tree got in okf/done/transition-per-direction-curves.md. So a
node entered on its entry's curve and could not leave at all.

Shaping the fix surfaced a second symptom: the trees did not agree on
what removal is. 2d's `removeSprite`/`removeGroup` and 3d's
`removeInstance` destroyed, 3d's `remove` detached a re-addable subtree,
and each package's docs explained the other's exception. An exit
animation belongs to destroy in every peer (Godot `queue_free`, Unity
`Destroy`, PixiJS `destroy()`, Phaser `destroy()`, Framer `exit` on
unmount) and is meaningless on a detach - a detached node is coming back,
and a re-add mid-exit would draw the corpse and the new node at once - so
the verb the exit rides on had to exist, with one meaning, in both trees
first.

## The rule

A node animating out is a ghost: it is painted and nothing else. It is
not hit-tested by the pointer (the element tree's routing, a 2d layer's
pick, a 3d view's `pick`), not returned by `raycast`, `overlap` or
`sweep`, and not a collision partner. The element tree already did this;
the arena now does the same (one chokepoint, `collider` in
alloy/src/spatial/mod.rs, behind every query and both trees' picks), and
packages/core/AGENTS.md states the rule once, for both trees. The other
half holds in both: an entering node is live from its first frame,
whatever `from` it is passing through.

## What landed

The removal verbs, aligned. `remove` detaches and snaps; `destroy` frees
for good and is where `exit` plays - the same words with the same meanings
in @solidrt/2d and @solidrt/3d, the peers' spelling:

- 3d gained `destroy(node)`: the subtree is gone for good, every handle in
  it inert (writes no-ops, `add` throws, `remove` skips); an instance is
  destroyed like any node (its slot recycles at the free), so
  `removeInstance` is gone. `remove` stays Three's detach and never
  animates. The components destroy on unmount.
- 2d's `removeSprite`/`removeGroup` became `destroySprite`/`destroyGroup`.
  2d keeps having no detach (a sprite cannot exist outside its layer; a
  sprite that should come back is hidden), so after the rename no 2d verb
  is called remove and nothing is left to mean two things.

Node transition entries take the element vocabulary whole
(`NodeTransitionEntry { motion, from, exit }` with `NodeEndpoint { value,
motion }` per component in alloy/src/spatial/transitions.rs, the element
`TransitionEntry`'s shape): `from` and `exit` as a bare lane array or the
endpoint object `{ value, curve?, duration?, bounce?, delay? }`, merged by
the element decoder's rule through one shared endpoint decoder
(flux/src/alloy_plugins/properties/transition.rs `decode_endpoint_with`);
`delay` on entries and endpoints, riding a per-track clock (`since_ms`)
and a held-write list, so a late frame catches up as the element tracks
do. 2d lifts endpoint values through the same lane lift as `from`, bare
or inside the object. `stagger` too, on an ancestor's declaration (a 2d or
3d Group): the item first said arena nodes have no tree order to cascade
in, but stagger needs only a parent chain, a per-frame occurrence order
and a delay mechanism, and the arena has all three - `set_parent` is the
hierarchy, `children` is attach order (JSX order for template children),
enters drain in creation order and exits start in the consumer's
children-first teardown order, and the held-write list is the delay. It
is the element rule verbatim: nearest declaring ancestor, per-frame
counts, enters and exits counted apart, descendants only.

The arena (alloy/src/spatial/mod.rs): `Spatial::exit(id)` starts each
declared exit from the component's current mid-flight value on the exit's
own motion and marks the node `leaving`; `destroy(id)` keeps its meaning,
free now, and on a leaving node is the cancel. A leaving node frees when
its exit tracks are done AND no leaving child remains under it: both
trees tear down children-first, so a parent's `exit` finds its leaving
children in place and waits for them - the element tree's subtree gating,
for the cost of a children scan, and what keeps a dying group's parts in
its frame to the end (a mixer driving its joints keeps driving them).
Undeclared components keep running and never gate. Settles of a leaving
node feed its free instead of `onTransitionEnd`; frees of leaving nodes
land in `take_freed`, however they came.

flux: `exitNode`, `describeNode` (the node dump: leaving, shown, the
motion in force per component with a held write's due time) and one
`spatialNodeFreed` event per freed leaving node, emitted after the
advance's flush so a slot recycled on it never races the corpse's last
frame. JS: a 2d sprite's pose slot and a 3d instance's record slot recycle
at the freed event; a 3d mesh's draw entries detach at it (the handle's
`_scene` goes at destroy so every scene-routed setter is a no-op, its
`_node` at the free so a mixer's targets and growInstances' bound-record
check stay right). Every dispose path (`layer.dispose`, `scene.dispose`,
`disposeInstances`, `model.dispose`) frees its corpses on the spot first,
so nothing draws with a freed buffer or texture.

A follow-up closed the holes a review found: the populated meshes'
components unmounted through `disposeInstances` alone (a detach plus the
buffer free, cutting their instances' exits short), so they now unmount as
`destroy(mesh)` then `disposeInstances(mesh)`, and a disposer reaching a
node still leaving waits for its free (`afterFree` in
packages/3d/src/node.ts; `disposeInstances` and `model.dispose` use it,
while a scene's or a layer's `dispose` still cuts corpses short, the whole
target being torn down); the layer and scene roots can carry `stagger`
(`createSpriteLayer({ stagger })` / `layer.setStagger`, `createScene({
stagger })` / `<Scene stagger>`), so a cascade needs no wrapping group.

Docs: the lifecycle and retargeted-motion paragraphs of both packages'
AGENTS.md, the rule in packages/core/AGENTS.md, the flux-types
declarations. packages/2d/examples/springs.tsx and
packages/3d/examples/exits.tsx are the live examples: tap a sprite or a
crate and it leaves through its `exit`, a replacement pops in through its
`from`; the 3d shelf cascades out and back through `<Show>` on the
Group's `stagger`.

## Verified

Rust: alloy/src/tests/spatial_transitions.rs (exit runs and frees at
settle; nothing to animate frees at once; mid-flight momentum survives
the exit retarget; a leaving node is invisible to raycast while still
shown; a parent waits for leaving children and frees after the last,
with or without an exit of its own; `destroy` on a corpse takes its
corpses and reports them; destroying the last corpse frees a waiting
parent; leaving nodes refused by `set_parent`; undeclared components keep
running without gating; clearing a leaving node's declaration frees it;
an exit in the creating tick skips the enter; held writes run on schedule
and catch up after a late frame; a newer write restarts the hold and an
immediate one supersedes it; enter and exit delays hold; a held exit to
the current value frees when due; stagger spaces enters and exits under
the declaring ancestor, the group waits for the last of a staggered
cascade, counts are per frame with the nearest ancestor winning, and a
group's own lifecycle and ordinary writes never stagger) and
flux/src/tests/properties.rs (the node entry decoder: delay, bare and
object endpoints, the merge rule, lane counts, endpoints refused on
`all`, `stagger` a positive number of ms).

Live, over the control API with the clock frozen (probes/2d-exit-probe.tsx,
probes/3d-exit-probe.tsx; one `clock?step=1` request per frame - at scale
0 a running animation does not chain frame demand, so a queued `step=n`
trickles in on idle ticks, and the leaf snapshot is display-scaled, so a
crop is at leaf coordinates times the display scale). 2d: a live sprite is
picked, overlapped and fully lit; after `destroySprite`/`destroyGroup` and
four stepped frames `describeNode` reports the sprite leaving with its
scale motion in force and the group leaving with its delayed position
write held, pick and overlap return nothing, and the sprite's crop is
still 126 of 144 pixels lit (shrinking from 144); the child (100 ms exit)
frees first, then the sprite (200 ms), then the group (100 ms hold + 200
ms), `spatialNodeFreed` in that order; afterwards both spots are dark and
a respawn recycles the freed slot. 3d: a `destroy` on a group holding a
crate with a scale exit and a ball without one frees the ball at once
(its draw entry gone from `/gpu`), keeps the crate leaving and drawing
while `pick` misses it, holds the group waiting on the crate, then frees
crate and group in that order and the GPU inventory drops to no entries.
Stagger, live: a group declaring `stagger: 40` with three sprites added
in one frozen frame holds their enters at +0, +40 and +80 ms in add
order; `destroyGroup` holds their exits the same way in destroy order and
frees the sprites in order, the group last. The populated-mesh unmount,
live: an `<InstancedMesh transition={{ stagger: 30 }}>` with three
`<Instance>`s declaring scale exits, flipped off through `<Show>`, keeps
its instanced draw entry (count 3) while the instances leave with their
exits held at +0, +30 and +60 by teardown order, and drops the entry only
after the last free; no freed-buffer warning. No errors logged in any run.

## Not done, on purpose

- A 2d detach verb: a sprite cannot exist outside its layer; hiding is the
  pooling idiom, as it was.
- A raw flux:spatial consumer that leaves a live child under a node it
  exits: the child stays put and becomes a root at the free, as `destroy`
  always did; the 2d and 3d packages never produce this arrangement.

## Related

- okf/done/exit-transitions-subtree.md, transition-per-direction-curves.md,
  transition-delay-catch-up.md: the element-tree side this mirrors.
- transition-layout-animations.md: the element tree's remaining gap; no
  arena equivalent, since arena nodes have no layout.

The findings are kept beside the code they govern, per okf/README.md:
the free gate and the children-first contract (`exit`/`check_exit`,
alloy/src/spatial/mod.rs), a held target re-sent keeping its hold
(`NodeTransitions::write`, alloy/src/spatial/transitions.rs), the freed
event's ordering after the flush (flux/src/alloy_plugins/spatial.rs
`tick`), and the two-step inert marking of a 3d handle (`letGo` /
`finishLeave`, packages/3d/src/node.ts).
