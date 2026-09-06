---
title: JS test infrastructure
description: The workspace has no JS test story at all - zero test files in core, components, cli, 3d; the only automated checks are ad-hoc self-reporting scripts (packages/3d/checks, flux/examples/*_test.js) with no runner, no discovery, no CI step. Decide the runner (flux, not bun, is the runtime under test), the file convention, and the CI hook, then fold the existing rigs into it.
created: 2026-08-17
---

# JS test infrastructure

Symptom: a change to pure JS logic anywhere in the workspace - the draw-list
sort in `@solidrt/3d`, `createTextBuffer` in core, `remapPositions` in cli -
is verified by hand or by a one-off script, and nothing in CI notices when
it breaks. The Rust side has the opposite: `cargo test --workspace` over one
`src/tests/` folder per crate, run on every push.

What exists today, none of it structural:

- `packages/3d/checks/{pick,order}-check.ts` - self-reporting rigs
  (seeded PRNG, `fail()` counter, PASS/FAIL print, throw at the end), run by
  hand as `bunx srt bundle -f --stdout <file> | target/release/flux -`.
  They are tests in everything but name and harness.
- `packages/2d/checks/{camera,camera2d,frames,oversample,pick}-check.ts` -
  the same rig shape, headless on flux by keeping the checked modules free
  of core imports: `srt:events` (the pointerFrame terminator core's
  recognizers subscribe to) exists only under lattice, so anything that
  imports `createPan`/`createTransform` cannot run on the bare binary. The
  3d cameras (`createOrbitCamera`, `createFirstPersonCamera`) mix their
  motion with that glue and have no rig for that reason; the 2d camera's
  motion/input split (camera-motion.ts vs camera2d.ts) is the precedent
  for giving them one. Also: `srt check` typechecks examples and demos but
  not `checks/`, so a broken rig is only found by running it.
- `flux/examples/*_test.js` - 13 manual smoke scripts for the flux modules
  ([flux-crate-review](../notes/flux-crate-review.md) item 7 already asks to
  promote them).
- The core and cli reviews ([core-package-review](../notes/core-package-review.md),
  [cli-package-review](../notes/cli-package-review.md)) each list pure
  candidates in value order and each reach for `bun test` - which is the
  question this item exists to answer first, not assume.

## Decisions to take, in order

1. **Runner: flux.** The code under test ships on flux; a test that passes on
   bun and fails on flux (or the reverse - the `flux:*` imports that the
   reviews propose to mock away) is the wrong signal, and dogfooding the
   runtime is the standing rule. Cost: no `bun test` batteries -
   discovery, `describe/it/expect`, reporter, exit code - so a minimal
   harness has to exist. Prerequisite done:
   [flux-bin-exit-code](../done/flux-bin-exit-code.md) (the flux binary exits 1
   on any uncaught error, so a thrown assertion fails the step).
2. **Shape of the harness.** Minimal first: a `srt test [path]` command that
   discovers test files, bundles each with `-f`, runs it on the flux binary,
   and fails on nonzero exit or a FAIL marker; plus a tiny assertion module
   (`expect`-style, a dozen matchers, no mocking framework) the tests import.
   Whether that module is `@solidrt/test`, a `srt:test` builtin, or a file in
   the cli package is part of the decision; keep the standard names
   (`test`, `expect`), simplify the semantics to what our tests need - the
   usual SolidRT lens on a standard.
3. **File convention, mirroring the Rust rule** (tests never inline with
   sources): one `tests/` folder per package (`packages/<pkg>/tests/*.test.ts`),
   never inline `*.test.ts` beside sources. Excluded from `files` in
   `package.json` like `checks/` is today.
4. **CI step** in `.github/workflows/ci.yml` beside the typecheck job, once
   1-3 land: needs a flux binary, so it rides the Rust build job or a cached
   artifact - decide which.

## Done looks like

- `srt test` runs every `packages/*/tests/*.test.ts` on flux and fails CI on
  any failure.
- `packages/3d/checks/` is gone: `pick-check` and `order-check` are
  `packages/3d/tests/pick.test.ts` and `order.test.ts` under the harness
  (they need no rewrite of substance - the oracle loops become `test()`
  bodies).
- The first tests from the review notes' candidate lists exist in core and
  cli, and `flux/examples/*_test.js` are either promoted or deleted.
- One CAPTURE-based check exists for `@solidrt/3d`, the tier the pure
  rigs cannot reach (GLSL plus a scene write): `packages/3d/examples/
  fog.tsx` is the first candidate - its `pan`/`fog` debug commands park
  the camera and pick a mode deterministically, so a headless render
  (`bunx srt render`, or the control API's `/snapshot`) plus a pixel
  assertion at two coordinates (a valley pine fogged, the `fog: false`
  sun not) is the whole test. Fog shipped 2026-08-30 verified by eye
  only; this is what would catch a regression in `FOG`.

## Not this item

Rendering/snapshot tests (they need a client and the control API - the MCP
verification flow covers that by hand today) and Rust test structure
(exists, fine).
