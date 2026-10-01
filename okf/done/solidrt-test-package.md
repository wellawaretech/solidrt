---
title: Move the app test layer into @solidrt/test
description: The app test layer (mount, find, input, frames, readers) leaves core for a package of its own, @solidrt/test, a devDependency of every app and a peer of core like the router; core's surface and its auto-imported AGENTS.md go back to being the app's alone. No runtime or Rust change.
created: 2026-10-01
completed: 2026-10-01
---

# Move the app test layer into @solidrt/test

## Problem

An app test imports `test` and `expect` from `@solidrt/core/test` (D31 in
`okf/done/test-harness.md`). That puts test code in core:

- An app declares its test API as a runtime dependency. The bundle is not
  affected (only an imported module is bundled), but `dependencies` no
  longer says what ships.
- Core's AGENTS.md carries a 67-line "Testing an app" section, and the
  scaffold auto-imports that file into every agent session of every app,
  whether it writes a test or not.
- Test APIs are usually packages of their own (`@playwright/test`,
  `@testing-library/solid`), and the name lines up with `srt test`,
  `flux:test` and `srt:test`.

D31 settled the layering (the Solid half in TypeScript, over the native
`srt:test`) and rejected `srt:test` as the import; a separate package was
not weighed. This item changes only where that TypeScript lives.

## Done looks like

```tsx
import { test, expect } from "@solidrt/test"
```

- `packages/test` is published as `@solidrt/test`, a peer of
  `@solidrt/core` (pinned to the same version at release, like the
  router), and a devDependency of the scaffold.
- `@solidrt/core/test` is gone (no alias: no backwards compatibility).
- The tests of the layer itself live in `packages/test/tests/`.
- Same test count before and after, every layer green.

## Why it is mechanical

Checked before writing this down:

- `packages/core/src/test.ts` imports `flux:test`, `flux:test/gui`,
  `srt:test` and `./renderer` (`createElement`, `insert`, `render`). Core
  already exports all three from its root (`export * from "./renderer"`),
  so the package imports them from `@solidrt/core` and core opens nothing
  new. Nothing in core imports `test.ts`.
- One core instance (D31's requirement: `mount` runs in the bundle's own
  copy of core). In this repo, Bun's bundler keys modules by real path: a
  file reached through the workspace symlink and the same file reached
  relatively is one module (checked with a scratch bundle). So core's own
  tests may mix `../src/...` with `@solidrt/test` -> `@solidrt/core`. In
  an install, the exact peer pin keeps one copy; a mismatched manual install
  could nest a second core, the same exposure router, 2d, 3d and components
  have today.
- `srt test` needs no change: it finds every `tests/` folder by walking
  (`packages/cli/src/test/main.ts`, `discover`), so `packages/test/tests/`
  is picked up; it classifies a file by the runtime modules its bundle
  imports (`APP_MODULE`, `runtimeImports`), so a `.ts` file importing
  `@solidrt/test` is still an app test (its bundle imports `srt:test`).
- The release pin script rewrites every `@solidrt/*` specifier in all four
  dependency fields of every `PUBLISH_DIRS` package and of the scaffold's
  package.json, so adding the directory to `PUBLISH_DIRS` is the whole
  release change.
- `srt init` prunes unchosen extensions from `dependencies` only;
  `create-solidrt` forwards to `srt init`. Neither changes.
- No Rust, no native surface, no `srt:test` change. The website takes
  `src/test/docs.md` as it is and lists no packages.

## Decided here

- The `srt:test` declarations stay in core's `runtime-modules.d.ts`, with
  the other `srt:*` modules: the native surface is declared in one place,
  only the TypeScript layer moves, and the package sees the declarations
  through its peer's types. (The other way: an ambient `.d.ts` in
  `packages/test` referenced from `src/index.ts`, so core holds no trace of
  testing, at the price of `srt:*` declared in two packages.)
- Core's "Testing an app" section moves out whole, with no pointer left
  behind: the scaffold's AGENTS.md index already routes test work to
  `@solidrt/cli/agents/testing.md`, and the API section becomes
  `packages/test/AGENTS.md`.
- The package ships `src/` and `AGENTS.md`, no README, like core.

## What moves and what changes

New package, `packages/test/`:

| File | From |
|---|---|
| `package.json` | new: `@solidrt/test`, `main`/`exports` `./src/index.ts`, `files` `src/` + `AGENTS.md`, peer `@solidrt/core: workspace:*`, dev `@solidrt/flux-types: workspace:*` |
| `tsconfig.json` | `packages/router/tsconfig.json` (flux-targeted) |
| `src/index.ts` | `git mv packages/core/src/test.ts`; `./renderer` -> `@solidrt/core` |
| `AGENTS.md` | `packages/core/AGENTS.md`, "Testing an app" (lines 698-764) |
| `tests/app.test.tsx` (12), `readers.test.tsx` (7), `settle.test.tsx` (6), `sandbox.test.ts` (2), `fixtures/entry.tsx` | `git mv` from `packages/core/tests/`: they test the layer itself; `../src/test.ts` -> `../src/index.ts`, core's `../src/index.ts` (and the fixture's `../../src/index.ts`) -> `@solidrt/core` |

