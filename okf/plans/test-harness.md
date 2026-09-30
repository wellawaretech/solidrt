---
title: Test harness - flux:test, srt:test and srt test
description: Tests for flux programs, SolidRT apps and our own packages, run on our own runtime and deterministic by construction - a base layer on the flux binary (flux:test - test, expect, a stepped clock, settle) and an app layer on the headless SolidRT runtime (srt:test - mount, find, input, frames, reading), behind one command, srt test. The test owns the clock; nothing waits on wall time. Supersedes the JS test infrastructure backlog item; the ten bun test files and the checks/ rigs are its first consumers.
created: 2026-08-17
---

# Test harness - flux:test, srt:test and srt test

Grown out of the backlog item "JS test infrastructure" (2026-08-17), which
asked for a runner and a file convention for the workspace's own JS. The
shape below was decided 2026-09-29 and widens it twice: down to flux programs
that have no window, and up to SolidRT apps an author wants to test.

## Where this stands (2026-09-30)

Stages 1 and 2 are built and verified, uncommitted.

- Stage 1: `flux:test` (`test`, `expect`, `run`) behind the `test`
  feature on the `flux` binary, `srt test`, and the ten former bun test
  files on flux.
- Stage 2: the stepped clock (`test(name, async clock => ...)`), the
  arguments after `--`, the 19 pure rigs as test files (the four GPU rigs
  are what is left under `checks/`), and a first test of cli logic
  (`packages/cli/tests/remap.test.ts`).

`bunx srt test` at the repo root: 306 tests in 29 files, about 11 s, most
of it the real waits of the gesture tests that still ride
`performance.now()`. `cargo test -p flux --lib --features test`: 37
tests. `srt check` passes for the packages with their test files in.

To pick up: stage 3 (CI), a small decision (build flux in the job or
take an artifact), then stage 4, the app layer, which waits on
[event-timestamp](../backlog/event-timestamp.md) and now also carries
`settle()`.

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
  that throw at the end. Nineteen are pure and run by hand as
  `bunx srt bundle -f --stdout <file> | target/release/flux -`; four need a
  GPU and run as `bunx srt render <file> --duration <s> --size 128x128`
  (`gpu-lease-check.tsx` in core, `collision-check.tsx` in 2d and 3d,
  `raycast-check.tsx` in 3d). `srt check` does not typecheck `checks/`, so
  a broken rig is only found by running it.
- `flux/examples/*_test.js`: manual smoke scripts from building the flux
  modules, untracked (see Findings on what is tested where).
- Timing is asserted against the wall. `packages/core/checks/gesture-check.ts`
  sleeps on real timers and compares with tolerances, which is the
  flakiness a stepped clock removes.

The candidate lists for first tests are in
[core-package-review](../notes/core-package-review.md) and
[cli-package-review](../notes/cli-package-review.md).

## Layers

| layer | module | runs on | adds |
| --- | --- | --- | --- |
| base | `flux:test` | the `flux` binary | `test`, `expect`, `run`, the stepped clock |
| app | `srt:test` | the dev client, headless | `mount`, `find`, input, frames, reading |

`srt:test` re-exports the base, so an app test has one import. `srt test`
is the command for both. Neither module is in a shipping runtime (D20).

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
the SolidRT runtime. Nothing is guessed and nothing is configured. The
rule underneath is wider: a file that imports any gui-backed module
(`flux:gpu`, `flux:rendertree`, ...) needs the dev client, whatever test
module it uses; `srt test` applies that once such a test exists.

**D4. The command is `srt test`.** Discovery and TypeScript bundling live in
the CLI, which already bundles for flux (`srt bundle -f`). Rejected: a
`flux test` subcommand on a binary whose one job is to run a script.

**D5. Time.** A flux test runs on real time unless it takes the clock
(D27), and then it runs stepped; an app test is always stepped, since
frames require it.
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
- by direct reference: `app.ref()` returns one value that is both the
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
ten bun test files assert with: `test`, `expect`, `not`, and `toBe`,
`toEqual`, `toBeCloseTo`, `toThrow`, `toBeNull`, `toContain`,
`toMatchObject`, `toBeLessThan`, `toBeLessThanOrEqual`, `toBeGreaterThan`,
plus `toBeGreaterThanOrEqual` for symmetry. No hooks and no mocking
framework: none of the files uses either.

