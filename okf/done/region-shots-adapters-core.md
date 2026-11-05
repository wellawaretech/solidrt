---
title: Region and shots - put each generic piece at the layer that owns it
description: @solidrt/2d and @solidrt/3d each wrap core's region gate and shot blender in an adapter of their own, written twice and already drifted (enter/leave defaults, grid validation, grid placement, the shot's start camera); move each generic piece down to the lowest layer that owns it - grid placement into the Rust spatial core, defaults and the package handle into core TS, shots into core camera-control - and leave the packages only what touches their own representation.
created: 2026-11-05
completed: 2026-11-05
---

# Region and shots: each generic piece at the layer that owns it

## Symptom

The 10-20 alignment ([2d-region-handlers](../done/2d-region-handlers.md))
moved the residency gate into core, and core's `createShotBlend` owns
the shot blending. What each package still writes for itself is the
adapter over them, and the two adapters are one design written twice:

| pair | lines (2d / 3d) | identical lines |
| --- | --- | --- |
| `src/region.ts` | 512 / 499 | 82 |
| `src/components/region.tsx` | 178 / 178 | 69 |
| `src/components/shots.tsx` | 90 / 93 | 51 |
| `src/shots.ts` | 64 / 63 | one body since 2026-11-05, 2d's list form apart |

Line for line in both region adapters: a handle over the core region
(`handleOf`/`nodeOf` weak maps), `coreHandler` re-wrapping every app
callback, core state forwarded to the handle, the error falling
through to the region's listener, then the grid's, then the console
(a copy of what core already does, since core only ever sees the
package's forwarder), and the grid's `cells`/`cellAt`/`onStateChange`
translating back.

The cost is drift, not bytes. Found reading the pairs on 2026-11-05:

1. `enter`/`leave` on a grid: optional in 2d, defaulting to 0.5 and
   1.5 times the largest cell side (`DEFAULT_ENTER_CELLS`,
   `DEFAULT_LEAVE_CELLS`); required in 3d.
2. Grid validation: 2d throws on a cell unbounded on every axis and on
   a malformed `bounds`; 3d passes both to core unchecked.
3. Grid placement. 2d's grid is layer space, and `<RegionGrid>` throws
   inside a `<Group>`. 3d's `<RegionGrid>` syncs transform props and
   `RegionGridOptions.origin` is documented as "in the grid node's
   space", but the gate's grid is world space (`GridSpec.origin`) and
   the node's transform is not folded in (`createRegionGrid`'s own
   comment: "a grid is placed at identity"). A 3d grid under a
   non-identity transform builds its cells away from where the gate
   tests them. No test places a transformed grid; write that test
   first.
4. Shots: the 2d shot started from fixed defaults and the 3d one from
   the owner's camera, until the 2026-11-05 fix gave `Camera2dTarget`
   a `camera()`.

## Where each piece goes

The placement test is AUDIT.md "2d and 3d speak one language", check 5.
The Rust spatial core takes per-node or per-frame work over arena state
that needs no JS, reusable by any extension on the arena and any
engine. Core TS takes generic orchestration that must call app JS, or
Solid lifecycle, receiving package things as plain values or type
parameters, never as an interface the package implements. The package
keeps what touches its own representation, and so does code only our
two packages could use. A bag of node operations (make a group, place
it, show it, destroy a child, list children, measure, release) fails
the test: a third party would describe its node tree to core through
seven callbacks to save a hundred lines. Core's hooks are EVENTS (core
says when), not operations (core says how).

### 1. A grid placed by a node - Rust spatial core (about a day, tests included)

`GridSpec` gains an optional placement `node`: the grid lives in that
node's space, the way an explicit region (`setRegion(node, spec)`) is
already tested through its node's world matrix. `origin`, `cell` and
`bounds` are then in the node's space; `enter` and `leave` stay world
distances, as an explicit region's box is local and its margins world.
`node` and `root` are exclusive (`GridSpec::validate`): a placed grid's
world is its node's top ancestor, derived every pass as `test_regions`
derives a region's, and a node that is itself a root places a grid in
no world, as a root node's region. `root` stays for grids with no node
(an app streaming non-node things, the tile layer).

- Geometry. The grid's world-space geometry is used in three places in
  `alloy/src/spatial/residency.rs`: the per-viewer cell index range in
  `enumerate_grids`, `cell_box` for the frustum and depth tests, and
  `cell_center` for enter urgency in `pace_enters`. A placed grid maps
  the viewer's reach (its eight corners) into node space through the
  node's inverse world matrix and takes the index range there; each
  cell box goes back to world as the bound of its eight transformed
  corners, an unbounded axis taking the node-space reach; the urgency
  center is computed in node space (the eye mapped in) and mapped
  back. The boxes are loose under rotation (most along an unbounded
  axis the rotation tilts) and never miss a cell; the enter/leave
  margins absorb the slack. The matrices are `f64`, so a placed grid
  far from the origin loses nothing against today's `f64` origin.
