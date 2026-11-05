---
title: One region adapter and one shots adapter in core for 2d and 3d
description: @solidrt/2d and @solidrt/3d each wrap core's region gate and shot blender in their own adapter - the same control flow written twice, about 250 lines a side - and the copies have already drifted (enter/leave defaults, grid validation, grid placement, the shot's start camera); move the dimension-free half into core behind a small bag of node operations, as core's grid hooks and shot blender already do.
created: 2026-11-05
---

# One region adapter and one shots adapter in core

## Symptom

The 10-20 alignment ([2d-region-handlers](../done/2d-region-handlers.md))
moved the residency gate itself into core (`createRegion`,
`createRegionGrid`, `createStreamingSource`), and core's
`createShotBlend` already owns the shot blending. What each package
still writes for itself is the ADAPTER over them, and the two adapters
are one design written twice:

| pair | lines (2d / 3d) | identical lines |
| --- | --- | --- |
| `src/region.ts` | 519 / 499 | 82 |
| `components/region.tsx` | 178 / 178 | 69 |
| `components/shots.tsx` | 90 / 93 | 51 |
| `src/shots.ts` | 64 / 63 | the same function since 2026-11-05 |

The region adapter, line for line in both: a handle over the core
region (`handleOf`/`nodeOf` and `stateOf` weak maps), core state
forwarded to the handle, the error falling through to the region's
listener, then the grid's, then the console, `coreHandler` mapping
core calls to the package handle, `shown` (content visible, measured,
proxy gone), `teardown` (content cleared and hidden; proxy back for an
explicit region, the cell destroyed for a grid), and the grid's
`spawn`, `cells`, `cellAt`, `onStateChange`, `dispose`. The component
pair shares the held-region machinery (the shown and data signals, the
proxy and content switch, the grid's cell list) and differs only in
the context it scopes and how it places the node.

The cost is drift, not bytes. Found reading the pairs on 2026-11-05:

1. `enter`/`leave` on a grid: optional in 2d, defaulting to 0.5 and
   1.5 times the largest cell side (`DEFAULT_ENTER_CELLS`,
   `DEFAULT_LEAVE_CELLS`); required in 3d.
2. Grid validation: 2d throws on a cell unbounded on every axis and on
   a malformed `bounds`; 3d passes both to core unchecked.
3. Grid placement. 2d's grid is layer space, and `<RegionGrid>` throws
   inside a `<Group>`. 3d's `<RegionGrid>` syncs transform props and
   `RegionGridOptions.origin` is documented as "in the grid node's
   space", but `createRegionGrid` declares the gate in world space and
   does not fold the node's transform in (its own comment: "a grid is
   placed at identity"). A 3d grid under a non-identity transform then
   builds its cells away from where the gate tests them. No test places
   a transformed grid; confirm with one before fixing.
4. Shots: the 2d shot started from fixed defaults and the 3d one from
   the owner's camera, until the 2026-11-05 fix gave
   `Camera2dTarget` a `camera()`.

Each of these is the same decision taken twice, and taken differently.

## Shape

Core keeps owning the mechanism; each package hands it the few
operations that are about its dimension, the pattern core's grid
`hooks` (`spawn`/`shown`/`teardown`) and `createShotBlend(push, mix)`
already use.

Regions: a core adapter over `createRegion`/`createRegionGrid` taking
a bag of node operations - make a group under a parent, place it at a
cell origin, show or hide it, destroy a child, list the content's
children, measure the content's bytes, and an optional release for
what the content holds at teardown. Core then owns the handle maps, the
forwarding, the error fall-through, `coreHandler`, `shown`, `teardown`
and the grid's cell surface. It also owns the decisions that drifted:
the enter/leave defaults, the validation, and the rule for a grid's
transform (one rule, enforced the same way on both sides).

What stays per package: the handle's dimension (2d `x/y/width/height`
on a layer, 3d `bounds` on a node), the byte measure (sprite records
against geometry and population buffers), 3d's `warm`/`own` holds and
`buildModel`, and the streaming source's point shape (`x, y` against a
`Vec3`).

Components: a core helper for the held-region lifecycle (`held`, the
proxy and content switch, the grid's cell list), taking the package's
`scope(parent, children)` that provides its own context. Core already
holds Solid-side helpers, so this is no new layer.

Shots: a core `createShots<C>(owner | owner[], mix, options)` over
`createShotBlend`, the owner being `{ setCamera, camera, size }` (since
2026-11-05 the same contract in both packages); each package's
`createShots` becomes the call with its `mixCamera`. The `<Shots>` and
`<Shot>` lifecycle (priority, `active`, removal on unmount) moves the
same way as the region components, the package supplying the context
its shot provides.

No public name changes. The packages keep their exports; the drift
fixes above are the only behavior change, each decided once in core.
Engines have nothing to compare here: this is where the code lives,
not what an app sees.

## Done when

- `packages/2d/src/region.ts` and `packages/3d/src/region.ts` hold only
  their node operations, byte measures and dimension-specific options;
  the two `components/region.tsx` and `components/shots.tsx` hold only
  their context scoping.
- Both `createShots` are one line over the core one.
- Drift items 1-3 resolved once in core, with a test per side for the
  grid placement rule.
- `bun run sol test packages/2d` and `packages/3d` pass;
  `sol check` passes on both.

## Involves

`packages/core/src/residency.ts` (or a sibling module beside it),
`packages/core/src/camera-control.ts`, both `src/region.ts`,
`src/shots.ts`, `components/region.tsx`, `components/shots.tsx`, the
region and shots tests on both sides (`packages/2d/tests/region.test.tsx`,
`packages/3d/tests/residency.test.tsx`, the shots cases in
`camera2d.test.ts` and `orbit.test.ts`), and the large-worlds and
views docs where the grid options are listed. Land after
[2d-node-verbs-one-per-act](../done/2d-node-verbs-one-per-act.md): that item
removes the per-kind destroy branch from the 2d adapter first.