**D19. Tests are flat: no `describe`.** The file is the group and the test
name is a sentence that names its subject
(`test("matchPath: a param segment matches any value", ...)`). `describe`
does three jobs elsewhere and none is needed here: a scope for shared
setup hooks (there are no hooks; setup is a plain function the test
calls), a name prefix in the report (the file name is one), and running a
subset (a name filter on `srt test`). The six bun files that use it, 22
groups, use it one level deep with no hooks, as a label for the function
under test. Rejected: `describe` for familiarity (nesting plus hooks hides
what a test depends on; pytest, Go, Rust, `Deno.test` and ava are flat,
ava by refusal). `test(name, fn)` and the `expect` chain stay: a sentence
reads better in a report than a function identifier, the chain gives `not`
and expected/received messages, and it is the form agents know best.

**D18. File convention, mirroring the Rust rule.** One `tests/` folder per
package or project (`tests/*.test.ts`, `*.test.tsx`), never beside the
sources, excluded from `files` in `package.json`.

**D20. The test modules are not in a shipping runtime.** `flux:test` sits
behind a `test` cargo feature in flux, which requires `compile`; the flux
Makefile turns it on for the `flux` binary only, and `test` joins the
capabilities list so `srt test` can name a binary built without it.
`fluxrt` and `fluxc` do not carry it (`fluxrt` is built without `compile`
and cannot evaluate source at all). Lattice passes the feature through as
it does `video` and `ktx2`: `srt:test` exists in the dev client
(`solidrt-go`), not in the production runtime (`solidrt`). Rejected: a
no-op stub in the shipping runtimes, the way `srt:dev` is registered in
both today (a packed app has no reason to import a test module, so
leaving it out is cleaner than stubbing it).

**D21. The JavaScript half of `flux:test` is plain JS inside the flux
crate, embedded in the binary.** `test` and `expect` need no Rust; only
the clock and `settle` do. Rejected: a TypeScript package bundled and then
embedded (the flux build would depend on a bundling step); shipping it in
the CLI and bundling it into every test (the binary would not provide
what the `flux:` name says it does).

**D22. `flux:test` lives in `flux/src/test_plugins/`, a fourth plugin
layer** (2026-09-30). The placement rule has no slot for it: it is no web
standard and marshals neither forge nor alloy, but flux's own facilities
(the virtual timers, the pending-operation count). The root CLAUDE.md and
`flux/CLAUDE.md` each gain a line for the layer when it is built.

**D23. Test files run one after another** (2026-09-30). Running them in
parallel is additive later, as an option on `srt test`.

**D24. The tests are run by a public `run(options?)`, an async iterable
of results** (2026-09-30). `srt test` appends a few lines to a test
file's bundle that iterate it:
`for await (let result of run({ filter })) ...`. Tests run as the
consumer pulls, in registration order, so each result leaves before the
next test starts. Everything is optional and sits in one object that can
grow (`{ filter, timeoutMs }` today). The result record is
`{ name, ok, durationMs, error? }`, and it is the only record kind: a
test that does not finish within `timeoutMs` (wall time, a safety cap and
not a wait, so D16 stands) is an ordinary failed result, which also keeps
the engine from exiting cleanly on a promise that never settles. The
primitive is visible and usable without the CLI, and the appended lines
use public API only. Rejected: the module running the tests by itself once
the entry has evaluated (hidden scheduling, the filter through argv, and
the reporter moves into flux); `run` exported but left out of the types
(the declarations are the documentation); a callback,
`run(onResult, options?)` (it needs a second callback, `onStart`, to name
a test that never finishes, where the timeout result needs nothing).

**D25. Results reach the command as JSON lines on stdout** (2026-09-30).
One line per result behind a fixed prefix; every other stdout line is the
test's own console output, attributed to the test running then. The CLI
is the only reporter, so both ends of the contract live there, and one
ordered stream keeps output tied to its test (D14). A test that prints
the prefix itself would confuse the parser, so the prefix is unlikely by
construction. Rejected: a results file (the order between output and
tests is lost, and a crash leaves it partial); the child as the reporter
(no counts across files, and failures cite bundle lines, since
[runtime-sourcemap-remap](../backlog/runtime-sourcemap-remap.md) is
deliberately unbuilt); TAP (clumsy for expected and received values and a
stack, and still a parser).

**D26. Two modules today: `flux:test` and `srt:test`** (2026-09-30). The
node verbs (`find`, reading) are not SolidRT-specific (see Findings), so
a gui test part inside flux is the layering they point at; it is left
for later and is additive (see Later). Rejected: the `gui` feature on the
`flux` binary (alloy in the binary that hosts the dev server and the bake
tools); every test on the dev client (a flux program tested on a binary
it does not ship on, and a pure test needing the client build and a GL
context in CI).

