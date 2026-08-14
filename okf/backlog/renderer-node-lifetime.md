---
title: Node lifetime is a deferred sweep, not reference lifetime
description: removeNode detaches and a microtask sweep frees, so control-flow reuse inside one tick survives but a node re-inserted in a later async tick is already gone, where the DOM would have kept it alive.
created: 2026-08-14
---

# Node lifetime is a deferred sweep, not reference lifetime

`removeNode` detaches the native node (`tree.detachNode`) and defers
destruction to a microtask sweep, `flushDestroy`
([packages/core/src/renderer.ts:66](../../packages/core/src/renderer.ts)). A
re-insert in the same tick cancels the pending destroy, so a node reused across
`<Show>`/`<Switch>` branches, or reordered by `<For>`, survives the way a moved
DOM node does.

The gap: a node removed in one tick and re-inserted in a LATER async tick has
already been freed. The DOM keeps a node alive for as long as JS holds a
reference to it; we keep it alive until the end of the current tick. Standard
control flow is synchronous, so nothing in the tree hits this today - an app
that stashes a node, awaits, and re-inserts would.

## Approach B, if it becomes worth doing

Tie native lifetime to the JS ProxyNode's lifetime with a FinalizationRegistry:
`removeNode` only detaches, and a finalizer calls `tree.destroyNode(id)` when
the ProxyNode is collected. The detach/destroy split already exists on both
sides that need it - `detach_node` and `destroy_node` in
[alloy/src/rendertree/tree.rs:122](../../alloy/src/rendertree/tree.rs) and the
flux gui tree - so only the JS-side trigger changes.

Costs, all of which argue for leaving it alone until something forces it:

- needs FinalizationRegistry in QuickJS; verify it is exposed in the rquickjs
  build before designing around it
- destruction timing becomes non-deterministic, which makes node-count
  assertions in tests and the MCP `get_stats` leak checks fuzzy
- detached subtrees linger in native memory until GC runs, so the memory
  profile of a churning list gets worse before it gets better

Do not pick this up speculatively. The trigger is a real app that needs a node
to outlive its tick; until then the sweep is both simpler and more predictable.

Source: root TODO.md, migrated 2026-08-14. Related: `done/unmount-node-leak.md`,
which fixed a different lifetime bug (element-valued props rebuilding subtrees).
