---
title: Test harness - flux:test, srt:test and srt test
description: Tests for flux programs, SolidRT apps and our own packages, run on our own runtime and deterministic by construction - a base layer on the flux binary (flux:test - describe, test, expect, a stepped clock, settle) and an app layer on the headless SolidRT runtime (srt:test - mount, find, input, frames, reading), behind one command, srt test. The test owns the clock; nothing waits on wall time. Supersedes the JS test infrastructure backlog item; the ten bun test files and the checks/ rigs are its first consumers.
created: 2026-08-17
---

# Test harness - flux:test, srt:test and srt test

Grown out of the backlog item "JS test infrastructure" (2026-08-17), which
asked for a runner and a file convention for the workspace's own JS. The
shape below was decided 2026-09-29 and widens it twice: down to flux programs
that have no window, and up to SolidRT apps an author wants to test.

## Problem

A change to JS logic anywhere in the workspace is verified by hand or by a
one-off script, and nothing in CI notices when it breaks. The Rust side has
the opposite: `cargo test` over one `src/tests/` folder per crate, on every
push. An app author has no way to test an app at all; verification is by
hand over MCP.

What exists, none of it structural:

- Ten test files on `bun:test` (`packages/3d/tests` 7, `packages/router/tests`
  2, `packages/core/tests` 1; 1309 lines). Bun is not the runtime the code
  ships on, and no CI job runs them.
- The `checks/` rigs (`packages/core/checks` 6, `packages/2d/checks` 8,
  `packages/3d/checks` 9): self-reporting scripts with a `fail()` counter
  that throw at the end. Twenty are pure and run by hand as
  `bunx srt bundle -f --stdout <file> | target/release/flux -`; four need a
  GPU and run as `bunx srt render <file> --duration <s> --size 128x128`
  (`gpu-lease-check.tsx` in core, `collision-check.tsx` in 2d and 3d,
  `raycast-check.tsx` in 3d). `srt check` does not typecheck `checks/`, so
  a broken rig is only found by running it.
- `flux/examples/*_test.js`: manual smoke scripts for the flux modules
  ([flux-crate-review](../notes/flux-crate-review.md) item 7 asks to promote
  them).
- Timing is asserted against the wall. `packages/core/checks/gesture-check.ts`
  sleeps on real timers and compares with tolerances, which is the
  flakiness a stepped clock removes.

The candidate lists for first tests are in
[core-package-review](../notes/core-package-review.md) and
[cli-package-review](../notes/cli-package-review.md).

## Layers

| layer | module | runs on | adds |
| --- | --- | --- | --- |
| base | `flux:test` | the flux binary | `describe`, `test`, `expect`, `clock`, `settle` |
| app | `srt:test` | the SolidRT runtime, headless | `mount`, `find`, input, frames, reading |

`srt:test` re-exports the base, so an app test has one import. `srt test`
is the command for both.

## Decisions

**D1. The runner is our own runtime.** The code under test ships on flux; a
test that passes on bun and fails on flux is the wrong signal, and
dogfooding the runtime is the standing rule. Prerequisite done:
[flux-bin-exit-code](../done/flux-bin-exit-code.md). Rejected: `bun test`.

**D2. Two layers, flux at the base.** flux is a runtime of its own with its
own programs (servers, tools, the dev server itself), and none of them has
a window. Rejected: one SolidRT-only harness; one runtime for every test
(a flux program must be tested on the binary it ships on).

**D3. The binary follows from the imports.** A file that imports only
`flux:test` runs on the flux binary; one that imports `srt:test` runs on
the SolidRT runtime. Nothing is guessed and nothing is configured.

**D4. The command is `srt test`.** Discovery and TypeScript bundling live in
the CLI, which already bundles for flux (`srt bundle -f`). Rejected: a
`flux test` subcommand on a binary whose one job is to run a script.