- Movement. The flush walk sets `region.moved` when a region node's
  world matrix changes (`spatial/mod.rs`, the walk's `if changed`
  block). A node gets a count of the grids it places; the same block
  sets a `grids_moved` flag while the count is non-zero. The pass
  refreshes each placed grid's cached world and inverse matrices, and
  a grid whose matrix changed re-enumerates under every viewer instead
  of the dirty ones. Both gates must learn the flag: `needs_pass`
  (today `regions_moved` only) and `pass`'s `if !dirty.is_empty()`
  guard around `enumerate_grids`, or a moved grid under still cameras
  is never re-tested. An animated grid node then costs a
  re-enumeration per frame it moves, what a moving viewer costs.
- Freeing. A placement node freed before its grid is destroyed ends
  the grid through `destroy_grid`'s path (`free_node`, beside
  `end_region`): resident cells leave with events, pending ones go
  through `forget_pending`, so the pending and stalled counters stay
  true. Core's `RegionGrid.stop` then tolerates a gate grid already
  gone, as `Region.detach` does for a freed node.
- Surface. The flux plugin decodes `node` (decoding only; the
  exclusivity is `GridSpec::validate`'s), `flux-types/gui/spatial.d.ts`
  declares it, core's `RegionGrid` passes it (below).
- Tests in `alloy/src/tests/spatial_residency.rs`: a translated, a
  rotated and a scaled grid, moving the node under a still viewer,
  freeing the node first, `node` with `root` refused.

Core TS: `RegionGridOptions` gains `node?: NodeId` beside `root`, and
`start(root?)` becomes `start(place?: { root?: NodeId; node?: NodeId })`
for a package that knows the node only when it enters a scene.

In the packages: 3d starts its grid with `{ node: node._node }` and
drops the identity caveat, so the `origin` doc becomes true with no
change to how cells are placed. 2d's `createRegionGrid` gains `parent`
(as its `createRegion` has), mounts the grid group there and places the
grid by that group's node; `<RegionGrid>` passes the enclosing
`<Group>` and drops its throw. `origin` then means the same on both
sides, the grid node's space (AUDIT check 3). The tile layer calls
`spatial.createGrid` directly in layer space and is unaffected.

### 2. Defaults and validation - core TS `createRegionGrid` (trivial)

Once in core: `enter` and `leave` optional, defaulting to
`DEFAULT_ENTER_CELLS` and `DEFAULT_LEAVE_CELLS` times the largest
finite cell side (the constants move from 2d, which does not export
them from its root); the checks on a cell unbounded on every axis, on
finite non-negative cell sizes, a finite origin, and an integer
6-entry `bounds`. 2d keeps only its own shape check (the 4-entry
`bounds` it converts); 3d drops its cell and origin checks. 3d's
`enter` and `leave` become optional, an additive change.

### 3. Core passes the package handle - core TS (about a day)

`RegionHandler<T, R>` is already generic in the handle, but every
option type fixes `R = Region<T>`, so each package translates by hand.
Core lets `R` be the package's handle, a type parameter (check 5):

```ts
export type RegionOptions<T, R = Region<T>> = RegionHandler<T, R> & {
  ...
  /** Make the package's handle as the region comes into being: an
   * explicit region at creation, a grid cell at its enter, before
   * `spawn`. What every handler call, hook and listener receives. */
  handle?: (region: Region<T, R>) => R
  hooks?: RegionHooks<R>
}

export type Region<T, R = ...> = {
  ...
  /** Assignable: called after every state change. */
  onStateChange?: (region: R, state: RegionState) => void
  onError?: (region: R, error: unknown) => void
}

export type RegionGrid<T, R = Region<T>> = {
  ...
  cells(): R[]
  cellAt(i: number, j: number, k: number): R | null
  onStateChange?: (region: R, state: RegionState) => void
  onError?: (region: R, error: unknown) => void
}
```

`handle` is one callback that runs when a region begins, an event with
a returned value, not an operation on the package's nodes. `createRegion`
and `createRegionGrid` take it through an overload: with `R` given,
`handle` is required; without, `R` is the core region and the handle
is the region itself, so core's own callers and tests stay as they are.
Core keeps the handle beside its region and passes it to every handler
call and hook. The listeners stay core's own slots, typed with `R`, so
core's fall-through (the region's listener, the grid's, the console)
is the only one; the package handle's `onStateChange`/`onError` are
accessors over those slots, and core reads nothing off a package
object. A package grid then reads:

```ts
let core = createCoreGrid<T, Region<T>>({
  load: opts.load, build: opts.build, unload: opts.unload,
  node: group.node, origin, cell, enter: opts.enter, leave: opts.leave,
  handle: cell => makeHandle(layer, cell, addGroup(layer, { ...at(cell), parent: group })),
  hooks: { shown, teardown: region => teardown(region, false) },
})
grid.cells = () => core.cells()
grid.cellAt = (i, j) => core.cellAt(i, j, 0)
// onStateChange/onError: accessors over core's
```

Gone from each package: `handleOf`/`nodeOf`, `coreHandler`, the
per-region and per-grid listener forwarding with the copied
fall-through, and the lookups in `shown`, `teardown`, `cells` and
`cellAt` - about 40 to 50 lines a side. A third party gets its own
handle type in every callback, which `RegionHandler<T, R>` promises
today and does not deliver. The cost is threading `R` through core's
lifecycle record, `GridImpl`, `Region.grid` and the overloads.

### 4. Shots - core TS camera-control (small)

`createShots<C>(owner | owner[], mix, options)` over `createShotBlend`,
the owner `ShotsOwner<C>` = `{ setCamera, camera, size }` (since
2026-11-05 the contract of both packages); a list pushes every owner
and reads the first's camera and size. Each package's `createShots` is
the call with its `mixCamera`: 2d passes its one target or list, 3d its
one owner (AUDIT: 2d camera drivers take one target or a list, 3d's
one). 3d's `ShotsOwner` becomes core's `ShotsOwner<CameraState>` under
the same export, which 2d's `views.md` cross-references. A third-party
camera with its own state uses it as is. TS is its layer: nothing in
Rust consumes it, and it runs per frame only during a blend.

### 5. Stays in the packages

Making, showing, hiding and destroying the package's nodes (it keeps
JS mirrors of them, 2d `_visible`, 3d `node.visible`, so core writing
the arena directly would desync them), the byte measure (sprite
records against geometry and population buffers), 3d's `warm`/`own`
holds and `buildModel`, each handle's position shape (2d
`x/y/width/height` on a layer, 3d `bounds` on a node), and the
streaming source's point shape.

