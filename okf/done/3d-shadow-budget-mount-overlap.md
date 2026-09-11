---
title: A transient shadow-slot overlap during a subtree swap kills the app
description: Shadow placement runs synchronously at light attach, so a declarative swap whose incoming branch attaches before the outgoing one detaches sums both branches against the 8-slot budget and throws; a casting point light (6 slots) is unusable in any app that swaps scene content.
created: 2026-09-08
completed: 2026-09-11
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

## Decision (2026-09-11)

Defer. The shadow set settles in the scene's sync, never at attach, and
the same rule moves the light cap: a per-scene budget is a settled-set
check at the flush.

- `createShadow`/`destroyShadow`/`shadowChanged` collapse into one
  `invalidate(light)` that marks the set dirty and schedules; a new
  `settle()` runs first in `sync` and does what `placeShadows(adding)`
  did over the settled set: drops the shadows of lights that left,
  stopped casting or changed shape, places the wanted casters' tiles in
  attach order until the budget is spent (first fit, so a later spot
  still gets its map when an earlier point light does not), creates the
  views of new casters.
- Over budget is not a throw-before-mutation: the casters that fit keep
  or get their maps, the rest light the scene unshadowed, and `settle`
  returns the error, which `sync` throws at its end after the light
  rewrite and the spatial flush, so a failing set never leaves the scene
  half-written. One report per change of the set (the dirty flag clears
  before the throw). This is what covers overlaps that span flushes
  (a Solid transition holding the outgoing branch, an exit on a casting
  light), which deferral alone does not.
- The light cap moves the same way: `_attachLight` no longer throws,
  `writeLights` writes the first MAX_LIGHTS lights and skips the rest,
  `settle` never places a caster past the cap, and sync reports once per
  set change.
- The report reaches the app's boundary: `createScene` takes `onError`
  (without it the sync throws, uncaught, log only), the `<Scene>`
  component hands one in that parks the error and bumps a signal, and a
  render effect in the component takes and rethrows it, so the error
  window (or an `<Errored>` closer in) shows it. Reset finds nothing
  pending and returns to the scene, which kept rendering with what fits.
  Added the same day after the first cut left the report in the log
  only, which is unnoticed in practice.

## Findings

See [solid-swap-attach-before-detach](../notes/solid-swap-attach-before-detach.md).

Verification (probes/3d-shadow-swap-probe.tsx, gitignored; `mode` debug
command): five `<Switch>` swaps between two rigs of two casting spots
and a casting point light (8 slots each) ran without a report, the atlas
at 8 tiles throughout; two casting point lights reported once and placed
the first (6 tiles); nine spots reported the light cap once and dropped
the atlas; back to a rig, no report, 8 tiles. cascades.tsx and lamps.tsx
load clean.

With the `onError` channel: the over-budget set and the nine-light set
each replace the app with the error window (message and `settle`/`sync`
stack); a tap on Reset through `/input` returns the scene, still
rendering with what fit (6 tiles, then 0), and the next fitting set
draws with no report. Checked in a bun probe first that a throw from a
render effect's compute reaches `createErrorBoundary` and that reset
recovers when the effect no longer throws. Reset logs a burst of
STRICT_READ_UNTRACKED warnings, one per node of the error window; the
halt probe's user-effect throw does the same with none of this code
involved, so it was the renderer's reset path: `destroyNode` compared
every node against the focus signal inside the boundary effect's apply.
Fixed the same day (the focused id is read once, untracked, before the
walk); Reset logs nothing now.