**D5. Time.** A flux test runs on real time unless it uses `clock`, and then
it runs stepped; an app test is always stepped, since frames require it.
Under a stepped clock nothing advances unless the test says so. Rejected:
always stepped (a flux test talks to sockets, subprocesses and peers, and
the other side lives on wall time).

**D6. `performance.now()` stays real time and is not virtualized under
test.** It is for measuring work. Package logic takes time from the event
or the frame tick instead: [event-timestamp](../backlog/event-timestamp.md).
Rejected: a virtual `performance.now()` in test mode (a synchronous wait
loop on it would never end, and the production code would still ride the
wrong clock).

**D7. Tests run in-process.** The test runs in the JS context of the code
under test, and each `await` on a time verb hands control to the runtime's
loop. No server, no ports, no polling. Rejected: driving the app from
outside through the control API.

**D8. One vocabulary.** The app layer's records and event shapes are the
control API's: a located node reads as the `/tree` record, input takes the
`/input` event shape, `debug` and `link` match `/debug` and `/link`. What
an agent did by hand over MCP transcribes into a test.

**D9. Naming a node.** Three ways, and every verb accepts all three:

- by what the user perceives: `find({ text: "Increment" })`. The default,
  since such a test breaks when the visible behavior breaks.
- by a stable name: `find({ label: "sidebar" })`, the escape hatch for a
  node with nothing visible to find it by. `label` is a new prop on host
  elements and the word the GPU inventory already uses for a name that
  survives a reload.
- by direct reference: `app.node()` returns one value that is both the
  `ref` callback and a locator, for JSX the test writes itself. This is
  what a core test over bare views uses.

`mount` returns a locator for what it mounted, and `locator.find(...)`
searches within a subtree. Rejected: selector strings (`"#count"`, a
mini-language to parse and document; the object form also leaves
`find({ role, name })` purely additive once a semantics layer exists);
`id` as the prop name (taken: a node is `{ id: number }` and `/tree`
reports that number as `id`); a declared variable plus an assignment
callback per node (two places and a type annotation for one reference).

**D10. Locators are lazy and strict, and nothing auto-waits.** A locator is
resolved each time it is used, so it stays valid across time steps. An
action on a locator that matches several nodes throws; `findAll` is for the
plural case. Other frameworks retry because the page moves on its own;
here the tree changes only when the test steps time, so an assertion is
checked once and fails deterministically.

**D11. `settle()` is a condition.** It ends when no frame is demanded and
nothing is in flight, and fails past a cap, which catches a runaway
`onFrame`. Rejected: a wall-clock wait, which is what `srt render
--settle` is today.

**D12. An action runs the frame it lands in and no more.** Input is
frame-batched, so the tree is current when the verb returns. A gesture
with a duration takes it as an argument, since velocity depends on it.
Input goes through the real pipeline: a tap lands on whatever the hit
test finds at the node's painted center, a cover included.

**D13. Pixels are the last resort.** In order of preference: tree and props
assertions; a textual tree snapshot (independent of the GPU, diffable);
pixel probes at coordinates; image goldens with a tolerance against one
reference renderer. For an app whose tree is one texture leaf the
equivalents are draw counts, uniforms and texture reads.

**D14. Failure output is part of the product.** A failure carries the
virtual time and frame index, the tree around the target, a snapshot, the
logs since the test started and, when a `find` matched nothing, what was
there instead. Agents write and repair most tests.

**D15. Isolation.** A fresh runtime process per file; per test a fresh
mount, the clock at 0 and empty storage. An app test runs at a fixed size,
display scale and font set, as `srt render` already does.

**D16. No retries, no sleeps.** Flakiness is a bug in the test or the
harness, not a rate to manage.