Import changes only (`@solidrt/core/test` or `../src/test.ts` ->
`@solidrt/test`), the tests of a feature that stay with it:

- `packages/core/tests/gesture-timers.test.ts` (3), `gpu-lease.test.tsx`
  (5), `input-map-hold.test.ts` (1)
- `packages/2d/tests/collision.test.tsx` (5)
- `packages/3d/tests/fog.test.tsx` (3), `collision.test.tsx` (6),
  `raycast.test.tsx` (7), `camera-components.test.tsx` (2)

Manifests and wiring:

- `packages/core/package.json`: drop the `./test` export; devDependency
  `@solidrt/test: workspace:*` (its tests import it; a dev-only cycle,
  which Bun workspaces allow).
- `packages/2d/package.json`, `packages/3d/package.json`: devDependency
  `@solidrt/test: workspace:*`.
- Root `package.json`: `@solidrt/test: workspace:*` beside the others.
- `.github/workflows/release.yml`: `packages/test` in `PUBLISH_DIRS`, after
  `packages/router` (published after core, which it peers).
- `.github/workflows/ci.yml`, "Typecheck packages": `packages/test` in the
  list.
- `packages/cli/src/init/scaffold/package.json`: devDependency
  `@solidrt/test: 0.0.0` (the placeholder; the release pins it).
- `bun.lock`, by `bun install` (workspace entries only).

Docs and comments naming the old import:

- `packages/cli/agents/testing.md`: the header's reference path (lines
  5-7), the layer rule (28), the example (59).
- `packages/cli/src/test/docs.md`: lines 105, 110, 126.
- `packages/cli/AGENTS.md:76`, `packages/cli/agents/debugging.md:167`,
  `packages/3d/AGENTS.md:561`.
- `packages/cli/src/init/scaffold/AGENTS.md:62-63`: the test line also
  names `node_modules/@solidrt/test/AGENTS.md`.
- Comments: `packages/core/src/runtime-modules.d.ts:241`,
  `packages/core/src/types.d.ts:400`, `packages/flux-types/gui/test.d.ts:5`,
  `flux/src/test_plugins/gui.rs:2`, `lattice/src/plugins/test.rs:12`.
- Root `CLAUDE.md:69`.
- `okf/done/test-harness.md`, D31: one closing line pointing here (in
  `done/` by then, if the plan has closed).

## Verification

1. Before anything moves: the `bun run srt test` total, per layer.
2. `bun install`; the `bun.lock` diff holds the new workspace member only.
3. `bun packages/cli/bin/srt check` on `packages/test/src/index.ts` and on
   one test file each in core, 2d and 3d: "Types OK".
4. `bun run srt test`: the same total, both layers green. The app layer runs
   on the dev client as built: no Rust change, so no rebuild.
5. `git grep "core/test"` finds only the harness plan's history.
6. `bun pm pack` in `packages/test`: the tarball holds `package.json`,
   `src/` and `AGENTS.md` and nothing else (tarball deleted after).
7. `bun scripts/build-okf-index.ts --check`.

The release itself cannot run locally; step 6 and the `PUBLISH_DIRS` entry
are what it rests on.

## Not in this item

- A guard against two copies of core in one bundle (core throws when a
  second copy loads). It would protect every layer package, not this one in
  particular, so it is its own item if wanted.

## Built (2026-10-01)

As planned, with three deviations:

- `srt test` did change after all. The classification bundle of a plain
  `.ts` test (`bundleFlux`, Bun's own bundler) now reaches core's root
  through `@solidrt/test`, and core's root exports `Logo` from `logo.tsx`:
  Bun's JSX transform asked for `@solidrt/core/jsx-dev-runtime`, which core
  does not ship, and the three `.ts` app tests failed to bundle. The runner
  now takes a `.tsx` anywhere in the closure as what settles the layer
  (`packages/cli/src/test/main.ts`, `bundleFlux`). A `.ts` app test that
  imported core's root had the same exposure before this item.
- CI's "Typecheck packages" step reads its list from the tree
  (`packages/*/tsconfig.json`, `apps/*/tsconfig.json`, plus the cli's dev
  server) and fails on a package with TypeScript sources but no
  `tsconfig.json`: the router had been missing from the hand-kept list.
  `packages/create-solidrt` got the tsconfig (bun-targeted, like the cli)
  and the `@types/bun` devDependency that lets it typecheck.
- The lockfile diff also carries the devDependency lists of core, 2d, 3d
  and the root, not the new member alone; `fog.test.tsx` has 4 tests, not 3.

Totals: 370 tests in 41 files before and after, every layer green.
