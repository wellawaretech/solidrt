---
title: Routing and link delivery traps
description: "What building the router and link delivery turned up that stays true without them: the flux engine's exec-queue drain before module evaluation (launch facts at module scope), SDL's drop event as the URL delivery path on macOS/iOS/Android, a STRICT_READ_UNTRACKED pattern in For items, and the list-detail layout-route answer."
created: 2026-09-25
---

# Routing and link delivery traps

Distilled from [app-routing](../done/app-routing.md) (2026-09-23). True
regardless of that item.

## Launch facts at module scope need the exec drain before evaluation

The flux engine used to evaluate the entry module before draining the
exec closures queued between build and eval, so the sticky launch facts
(`env.launch`, `env.launchLink`, a replayed connection state) were
invisible at module scope: `env.launchLink` read null and `env.launch`
read "fresh" on a restored launch. Fixed in `flux/src/engine.rs` (drain
the exec queue, then the job queue, then evaluate); reading `env.launch`
or `env.launchLink` at module scope is correct only since then. Anything
that must be visible to module-scope code goes through that queue before
evaluation, not after.

## SDL delivers a URL as a drop

SDL's URL delivery on macOS (the kAEGetURL handler) and iOS (openURL) is
its drop path: the URL string arrives as `SDL_EVENT_DROP_FILE`. Alloy maps
a drop payload with a scheme to `AlloyEvent::Link` (`link_from_drop`,
tested); a file path stays unhandled. Android reuses the same path for a
warm intent: `SolidRTActivity.onNewIntent` calls SDL's own
`onNativeDropFile(link)`, so no new JNI and the vendored SDL Java stays
untouched. The cold link rides argv (`--link`) next to `--restored`, with
the intent's data cleared before SDL's lossy `getPath()` sees it.
Device-verified 2026-09-23 with explicit VIEW intents (`am start -n ... -a
android.intent.action.VIEW -d solidrt://settings`; no intent filter is
needed for an explicit component).

## A plain function over a memo, read per For item, is a strict read

A prop chain that ends in a plain function over a memo, read inside a
`For` item (the player's app list reading `active` from the home screen's
`selectedId`), raises `STRICT_READ_UNTRACKED` once per row at mount; the
same chain as a `createMemo` does not. Make the chain a memo.

## List-detail over a route tree

A layout route reads the matched child from the location and gives
`<Outlet>` to the pane that child belongs in (the player: settings and an
app's detail to the detail pane, connect to the list pane). `SplitView`
keeps its `showDetail` contract and knows nothing of routes. The cost:
two-pane no longer keeps a detail beside a panel that is its own route;
back returns to it.
