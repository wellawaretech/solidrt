---
type: backlog-item
title: Node/memory leak on unmount (element-prop double reads; fixed, with orphan stats + dev sentinel)
status: done
timestamp: 2026-07-18T00:00:00Z
---

# Node/memory leak on unmount

Source: app-port feedback round 2 (2026-07-17 session), finding 11. Reported
by a human as "memory usage is exploding", then reproduced and measured
through the MCP tools: cycling a search filter grew get_stats nodes/memBytes
monotonically (~6k nodes per cycle at 184 rows, roughly one full row subtree
per row per cycle), never returning to baseline.

## Root cause (found and fixed 2026-07-18)

NOT the detach/deferred-destroy pipeline. That path was verified watertight
two ways: the real renderer.ts + Solid For driven against a mocked native
tree (bun, flux:rendertree stubbed), and a live 184-row repro app on the real
runtime. Full-clear, partial-filter, identity-replacement (every row replaced
via reconcile), and typing-burst churn all returned node counts to baseline
exactly.

The leak: the universal JSX compile turns element-valued props into getters -
`get children() { return createElement(...) }` - so EVERY read of such a prop
builds a fresh native subtree. A subtree that is never inserted is never
destroyed: destroyNode only runs via the insert/remove sweep, so an unmounted
build is permanently unreachable by all cleanup, on both sides (native tree
and the JS proxy/handler maps). In DOM Solid the same double-read is only
wasted work (the GC reclaims the dropped nodes); in SolidRT it is a permanent
leak.

Offender class: probing element props with typeof. Pressable's
`typeof props.children === "function"` render-prop probe built and dropped
one full children subtree per row per mount - exactly the reported
one-row-subtree-per-row-per-cycle - plus one per hover/press state change.
Same pattern in Button/Badge/Radio (`isText()`) and Tooltip (`content`).

Fix (packages/components, 2026-07-18): resolve element props once via the
`children()` helper (re-exported from @solidrt/core) and let the typeof probe
and every mount site share the resolved memo. Render-prop children survive
`children()` because flatten only unwraps zero-arg functions; an arity>=1
function passes through intact (the same convention Solid's Show uses).

Verified live: the Pressable-wrapped repro went from baseline 8652 + 4048
nodes per filter cycle to 4604 flat (native and JS counts in lockstep).

The rule is documented in packages/cli/scaffold/AGENTS.md (item 17): read an
element-valued prop exactly once, at the place it is mounted; never
`typeof props.children` on the raw prop.

## Companion tooling (done 2026-07-18)

2. get_stats orphan visibility: the stats query now reports `mountedNodes`
   and `orphanNodes` (nodes unreachable from the root), computed live from
   the tree on the JS thread at query time - zero steady-state cost, exact
   when asked, fields absent when no engine runs. Native:
   RenderTree::mounted_count() (alloy tree.rs, tested), merged into the
   stats reply in lattice go/connection.rs. Orphans growing at a stable
   tree shape read directly as "unmount is leaking".
3. Dev-mode leak sentinel (packages/core renderer.ts scanForOrphans): every
   ~5s on a rendered frame, scan the existing proxy map for parentless
   proxies that are not the window root and not pending destroy, and
   console.warn once per element type. Deliberately NO per-creation
   bookkeeping (QuickJS hot paths stay untouched) and warn-only
   (auto-reclaim would break legitimate build-now-insert-later patterns).
   Dev bundles only, via the process.env.NODE_ENV define.

Drive-by fix while wiring the sentinel: the bundler's NODE_ENV define
substituted a bare identifier instead of a string literal (nothing had ever
read process.env.NODE_ENV from a bundle before), crashing at first use; the
define values are now quoted (packages/cli/src/bundler.ts).

Repro/driver app from the session: packages/cli/_leak_app.tsx (debug commands
setFilter / remap / counts over MCP call_debug; includes a deliberate
LeakProbe component that orphans one node to demonstrate the sentinel).