**D27. A test that takes the clock runs stepped** (2026-09-30):
`test(name, async clock => { ... })`. A test function with a parameter
is handed the clock and its timers are virtual from before its body
runs; one without runs on real time. Virtual time covers only timers
registered after it is installed, so the choice has to be made before
the body, and the parameter makes it impossible to order wrongly. It
mirrors `async app =>` in the app layer. Each test starts at 0, and
timers left at its end are dropped. Rejected: an imported `clock` whose
first `advance` switches the test over (a timer the test registered
before that call is already on the real path, which is every
`createSession(); await clock.advance(...)`); an explicit first call,
`clock.install()` (forgettable, and a wrong order fails silently).

**D28. Randomness in a test is seeded with a fixed constant** (2026-09-30).
A seeded test names its seed as a constant and prints it on failure;
`srt test <file> -- <seed>` overrides it, everything after `--` reaching
the test as `flux:process` argv, as it does under `srt run`. A random
seed per run is never the default.

## The test surface

Base layer, on the flux binary:

```ts
import { test, expect } from "flux:test"
import { serve } from "flux:http"

test("the server answers with the stored row", async () => {
  let server = serve({ port: 0, fetch: handler })
  let res = await fetch(`http://127.0.0.1:${server.port}/rows/1`)
  expect(res.status).toBe(200)
  server.stop()
})

