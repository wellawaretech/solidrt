---
title: Elements built before a suspending read are orphaned on every retry
description: A component that creates an element and then reads a pending async value throws NotReadyError to the nearest <Loading>, which discards the partly-built subtree without freeing it, so every retry leaks the elements built before the suspend point - and the leak sentinel that catches it names the wrong cause.
created: 2026-09-03
---

# Elements built before a suspending read are orphaned on every retry

## Symptom

The natural spelling of a component that loads something async - a shell
element around a suspending child - leaks a node on every retry:

```tsx
return (
  <view flex={1} {...handlers}>   {/* built, then the boundary retries */}
    {scene()}                     {/* suspends here */}
  </view>
)
```

```
Leak sentinel: 1 nodes are unreachable and will never be freed: <view> x1
```

The `<view>` is created, the read below it throws `NotReadyError`, the
`<Loading>` boundary discards the subtree, and nothing frees what was
already built. The work-around is to hoist every element above the
boundary so the suspending component creates none of its own, which
inverts how the component reads and is not something the app author has
any reason to expect.

## Why nothing catches it

`NotReadyError` is deliberately transparent to the renderer's error
containment - `guard()` in `packages/core/src/renderer.ts` rethrows it
untouched, because it is a pending async read on its way to a boundary
rather than a failure. That is correct, and it is also why the partial
build is never cleaned up on the way past.

The nodes are then permanently unreachable: `scanForOrphans` (same file)
derives orphans from the proxy map as parentless, non-window, not
awaiting the destroy sweep, which is exactly what a discarded partial
build is. Node creation is not tied to the reactive owner - it is left
off the hot create/insert paths on purpose - so a disposed scope frees
nothing.

The sentinel does catch it, which is the good half. The bad half is that
its message names the wrong cause: it blames reading an element-valued
prop more than once and prescribes `children()`. That is the leak it was
built for ([unmount-node-leak](../done/unmount-node-leak.md)), it is not
this one, and following the advice does not help.

## Shape, two options

**Free on scope dispose.** Tie a created node's lifetime to the owner
that created it, so a retried or discarded scope takes its partial build
with it. This is the correct-by-construction answer and the one that
stops app authors having to know the rule. Two things to settle before
committing: the cost on the create path that was deliberately kept clear,
and the interaction with the remove-then-destroy sweep for a node that IS
mounted when its creating owner disposes (double-free risk), plus the
legitimate build-now-mount-later pattern the sentinel's own message tells
users to ignore its warning for.

**Or document it.** Widen the sentinel's message so suspend-retry
orphaning is one of the named causes rather than a wrong lead, and add
the hoist-above-the-boundary rule to the trap list in
`packages/core/AGENTS.md` next to the other element-lifetime traps.

The second is a fraction of the work and removes the wasted debugging,
but leaves a leak that grows with every retry. Prefer the first if the
create-path cost turns out to be affordable; take the second as the
interim either way, since the sentinel message is wrong today whatever
happens to the underlying behaviour.

## Done looks like

The natural spelling above runs through a `<Loading>` retry cycle with
the sentinel quiet and the orphan count flat across repeated
suspend/resolve rounds - or, on the documented path, the sentinel names
the actual cause and `AGENTS.md` carries the rule.
