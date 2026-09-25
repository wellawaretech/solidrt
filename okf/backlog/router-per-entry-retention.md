---
title: Router per-entry retention
description: "Back to a list screen remounts it, so its scroll offset and focused row are lost; each stack entry should own a keyed store that lives while the entry is on the stack, with ScrollView opting in and focus restored on pop."
created: 2026-09-25
---

# Router per-entry retention

## Symptom

A pushed screen unmounts the one beneath it (the router renders one
keyed chain per stack), so back to a list comes up at the top with
nothing focused: the scroll offset and the focused row are gone. Tabs
([router-tabs](../done/router-tabs.md)) keep whole tabs mounted, which
covers switching tabs, not a push inside one.

## What done looks like

Each stack entry owns a keyed store, `useRouteState(key, initial)`, that
lives while the entry is on the stack (any stack: the parent's or a
tab's). Scroll offsets and the focused node's id go there; on pop the
binding restores focus through core's `setFocus`. `ScrollView` in
components opts in with one call; nothing in components is required by
the router.

## What it involves

- The store keyed by stack position in `packages/router/src/router.tsx`,
  dropped when the entry leaves the stack (pop, replace, reset).
- `ScrollView`'s opt-in and the focus restore.
- Whether it is on by default waits for a measurement of a list-screen
  remount on the low-end baseline with the player on the router.

Deferred 2026-09-23 from [app-routing](../done/app-routing.md) as
additive.