The component pairs stay too. `src/components/region.tsx` shares
`held()`, `Content` and the grid's cell list, but each piece is bound
to the package's handle and scopes the package's context; `held()`
alone is five lines, and the data signal is set from `build`, where a
core helper over the region could not observe it. `<Shots>`/`<Shot>`
provide the package's context the same way. Only our two packages
could use either, so by check 5 they are shared package code, not
core.

## Public surface

All additive. Core gains `createShots`, `ShotsOwner<C>`, the `R`
parameter and `handle` on the region types, `node` on
`RegionGridOptions`, and `start`'s place bag (its one caller outside
core is 3d). 2d's `RegionGridOptions` gains `parent`; 3d's `enter` and
`leave` become optional. No package export is renamed or removed.
Engines have nothing to compare: this is where the code lives, not
what an app sees.

## Tests mirror (AUDIT check 6)

The region tests are `packages/2d/tests/region.test.tsx` and
`packages/3d/tests/residency.test.tsx`; the 3d file becomes
`region.test.tsx`. The shots cases sit in `camera2d.test.ts` and
`orbit.test.ts`; they move to `shots.test.ts` on both sides. The
placement and default tests land under the mirrored names.

## Done when

- A transformed 3d grid and a 2d grid inside a `<Group>` build their
  cells where the gate tests them, pinned by a test on each side and in
  `spatial_residency.rs`.
- Grid defaults and validation live in core only; 3d's `enter`/`leave`
  are optional.
- Neither package's `region.ts` holds `handleOf`/`nodeOf`, `coreHandler`
  or listener forwarding.
- Both `createShots` are one call over the core one.
- The region and shots tests carry one name per concept on both sides.
- `cargo test -p alloy --lib spatial_residency`, `bun run sol test
  packages/core`, `packages/2d` and `packages/3d`, and `sol check` on
  all three pass.

## Involves

`alloy/src/spatial/residency.rs`, `alloy/src/spatial/mod.rs` (the walk's
move flag, `free_node`, `needs_pass`), `alloy/src/tests/spatial_residency.rs`,
`flux/src/alloy_plugins/spatial.rs`, `packages/flux-types/gui/spatial.d.ts`,
`packages/core/src/residency.ts`, `packages/core/src/camera-control.ts`,
`packages/core/tests/residency.test.tsx`, both packages' `src/region.ts`,
`src/shots.ts`, `src/components/region.tsx`, the region and shots tests
on both sides (`packages/2d/tests/region.test.tsx`,
`packages/3d/tests/residency.test.tsx`, the shots cases in
`camera2d.test.ts` and `orbit.test.ts`), and the docs listing the grid
options and the shots owner (`packages/3d/agents/large-worlds.md`,
`packages/2d/agents/views.md`).
