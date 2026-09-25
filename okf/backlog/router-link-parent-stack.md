---
title: Router link into a nested screen has nothing beneath it
description: "A launch link into a screen outside the tabs lands as a stack of one, so back leaves the app instead of going to the list; a route option to synthesize the parents beneath a linked entry."
created: 2026-09-25
---

# Router link into a nested screen has nothing beneath it

## Symptom

A launch link (or `initial` path) into a nested screen lands as a stack
of one entry, so back leaves the app. The platform convention for a link
into a detail is back to the list. Inside a tab this is solved: a link
into a tab lands with the tab's root beneath it
([router-tabs](../done/router-tabs.md)). Outside the tabs, and in apps
without tabs, it is not.

## What done looks like

A route option `parents: true` (or a router-level default) so a link into
that route lands with its parents' index screens below it, the way React
Navigation's `initialRouteName` on a stack does for a deep link.

## What it involves

A few lines in `packages/router/src/stack.ts` (`initialState`): for a
target under a route with the option, push each ancestor that has an
index child first. Wanted by the first app that links into a detail
outside its tabs.

Deferred 2026-09-23 from [app-routing](../done/app-routing.md) as
additive.
