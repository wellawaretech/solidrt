---
title: Declare the sol:* runner modules in @solidrt/flux-types
description: The sol:* module declarations lived in core's runtime-modules.d.ts and reached a program only through core's index; a bare type import of a core subpath (@solidrt/core/gpu from a bare-flux test) failed with TS2307 on sol:dev. They now ship in @solidrt/flux-types under lattice/, so they arrive through tsconfig types like the flux:* ones.
created: 2026-10-15
completed: 2026-10-15
---

# Declare the sol:* runner modules in @solidrt/flux-types

## Symptom

`sol check` on `packages/3d/tests/joints.test.ts` and `splat-data.test.ts`
(bare-flux tests) failed with

```
../core/src/gpu.ts(62,30): error TS2307: Cannot find module 'sol:dev'
```

after `gpu.ts` gained `import { registerStat } from "sol:dev"`. The tests
only `import type` from `@solidrt/core/gpu`, so the typechecker loaded
`gpu.ts` alone. The `sol:*` declarations sat in
`packages/core/src/runtime-modules.d.ts`, reached through a `/// <reference
path>` in core's `types.d.ts`, which only enters a program through core's
index. Every core subpath export that imports `sol:*` (`gpu`, `camera`) had
the same hole; the first bare type import of one exposed it.

## Decision

Ambient module declarations belong on the tsconfig `types` path, where the
`flux:*` ones already are: a program then has them whatever it imports
first. The `sol:*` surface is lattice's, not flux's, so the question was
where on that path it goes.

- A package of its own (`@solidrt/lattice-types`, the exact mirror of
  flux-types): the layering-pure answer. Rejected as overkill for ~240
  lines: a third `types` entry every user must understand, a publish
  target, devDependencies in five manifests, for a name boundary only.
- Core's `package.json` `types` field (the `@solidrt/cli` trick for
  `assets.d.ts`): core has an `exports` map, and a `types: ["@solidrt/core"]`
  directive resolves through `exports["."]`, dragging all of core into
  every program. Rejected.
- Into `@solidrt/flux-types` under `lattice/`: chosen. The package already
  declares surface a given build may lack (the `gui/` section, with
  `Flux.capabilities` as the runtime answer); the runner's modules are the
  same kind of conditional one layer up. Every lattice-targeted project
  names this package already, so nothing else changes.

## What moved

`packages/core/src/runtime-modules.d.ts` became
`packages/flux-types/lattice/{events,app,dev,apps,render,test}.d.ts`, one
file per module as the package lays out the rest, referenced from
`index.d.ts` in a section that names the layer. Core's `types.d.ts` lost
the reference; the package README states the widened scope. No tsconfig,
manifest or release change.

A bare-flux script now sees `sol:*` declared though an import fails to
resolve there, as a non-gui build already sees `flux:gpu`.