test("a session expires after its timeout", async clock => {
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
  let main = app.ref()
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
| naming nodes | `find`, `findAll`, `app.ref()`, `locator.find` |
| input | `app.tap`, `app.drag`, keys, wheel, gamepad, in the `/input` event shape |
| reading | `text`, `box`, `props`, `visible`, tree snapshot, pixel probe, texture, GPU inventory |

`app.frame(n)` runs exactly n frames; `app.advance(ms)` runs frames until
that much virtual time has passed, timers firing frame-quantized as they
do in the runtime.

## Stages

### Stage 1 - test, expect and the command, on real time (built 2026-09-30)

The smallest slice that runs real tests: no clock. The ten bun test files
and 18 of the 19 pure rigs use no timers (only `gesture-check` does), so
this slice alone moves the bun tests to flux and exercises the whole
runner (discovery, bundling, one process per file, reporting, exit codes)
before anything harder is built on it.

Design confirmed 2026-09-30.

**Step 1 - `flux:test`.**

| file | change |
| --- | --- |
| `flux/Cargo.toml`, `flux/Makefile` | the `test` feature, requiring `compile`; on for the `flux` binary only (D20) |
| `flux/src/test_plugins/mod.rs` | the module definition, registered under the feature (D22) |
| `flux/src/test_plugins/test.js` | `test`, `expect` and the runner, in plain JS (D21) |
| `flux/src/plugins/mod.rs` | registration (resolver and loader) and the `test` capability |
| `flux/src/tests/` | Rust tests that drive the module through the engine |
| `packages/flux-types/modules/test.d.ts`, `index.d.ts` | the types, which are the documentation |
| `docs/50-runtime/index.md` | the module listed |
| root `CLAUDE.md`, `flux/CLAUDE.md` | one line each for the fourth plugin layer |

No loader change is needed (see Findings): the module definition
evaluates the embedded JS in its `evaluate` step and exports what the JS
returns. When the clock arrives in stage 2, its native functions are
handed to the JS half as arguments.

Surface of this step: `test(name, fn)` registers a test; `expect(value)`
with the matchers of D17, behaving as Jest's do so that a migrated test
keeps its meaning (`toBe` is `Object.is`; `toEqual` is recursive, skips
properties that are `undefined` and tells `0` from `-0`;
`toBeCloseTo(expected, digits = 2)` passes within `10^-digits / 2`;
`toThrow` takes a message part, a pattern, an error class or nothing).
`run(options?)` runs them (D24).

**Step 2 - `srt test [path]`**, a command folder in
`packages/cli/src/test/`.

| part | design |
| --- | --- |
| discovery | `*.test.ts` and `*.test.tsx` inside `tests/` folders under the path (default: the working directory), skipping `node_modules`, build output and dot folders |
| bundling | each file on its own, for flux, `flux:*` kept external, with a sourcemap |
| process | one `flux` process per file, one after another (D23), the bundle on stdin, in the folder that holds the file's `tests/` folder |
| reporting | lines appended to the bundle print one JSON record per line behind a fixed prefix (`loaded`, one `result` per test, `done`); the command parses them, attributes the other lines to the test running then and prints the report (D25) |
| source lines | stack frames remapped with the existing `remapPositions` (`packages/cli/src/server/remap.ts`) and the bundle's sourcemap |
| name filter | an option that runs the tests whose name contains a text; what replaces a group for running a subset (D19) |
| exit | nonzero on any failure |

Two protections against a false result:

- Silent pass: a test whose promise never settles would let the engine
  exit cleanly with status 0 (see Findings). The per-test cap of D24
  holds the engine and turns it into a failed result; on top of that the
  entry prints a closing line, and a run that ends without it is a
  failure.
- Hang: a synchronous loop or a crash is out of the runner's reach, so a
  wall-clock cap per file kills a stuck process and the command reports
  the last test that did report. A safety cap, not a wait (D16 stands).

A `flux` binary built without the feature fails at the import of
`flux:test`; the command turns that into a message that says so and names
the build command.

`srt check` gains the `tests/` folders in its globs
(`packages/cli/src/check/main.ts`).

**Step 3 - the ten bun test files.** `bun:test` becomes `flux:test`. The
six that use `describe` also lose the wrapper, and each test name takes
the group's label as its prefix (D19). Mechanical. Verification: all ten
pass under `srt test`; then one assertion is broken on purpose to confirm
that the failure names the right file and line, and restored.

### Stage 2 - the clock and the rigs (built 2026-09-30)

- The clock (D27) on what flux already has: `install_virtual_time` and
  `advance_virtual_time` in `flux/src/standards_plugins/time.rs`, which
  carry their own Rust tests (`flux/src/tests/time.rs`). One
  `advance_virtual_time` call is one task-queue turn (a timer re-armed by
  a fired callback waits for the next advance), so `clock.advance(ms)`
  walks deadline by deadline to its target, with a task-queue turn
  between timers, and an interval fires as often as fits. `clock.now`
  reads the virtual time.
- The per-test cap of D24 moves off the JS timers: under a stepped clock
  a `setTimeout` never fires, so the cap rides real time natively.
- The nineteen pure rigs move to `tests/` as `test()` bodies: a rig's
  sections become its tests and its `fail(msg)` helper throws, so the
  oracle loops need no rewrite of substance. The seven seeded rigs take
  a fixed seed (D28). `gesture-check` moves to the stepped clock; the
  parts of it that ride `performance.now()` keep their real waits and
  tolerances until [event-timestamp](../backlog/event-timestamp.md)
  lands. The four GPU rigs stay in `checks/` until stage 5.
- `srt test <file> -- <args>`: the arguments after `--` reach the test as
  `flux:process` argv (D28).
- A first test from the review notes' candidate lists:
  `remapPositions` in the cli. The core candidates all import a runtime
  module and wait for the app layer (see Findings).

Not in this stage, and not this plan's: tests of the flux modules. See
the finding on what is tested where.

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
- **`srt:test`** as a lattice builtin beside `srt:dev` and `srt:events`,
  in the dev client only (D20).
- **`label`** on host elements: a rendertree property, reported in the
  tree record and matchable by a query, so `/tree` and the MCP tools gain
  it too.
- **`settle()`, for both layers** (moved here from stage 2 on
  2026-09-30: nothing before the app layer needs it). `PendingOps`
  (`flux/src/pending.rs`) counts what keeps the engine alive, which is
  not yet what `settle()` needs: see Findings. The flux half is a
  classification of its 26 hold sites in 13 files into work in flight
  (fetch, body and file reads, connects and binds, a subprocess stdin
  write, video open, the generic async-op wrapper) and standing holds (a
  listening server, an open socket or stream, a pending accept, a running
  child, event listeners, timers); `settle()` resolves once nothing is in
  flight and the job queue has drained, bounded by the test's own cap.
  Then audit the GUI-side loads.
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
| a gui test part in flux | the node verbs (`find`, `findAll`, reading, snapshots) compiled under `test` + `gui` in flux; `srt:test` re-exports it and keeps what is Solid (`mount`, `load`, `link`, `debug`) (D26) |

## Open

- Network, calendar time and randomness in an app test: closed, fixed and
  seeded by default, or left as they are. A flux test needs the real
  network either way (`serve` plus `fetch` over loopback is the dogfooded
  way to fake a backend).
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

- The node verbs are not SolidRT-specific. `flux:rendertree` and the text
  query behind `/tree?query=` (`snapshot_matches`, reached through
  `flux::gui::tree::with_tree`) are in flux's `gui` layer already; only
  the `/tree` record shaping (`node_json` in
  `lattice/src/go/connection.rs`) sits in lattice. The `flux` binary is
  built without `gui`, and lattice is the only host that turns it on.
  Whether the frame and input verbs can sit in flux too is unchecked:
  lattice drives the loop and the dispatch into Solid.
- A `fail()` that throws is caught by its own `try`. The rigs wrote a
  must-throw check as `try { f(); fail("must throw") } catch (e) { ... }`,
  which only worked while `fail()` counted. Fourteen such sites in eleven
  rigs would have passed silently after the move; each now records
  whether the call threw and checks it after the `try`. Anyone turning a
  counting helper into a throwing one has this trap.
- A `fail()` typed `never` narrows what follows it: after
  `if (count !== 1) fail(...)` the compiler holds `count` to be 1 and
  rejects a later `count !== 2`, though a callback changed it in between.
  The helpers are typed `void`.
- The stepped clock resolves to the millisecond: the long-press test
  asserts nothing fired at 499 ms and the press at 500, where the rig
  waited 400 and 600 ms of wall time with nothing in between. The
  double-tap, the pan and the swipe still wait on real timers, because
  they read `performance.now()`: event-timestamp removes that.
- None of the core candidates in
  [core-package-review](../notes/core-package-review.md) runs on the
  bare flux binary: `parseColor` imports `flux:rendertree`,
  `createTextBuffer` imports the window and layout bindings. They need
  the app layer, or a split of the pure part from the binding.
- The dispatch tests in 2d and 3d share one fake layer across their
  tests and depend on running in order, as the rigs did; a `--filter`
  run of one of them starts from a different state.
- What is tested where (2026-09-30). forge's tests cover the capability
  logic. `flux/tests/*.rs`, 172 cargo integration tests in 21 files, run
  JS source through the real engine and assert on its log, which is the
  marshalling path; they stay in cargo, since they test the runtime the
  test runner stands on and some assert what a test inside the engine
  cannot see (how the process exits). `srt test` is for code written in
  JS: the packages, apps and flux programs, not the flux modules. The
  first real-time I/O test under `srt test` is therefore a flux program,
  and the one there is is the dev server (`packages/cli/src/server`,
  the second tier in [cli-package-review](../notes/cli-package-review.md));
  its own piece of work. subprocess, p2p and ffi have no file in
  `flux/tests/`; they belong there. `flux/examples/*.js` are untracked
  scratch from building each module, neither tests nor documentation,
  and stay as they are; a maintained examples set is item 8 of
  [flux-crate-review](../notes/flux-crate-review.md).
- The "runtime-free entry" tests (`model-data`, `splat-data`, `textures`,
  `joints`) proved under bun that no `flux:*` import had crept into an
  entry a bake script loads under bun. On the `flux` binary the proof is
  narrower: a gui or `srt:` import still fails to link, a headless
  `flux:` import (`flux:fs`, `flux:image`) does not. Closing that gap
  needs the bundle's import list, which `srt test` has and a test does
  not; open.
- Bringing `tests/` into the packages' typecheck programs (they were
  excluded while they ran under bun) surfaced one type error, in
  `packages/3d/tests/invert.test.ts`, fixed with the migration.
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
- A flux module can be written in JS without touching the loader. Every
  module registered today is a native definition (`ModuleDef`), and its
  `evaluate` step receives the context, so it can evaluate an embedded
  source and export the values that source returns. rquickjs 0.14 also
  accepts a tuple of loaders, which would let a source loader sit beside
  the native one; not needed.
- The engine loop ends when the job queue is drained and no operation is
  pending (`FluxEngine::run`). A promise that never settles holds nothing,
  so a script waiting on one exits with status 0. A test runner on flux
  must treat an unfinished run as a failure.
- `fluxrt` is built without the `compile` feature and cannot evaluate
  source (`eval_source` and `ModuleCode::Source` sit behind it), so a
  module embedded as source cannot load there in any case.
- The ten bun test files import pure modules only. `srt test` cannot run
  code that calls Bun APIs, which covers parts of the CLI (the bundler,
  process spawning); pure CLI logic is fine.
- `packages/core/tests/textures.test.ts` doubles as the proof that
  `@solidrt/core/textures` imports no runtime module. The proof holds on
  the bare `flux` binary as it does on bun: a GUI or `srt:*` import would
  fail to bundle or to link.