**D17. Standard names, simplified semantics.** The base surface is what the
ten bun test files use, so their migration is an import change:
`describe`, `test`, `expect`, `not`, and `toBe`, `toEqual`, `toBeCloseTo`,
`toThrow`, `toBeNull`, `toContain`, `toMatchObject`, `toBeLessThan`,
`toBeLessThanOrEqual`, `toBeGreaterThan`, plus `toBeGreaterThanOrEqual`
for symmetry. No hooks and no mocking framework: none of the files uses
either.

**D18. File convention, mirroring the Rust rule.** One `tests/` folder per
package or project (`tests/*.test.ts`, `*.test.tsx`), never beside the
sources, excluded from `files` in `package.json`.

## The test surface

Base layer, on the flux binary:

```ts
import { test, expect, clock } from "flux:test"
import { serve } from "flux:http"

test("the server answers with the stored row", async () => {
  let server = serve({ port: 0, fetch: handler })
  let res = await fetch(`http://127.0.0.1:${server.port}/rows/1`)
  expect(res.status).toBe(200)
  server.stop()
})

test("a session expires after its timeout", async () => {
  let session = createSession()
  await clock.advance(SESSION_TIMEOUT_MS)
  expect(session.expired).toBe(true)
})
```

App layer, on the headless runtime:

```tsx
import { test, expect } from "srt:test"

test("a tap increments", async app => {
  let counter = await app.mount(() => <Counter />)
  await app.tap(counter.find({ text: "Increment" }))
  expect(counter.find({ text: "1" }).visible).toBe(true)
})

