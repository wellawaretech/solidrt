---
title: A transient shadow-slot overlap during a subtree swap kills the app
description: Shadow placement runs synchronously at light attach, so a declarative swap whose incoming branch attaches before the outgoing one detaches sums both branches against the 8-slot budget and throws; a casting point light (6 slots) is unusable in any app that swaps scene content.
created: 2026-09-08
---

# A transient shadow-slot overlap during a subtree swap kills the app

## Symptom

```
Error: The scene's shadow set is full: 9 maps over the 8-slot budget
  at placeShadows (scene-shadows.ts:215)
  at SpotLight (spot-light.tsx:54)
```

An app swapping scene content declaratively (`<Show>`, `<Switch>`) mounts
the incoming branch before the outgoing branch unmounts, so for the length
of that swap BOTH branches' lights are attached. A branch holding two
casting spots (1 slot each) plus a casting point light (6 slots, six cube
faces) is exactly at the 8-slot budget on its own and sums to 9 across the
overlap. `createShadow` calls `placeShadows` synchronously at attach
(scene-shadows.ts), so the sum is tested at the worst instant, and the
failure is a throw that replaces the whole app.

Two things make this worse than a budget being a budget:

- Nothing says a branch may only ever claim HALF the budget. The rule as
  documented is per-scene, and an app that respects it still dies.
- The practical consequence is that a casting `<PointLight>` is unusable
  in any app that swaps scene content, since 6 of 8 slots leaves no room
  for an overlapping twin.

Reported by an app that worked around it by making the light not cast.

## Done looks like

One of these, decided when the item is worked - they are alternatives, not
stages, which is why they are one item:

- **Defer placement to the flush.** The scene already coalesces work into a
  microtask flush; if shadow placement moved there, a mount overlap would
  resolve itself before the budget is ever tested, and the throw would
  fire only for a set that is genuinely over budget once settled. This
  keeps the strict contract and makes it mean what it says. Cost: the
  placement's `adding` argument and the atlas sizing have to work over the
  settled caster set rather than one incoming light.
- **Degrade instead of throwing.** Drop the over-budget caster with a
  warning, the way Three and Godot both degrade here. Cheaper, but it
  cuts against the throw-in-dev validation policy and hides a real
  authoring mistake behind a warning nobody reads.

Deferring is the better shape if it is affordable: it fixes the reported
failure without weakening the contract. Degrading is the fallback if the
flush cannot own placement.

Either way the shadow-budget documentation gains the overlap rule
explicitly, since an app can hit it during the window before this lands:
a subtree swapped by `<Show>`/`<Switch>` overlaps its predecessor, so
size each branch to at most half the budget.

Involves: `packages/3d/src/scene-shadows.ts` (placeShadows, createShadow,
destroyShadow), the scene's flush/schedule in `scene.ts`, the shadow
budget paragraph in `packages/3d/AGENTS.md`.

## Not this

A "swap-out-first" mode on `<Show>`/`<Switch>` - guaranteeing the outgoing
branch detaches before the incoming one attaches - was the reporter's own
suggestion and would fix this class generally (mount cost spikes and
probe-owned environments too). It is Solid's control flow, not ours, so it
is not the fix here; if the overlap keeps producing symptoms across
subsystems it becomes an upstream conversation.
