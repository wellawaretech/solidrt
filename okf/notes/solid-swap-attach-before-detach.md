---
title: A declarative swap attaches the incoming branch before the outgoing one detaches
description: Solid 2.0 runs the incoming branch's body before the outgoing branch's cleanup on a Show/Switch flip, both inside one flush, so a per-scene budget summed at attach counts both branches; test budgets over the settled set at the flush instead.
created: 2026-09-11
---

# A declarative swap attaches the incoming branch before the outgoing one detaches

On a `<Show>`/`<Switch>` flip Solid 2.0 (rc.6) runs the incoming branch's
component bodies first and the outgoing branch's `onCleanup` callbacks
after: a recomputed owner's old children and disposals are parked as
deferred ("zombie") disposal and drained by `commitPendingNodes` inside
`finalizePureQueue`, still inside the same flush. A microtask queued from
the incoming body runs after both.

Checked with a bun script against the repo's `solid-js` (`Show` with a
`fallback`, bodies and cleanups logging, a `Promise.resolve().then` from
each body): `body A`, flip, `body B`, `cleanup A`, flush end, then the
microtasks.

What follows for anything with a per-scene cap: a check at attach sums
both branches for the length of the flush, so a budget an app respects
still fails during a swap. Test the budget over the settled set at the
flush (the 3d scene's sync microtask does this for the light cap and the
shadow slots), and when it fails keep the state consistent (place what
fits in attach order, report once). Overlaps that span flushes - a Solid
transition holding the outgoing branch while the incoming one loads, a
node with its own exit transition - are deliberate and must fit alongside
their successor.

Origin: [3d-shadow-budget-mount-overlap](../done/3d-shadow-budget-mount-overlap.md).