test("a child fills the row's remaining width", async app => {
  let main = app.node()
  await app.mount(() => (
    <view flexDirection="row" width={300}>
      <view width={100} />
      <view ref={main} flexGrow={1} />
    </view>
  ))
  expect(main.box.width).toBe(200)
})
```

| area | verbs |
| --- | --- |
| start | `app.mount(ui)`, `app.load(entry)`, `app.link(link)`, `app.debug(name, args)` |
| time | `app.frame(n)`, `app.advance(ms)`, `app.settle()` |
| naming nodes | `find`, `findAll`, `app.node()`, `locator.find` |
| input | `app.tap`, `app.drag`, keys, wheel, gamepad, in the `/input` event shape |
| reading | `text`, `box`, `props`, `visible`, tree snapshot, pixel probe, texture, GPU inventory |

`app.frame(n)` runs exactly n frames; `app.advance(ms)` runs frames until
that much virtual time has passed, timers firing frame-quantized as they
do in the runtime.

## Stages

### Stage 1 - the base layer and the command

- `flux:test`: `describe`, `test`, `expect` with the matchers of D17,
  `clock`, `settle`. The stepped clock rides what flux already has:
  `install_virtual_time` and `advance_virtual_time` in
  `flux/src/standards_plugins/time.rs`. One `advance_virtual_time` call is
  one task-queue turn (a timer re-armed by a fired callback waits for the
  next advance), so `clock.advance(ms)` walks deadline by deadline to its
  target and an interval fires as often as fits.
- Types in `packages/flux-types/modules/test.d.ts`.
- `srt test [path]` as a command folder in `packages/cli/src/test/`:
  discover, bundle each file with `-f`, run one process per file, report,
  exit nonzero on any failure. Failures name the source line (the TSX
  sourcemaps exist).
- `srt check` typechecks `tests/`.

### Stage 2 - migration on the base layer

- The ten bun test files: `bun:test` becomes `flux:test`.
- The twenty pure rigs move to `tests/` as `test()` bodies; the oracle
  loops need no rewrite of substance. `gesture-check` moves to the stepped
  clock; the parts of it that ride `performance.now()` keep their
  tolerances until [event-timestamp](../backlog/event-timestamp.md) lands.
- `flux/examples/*_test.js`: promoted to tests or deleted.
- First tests from the review notes' candidate lists in core and cli.

### Stage 3 - CI

A step in `.github/workflows/ci.yml` that runs `srt test`. It needs a flux
binary, so it rides the `test-flux` job or a cached artifact; decide which.

### Stage 4 - the app layer

Prerequisite: [event-timestamp](../backlog/event-timestamp.md).

- **Test mode in the runtime.** Headless, on the stepped clock, the test
  requesting each frame. The playback loop today is the reverse: it runs
  a fixed number of frames and returns (`run_playback_loop` in
  `alloy/src/playback.rs`). This is the largest piece and a change in
  alloy and lattice; it keeps decision D6 of
  [frame-timing](../design/frame-timing.md) (a path that never touches
  the wall).
- **`srt:test`** as a lattice builtin beside `srt:dev` and `srt:events`.
- **`label`** on host elements: a rendertree property, reported in the
  tree record and matchable by a query, so `/tree` and the MCP tools gain
  it too.
- **In-flight work.** `PendingOps` (`flux/src/pending.rs`) counts what
  keeps the engine alive, which is not yet what `settle()` needs: see
  Findings. Audit the holders and the GUI-side loads.
- Locators, the input verbs, reading, and the failure output of D14.

### Stage 5 - migration on the app layer

- The four GPU rigs move to `tests/`; the `checks/` folders are gone.
- One capture-based test for `@solidrt/3d`, the tier the pure tests cannot
  reach (GLSL plus a scene write): `packages/3d/examples/fog.tsx` is the
  first candidate. Its `pan` and `fog` debug commands park the camera and
  pick a mode deterministically, so a pixel probe at two coordinates (a
  valley pine fogged, the `fog: false` sun not) is the whole test. Fog
  shipped 2026-08-30 verified by eye only.
- The 3d cameras (`createOrbitCamera`, `createFirstPersonCamera`) mix
  their motion with glue that imports `srt:events`, which exists only
  under lattice; under the app layer they get tests without a split.
- A testing guide in `packages/cli/agents/`, and one pointer line in the
  scaffold's AGENTS.md.

## Later, additive

| feature | the shape it takes |
| --- | --- |
| the same tests on a real device | a `--client` option on `srt test`; stepping maps to the dev clock control |
| queries by role and name | `find({ role, name })`, once a semantics layer exists |
| image goldens | a `toMatchImage` matcher with a tolerance |
| recorded input as a test | a generator over `--capture` output |

## Open

- Network, calendar time and randomness in an app test: closed, fixed and
  seeded by default, or left as they are. A flux test needs the real
  network either way (`serve` plus `fetch` over loopback is the dogfooded
  way to fake a backend).
- The name `app.node()`.
- Where `flux:test` lives in `flux/src/`: it marshals no forge core and is
  no web standard, and its `describe`/`test`/`expect` half needs no Rust
  at all.
- Whether the CI step builds flux or takes an artifact.

## Done looks like

- `srt test` runs every `tests/*.test.ts` and `*.test.tsx` of the
  workspace, each on the binary its imports name, and fails CI on any
  failure.
- No file imports `bun:test`, and no `checks/` folder is left.
- A scaffolded app can carry a test that mounts a component, taps it,
  steps time and asserts on the tree, with no wall-clock wait anywhere.
- A failing test tells an agent what was on screen and when.

## Not this item

Rust test structure (exists, fine), coverage reporting, and a mocking
framework.

## Findings

- `PendingOps` counts what keeps the engine alive, and that includes
  standing holds beside work that completes by itself. Holders as of
  2026-09-29: fetch and body reads, file reads, net, p2p, websocket,
  subprocess, `serve`, timers, events, video. A listening server or an
  open websocket never releases on its own, and a pending timer under a
  stepped clock releases only when the test advances, so `is_idle()` is
  not the `settle()` condition as it stands.
- `srt render --settle` is a wall-clock sleep after the mount frame
  (`PlaybackConfig::settle`): the frame clock does not run meanwhile and
  I/O completions land.
- Virtual timers are a flux facility, not a lattice one: lattice is one
  host that drives `advance_virtual_time`, the test clock is another.
