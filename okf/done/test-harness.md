---
title: Test harness - flux:test, srt:test and srt test
description: Tests for flux programs, SolidRT apps and our own packages, run on our own runtime and deterministic by construction - a base layer on the flux binary (flux:test - test, expect, a seeded Math.random; real time) and an app layer on the headless SolidRT runtime (@solidrt/core/test - mount, find, input, frames, reading; stepped by frames, no wall time), every test in an engine of its own, behind one command, srt test. Supersedes the JS test infrastructure backlog item; the ten bun test files and the checks/ rigs are its first consumers.
created: 2026-08-17
completed: 2026-10-01
---

# Test harness - flux:test, srt:test and srt test

Grown out of the backlog item "JS test infrastructure" (2026-08-17), which
asked for a runner and a file convention for the workspace's own JS. The
shape below was decided 2026-09-29 and widens it twice: down to flux programs
that have no window, and up to SolidRT apps an author wants to test.

## Where this stands (closed 2026-10-01)

Every stage is built and everything under "Done looks like" holds: 369
tests in 41 files behind `bunx srt test`, no `bun:test` import and no
`checks/` folder left, a scaffolded app can carry a mounting, tapping,
stepping test, and a failure says what was on screen and when. Stages
1, 2, 2b and 3 are committed and the CI jobs are green on all four
platforms (two rounds of fixes on 2026-10-01, under
[Stage 3](#stage-3---ci-written-2026-09-30-first-run-2026-10-01));
stage 5 landed the same day. What the work found that outlives it is in
[test-harness-findings](../notes/test-harness-findings.md); what is
still open went to [ideas](../ideas.md) and [tiny](../tiny.md). Stage 4 was redesigned on 2026-09-30 under the rule "the
best solution, not the least effort; no backwards compatibility" (D30 to
D35, [Stage 4](#stage-4---the-app-layer-redesigned-2026-09-30)), and all
six of its steps are built (4.5 and 4.6 on 2026-10-01, uncommitted):

- Step 4.1, the test host: every test runs in an engine of its own (D30).
  `flux --test` is the host, `run` and the appended runner lines are gone
  (D24 and D4 reversed), a test's output travels inside its record (D25
  amended), and an uncaught error fails the test it happened in.
- Step 4.2, test mode in the runtime: alloy's stepped mode, the step in
  lattice, the frozen wall, `solidrt-go --test`, `@solidrt/core/test`
  with `app.frame`, `app.advance` and `app.time`, and `srt test` choosing
  the binary by the bundle's imports. The four parked timer tests are
  back in `packages/core/tests/` as app tests, their thresholds asserted
  to the millisecond.

- Step 4.3, the verbs: `app.mount` and `app.load`, locators (`find`,
  `findAll`, `app.ref()`), reading (`text`, `box`, `props`, `visible`,
  `record`), input (`tap`, `drag`, `key`, `type`, `input`) through the
  shared input plan, `label` on every element, the node verbs in flux,
  and the sandbox emptied per engine. The control API changed with it
  (D37): a tap's up is a frame after its down, a mouse gesture moves
  first, `drag` is an action, `/tree?at=x,y` is the hit test as a read,
  and the tree query matches a label.

- Step 4.4, `settle()`: "the app is at rest" as a runtime facility with
  three hosts (D39): `settle()` in a flux test, `app.settle()` in an app
  test, and the control API's `/settle` with the MCP `settle` tool on a
  running client. Under it, flux's holds on the engine are typed (work in
  flight or standing), named by kind and released on drop (D40), and the
  frame protocol records why it asked for the next frame, so a settle
  that fails says what is left.

- Step 4.3's rest (2026-10-01): `app.link`, `app.debug`, a synthetic
  gamepad per engine, and the readers `outline()` (named `tree()` until
  2026-10-01), `pixel()`/`pixels()` and `app.gpu()`.

- Step 4.5, failure output, docs, CI (2026-10-01): a failed test's record
  carries details read in its engine (what is in flight; for an app test
  the app time and frame, what still demanded frames, the whole outline
  and a snapshot of the frame written under `dist/test/`), `srt test --only
  flux|app`, the `test-app` CI job, the testing guide
  (`packages/cli/agents/testing.md`).

- Step 4.6, render on the stepped mode (2026-10-01): alloy has one
  headless mode; `srt render` is a host in lattice that steps and reads
  the window back, with the wall frozen and `Math.random` seeded,
  `--settle` the condition of D11, `--strict` and `--seed` passed through.

`bunx srt test` at the repo root: 341 tests in 35 files, 7 to 12 s
depending on what else the machine does. `cargo test -p flux --lib
--features test,gui`: 101 tests; alloy 637 (the `always_render` test went
with the gate bypass), lattice 66. `srt check` passes for core, cli and
router. Renders of `examples/hello-world`, `examples/spin` and an
animating probe are byte-identical between the old lockstep loop and the
render host.

Stage 5 (2026-10-01): the four GPU rigs are app tests, the `checks/`
folders are gone, the fog example has a capture test, and the camera
components have tests on a real drag and key. `bunx srt test` at the
repo root: 369 tests in 41 files, about 28 s. Everything under "Done
looks like" holds.

The MCP bridge's `settle` tool and the `at` and `drag` arguments were
verified through a re-spawned bridge on 2026-10-01 (the fog example:
`settle` names the standing `onFrame`, `at` returns the hit path, a
`drag` cycles the mode).

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
| base | `flux:test` | the `flux` binary | `test`, `expect`, a seeded `Math.random` |
| app | `@solidrt/core/test`, over the native `srt:test` | the dev client, headless | `mount`, `find`, input, frames, reading |

`@solidrt/core/test` re-exports the base, so an app test has one import
(D31). `srt test`
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

**D4. The command is `srt test`.** Discovery, TypeScript bundling and
reporting live in the CLI, which already bundles for flux (`srt bundle
-f`). Amended 2026-09-30 (D30): the binary has a `--test` mode after all,
since the engine loop of a test file is native; it rejected "a `flux test`
subcommand on a binary whose one job is to run a script".

**D5. Time: a flux test lives on the wall, an app test has no wall**
(reworded 2026-09-30; the date row changed by D34). The line follows the module a test imports, as the
binary does (D3).

| | flux test (`flux:test`) | app test (`srt:test`) |
| --- | --- | --- |
| what drives time | the wall | frames, which the test requests |
| timers | real | stepped with the frames |
| `performance.now()` | real | 0 |
| `Date.now()` | real | a fixed epoch plus frame time (D34) |
| `Math.random()` | seeded | seeded |

In a flux program time matters: a monotonic clock is a legitimate input
of a server (a latency, a rate limit, a deadline), and the other side of
a socket lives on wall time. In an app the frame is the only clock, and a
test that reads the wall cannot be repeated. `srt render` and playback
are in the right-hand column by the same reasoning, and do not yet
freeze anything. A test of SolidRT logic that needs time to pass is an
app test, whether or not it needs a window; a pure package test stays a
flux test and takes every time it uses as an input. Rejected: a stepped
clock in a flux test (D27); freezing `performance.now()` in every test
(it breaks correct flux programs; measured, it fails exactly the six
tests that are app tests by this rule, and forcing `Date.now()` to 0
fails none).

**D6. `performance.now()` is never virtual: real in a flux test, 0 in an
app test** (reworded 2026-09-30; it read "stays real time" for both
layers). It is for measuring work. SolidRT logic takes time from the
event or the frame tick instead:
[event-timestamp](event-timestamp.md). At 0, logic that still
reads it fails the same way on every run instead of passing within a
tolerance. Rejected: a `performance.now()` that follows the stepped
clock (a synchronous wait loop on it would never end, and the production
code would still ride the wrong clock while its tests pass).

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
--settle` is today. Made precise by D39 to D41.

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
the seed of `Math.random` does. Rejected: a TypeScript package bundled and then
embedded (the flux build would depend on a bundling step); shipping it in
the CLI and bundling it into every test (the binary would not provide
what the `flux:` name says it does).

**D22. `flux:test` lives in `flux/src/test_plugins/`, a fourth plugin
layer** (2026-09-30). The placement rule has no slot for it: it is no web
standard and marshals neither forge nor alloy, but flux's own facilities
(the seed of `Math.random` today). The root CLAUDE.md and
`flux/CLAUDE.md` each gain a line for the layer when it is built.

**D23. Test files run one after another** (2026-09-30). Running them in
parallel is additive later, as an option on `srt test`.

**D24. Reversed 2026-09-30 by D30: there is no `run`.** The text below is
what was decided and built first. *The tests are run by a public
`run(options?)`, an async iterable of results* (2026-09-30). `srt test` appends a few lines to a test
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

**D25. Results reach the command as JSON lines on stdout** (2026-09-30;
amended the same day by D30: the host prints the records, and what a test
logged travels inside its record, collected by its engine's log sink, so
attribution no longer rests on line order). One line per result behind a
fixed prefix; every other stdout line is the
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

**D27. `flux:test` has no clock** (2026-09-30, reversing the decision of
the same day that a test taking a `clock` parameter runs stepped, which
was built in stage 2). Stepping only the timers of a flux test left it
with two clocks that disagree, stepped timers beside a real
`performance.now()` and `Date.now()`, and the tests it was built for are
app tests under D5. A flux program with a timeout is tested by waiting
for it, or takes its delay as a parameter. Stepping is the app layer's,
on frames (`app.frame`, `app.advance`). Rejected: moving
`performance.now()` and `Date.now()` with the stepped clock as well (a
second time model inside the layer whose point is real time).

**D28. `Math.random` is seeded under test** (2026-09-30, replacing the
first form of the same day, in which each test file named a seed of its
own and carried a generator). Importing `flux:test` seeds `Math.random`,
and every test draws the seed's sequence from its start, so random inputs
do not depend on the tests before or on a filter. It is the standard
function and no test API: a test writes `Math.random()`, and code under
test that calls it (the camera shake does) is reproducible with it. The
seed is a fixed constant of the harness; `srt test --seed <n>` runs the
same tests on another sequence and says so in its last line. A random
seed per run is never the default. Everything after `--` still reaches
the test as `flux:process` argv, as it does under `srt run`. Rejected: an
imported `random` (a second source of randomness that the code under
test never sees); a generator and a seed constant per test file (what
the rigs had: seven copies of the arithmetic).

**D29. Seeding is a facility of flux, and a seeded context keeps the
engine's generator in kind** (2026-09-30). `seed_random(ctx, seed)`
(`flux/src/standards_plugins/random.rs`) sits beside
`install_virtual_time`: a host opts a context in, and `flux:test` is one
host. QuickJS seeds its own `Math.random` from the clock and has no call
to seed it, so a seeded context gets flux's function: the same
xorshift64* and 52 bits a value, the state started from the seed through
splitmix64 (so seeds 1 and 2 are unrelated). A context nobody seeded
keeps the builtin, which is every shipping app: the seeded function is a
native call and the builtin is not. An isolate spawned by a seeded
context is seeded with a seed derived from its parent's seed and its
place among the children, without drawing from the parent's sequence.
The sequence of a seed is pinned by a test against an independent
implementation, since a seed somebody noted down has to mean the same
inputs later and elsewhere. Rejected: replacing `Math.random` in JS
inside `flux:test` (a 32-bit generator where the engine has 52 bits, and
out of reach for isolates, `srt render` and the app layer); flux's
function in every context (a native call per `Math.random()` in shipping
apps, for a test feature). `srt render` and playback are hosts that do
not seed yet:
[seeded-random-headless-render](../backlog/seeded-random-headless-render.md).

**D30. Every test runs in an engine of its own, in both layers**
(2026-09-30). The host evaluates the file once to list the tests it
registers and once more for each test, in a fresh engine that runs that
test and nothing else. A test starts from the file's freshly evaluated
module state, so a store, a listener or a timer of one test is not there
for the next: a result does not depend on the tests before it or on a
filter, the rule D28 set for `Math.random`. It also closes what was open
after stage 2: an uncaught error is the error of the test whose engine
raised it, a timed-out test is dropped with its engine (a synchronous
loop included, through the engine's interrupt), and a test waiting on a
promise nothing settles fails at once, when its engine runs out of work.
In the app layer it removes the need for an unmount in core (`render()`
stays once per engine) and lets a test load an app entry. The host is
native because engines are: one function in flux (`flux::test`), used by
the `flux` binary's `--test` mode and by the dev client's. Cost, measured
on release: about 10 ms per test for an 80 KB bundle; the 310 tests went
from about 5 s to 6.5 s. Compiling a file to bytecode once was considered
for that and not built: the overhead is small. Rejected: one engine per
file with an unmount between tests (module state leaks, which the 2d and
3d dispatch tests already showed nobody notices); a process per test (the
dev client's GL setup per test).

**D31. An app test imports `@solidrt/core/test`** (2026-09-30). `mount`
has to run the UI in a Solid root of the test bundle's own copy of Solid
and core, which a native module cannot reach, so the Solid half is
TypeScript in core, over a native `srt:test` that holds the engine verbs.
The same pattern as the rest of core: native modules are the low level,
apps import core. Rejected: `srt:test` as the import, resolved by the
bundler to core's source (the first `srt:` name that is no native module);
handing core's mount to the host through generated runner lines.

**D32. The node verbs live in flux, under `test` + `gui`** (2026-09-30,
D26's "later" done now). `find`, the readers and the `/tree` record
shaping (`node_json`, today in `lattice/src/go/connection.rs`) marshal
alloy's rendertree, which is flux's layer; the control API and the tests
then share one record. Frames, input injection and `settle()`'s frame half
stay in lattice, which owns the loop.

**D33. Test mode is a third headless mode of alloy that emits no frame
signals; a step is lattice's** (2026-09-30). `app.frame()` queues a frame
signal on the UI loop's channel and resolves after the frame verb ran.
The demand gate stays honest (a frame nobody demanded draws nothing),
which is what `settle()` reads. Rejected: building on the playback loop as
it is (it renders unconditionally and reads back every frame).

**D34. Calendar time in an app test is a fixed epoch plus frame time**
(2026-09-30, closing the open point on calendar time). A fixed date made
every elapsed-time or countdown display untestable; this is as
repeatable. The host sets `TZ=UTC`, so local-time methods read the same
on every machine. `performance.now()` stays 0 (D6). The network stays
real: `serve` plus `fetch` over loopback is the way to fake a backend, and
`settle()` waits for what is in flight.

**D35. `srt render` joins the same footing** (2026-09-30). Playback
freezes the wall and seeds `Math.random`
([seeded-random-headless-render](../backlog/seeded-random-headless-render.md)),
`--settle` becomes the condition of D11 instead of a wall sleep, and the
playback loop is rebuilt on the stepped mode (render becomes a host that
steps and captures), so alloy has one headless path. Last in the order,
once tests have proven the stepped loop.

**D36. `visible` means painted; a covered target fails the tap**
(2026-09-30, reversing the second half of D12). `visible` is true when the
node's painted quad, clipped by every clipping ancestor and the window,
still has area and the opacity multiplied up the chain is above 0: a node
scrolled out of a ScrollView is not visible. Whether something covers it
is checked where it matters: `app.tap(locator)` runs the real hit test at
the point first and throws, naming the node that is there, when the hit
path does not contain the target. `app.tap({ x, y })` lands on whatever is
there. Rejected: one loose `visible` (mounted, a box with area, own
opacity), which reads true under a modal; D12's "a tap lands on whatever
the hit test finds, a cover included" (a covered button is then a silent
wrong tap).

**D37. One gesture vocabulary, planned once** (2026-09-30). A test takes
the `/input` events as they are, and their `delayMs` and `holdMs` are app
time, which the host turns into frames. The expansion of a gesture into
events lives in one planner in lattice, shared by the control API and the
tests, and it takes the frame interval as a parameter. Three rules change
with it, for the control API too: the up of a tap comes at least one
frame after its down (a real tap is never 0 ms, and a down and up in one
batch never render the pressed state); a mouse tap or drag moves to the
point a frame ahead of the down, so hover is what a real mouse leaves; and
`drag` is an action of `/input` (`to`, `durationMs`), one move per frame.

**D38. The sandbox is emptied before every engine** (2026-09-30). The
data folder and the fetch cache, by the host, ahead of each engine of a
file's run, the listing included. With an engine per test the file is
evaluated after the wipe, so a database opened at module level is opened
in the engine that uses it. A flux test gets no sandbox: it runs in its
package's folder on purpose (relative paths), and none reads or writes
files today. Window size and display scale per test are split off:
[test-window-size-and-scale](../backlog/test-window-size-and-scale.md).

**D39. "At rest" is a runtime facility, not a test feature**
(2026-09-30). One condition, one loop (`lattice/src/settle.rs`), three
hosts: a test (`app.settle()`; `settle()` from `flux:test` is its flux
half), the control API (`/settle`, the MCP `settle` tool: what an agent
waits on in place of a sleep, on the interactive client) and `srt render`
once playback is on the stepped mode (step 4.6, where the `--settle <ms>`
wall sleep is deleted, not kept beside it). The loop is native and owns no
frame source: a stepped host hands it the step, a client on a display
hands it nothing and the display's frames come by themselves. Rejected:
the loop as JS in core, which render and the control API would each have
had to write again.

**D40. A hold on the engine has a class and a kind, and is released on
drop** (2026-09-30). `PendingOps` was one counter behind hand-paired
`hold()`/`release()` calls. It now hands out `Hold` guards: `in_flight(kind)`
for work that completes by itself, `standing(kind)` for what lasts until
its owner or the outside world ends it. Engine liveness is both, as
before; settling waits for the first only. The class of a binding is
decided by one question: does it end without anyone else acting? A read
on an open stream does not (socket, UDP, p2p, an isolate stream, a
child's output and exit), so it is standing, like the stream; a response
body is finite and in flight. A hold is taken where the work is started,
not inside its future: what a script started is in flight from the call
on, polled or not, and a future dropped half way releases like one that
ran to its end. The kinds are counted, so whoever waits can say what for
("still in flight: 1 fetch, 2 sqlite").

**D41. What settling does with time** (2026-09-30).
- A timer already due is unsettled, and a frame fires it: on the frame
  timeline a timer fires with the next frame, so until that comes the app
  has work waiting that no frame request stands for. Time still passes
  only through frames. A timer due later is standing; `advance` reaches
  it.
- A demanded frame is run, so a running transition is played to its end
  and the test reads the end state. What demands frames forever (an
  `onFrame` loop, a looping animation, a playing video) fails at the cap,
  named.
- Work in flight is waited for with no app time passing; one that never
  lands ends at the test's own wall cap.
- The cap is app time in a test (5000 ms, `app.settle({ maxMs })`), so it
  means the same at 60 and at 1000 fps, and wall time on a client whose
  frames the display paces (`/settle?max=`).

## The test surface

Base layer, on the flux binary:

```ts
import { test, expect } from "flux:test"
import { serve } from "flux:http"

test("the server answers with the stored row", async () => {
  let server = serve({ port: 0, fetch: handler })
  let res = await fetch(`http://127.0.0.1:${server.port}/rows/1`)
  expect(res.status).toBe(200)
  server.close()
})
```

App layer, on the headless runtime:

```tsx
import { test, expect } from "@solidrt/core/test"

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

### Stage 2 - the clock and the rigs (built 2026-09-30; the clock is removed again by stage 2b)

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
  oracle loops need no rewrite of substance. The seven rigs that draw
  random inputs draw them from the seeded `Math.random` (D28).
  `gesture-check` moves to the stepped clock; the parts of it that ride
  `performance.now()` keep their real waits and
  tolerances until [event-timestamp](event-timestamp.md)
  lands. The four GPU rigs stay in `checks/` until stage 5.
- `srt test <file> -- <args>`: the arguments after `--` reach the test as
  `flux:process` argv.
- A first test from the review notes' candidate lists:
  `remapPositions` in the cli. The core candidates all import a runtime
  module and wait for the app layer (see Findings).

Not in this stage, and not this plan's: tests of the flux modules. See
the finding on what is tested where.

### Stage 2b - the clock leaves flux:test (built 2026-09-30)

D27. What goes:

- `test(name, async clock => ...)`, the `Clock` type and its paragraphs
  in `packages/flux-types/modules/test.d.ts`, the clock section of
  `packages/cli/src/test/docs.md` and the line in `packages/cli/AGENTS.md`.
- In `flux/src/test_plugins/`: the stepped clock in `test.js` and the
  natives `installClock`, `uninstallClock`, `clockNow`, `nextDeadline`,
  `advanceTo`, `turn` and `wallTimeout`; the per-test cap goes back onto
  an ordinary `setTimeout`, which is real again in every flux test.
- The four Rust tests of the clock in `flux/src/tests/test_module.rs`.
- What stage 2 added to `flux/src/standards_plugins/time.rs` for it:
  `uninstall_virtual_time`, `next_virtual_deadline`, `virtual_now`,
  `set_wall_timeout`, the `Timers` userdata, and their exports and tests.
  Lattice used none of them, and `time.rs` and its tests are back to what
  they were before the harness. The app layer may want the deadline walk
  back in stage 4, and it is in git (commit "Test harness (1)").

What is parked, beside the four GPU rigs under `checks/`, until
`srt:test` exists (stage 4), because these tests are about time passing
in SolidRT logic (D5):

- all of `gesture.test.ts` (11 tests), now
  `packages/core/checks/gesture.test.ts`. The long-press, the only user
  of the clock, is back on the rig's real waits (nothing at 400 ms, fired
  by 600);
- the test "interactions: hold, tap, doubleTap, chord" of
  `input-map.test.ts`, now
  `packages/core/checks/input-map-interactions.test.ts`; the other twelve
  stay.

Nothing is skipped or retried: they are not flux tests. `srt test` finds
only `tests/` folders, so a parked file runs when it is named
(`srt test packages/core/checks/gesture.test.ts`; all twelve pass), and
the package's typecheck still covers it. After this the flux suite holds
no test that depends on how much time passed, which is what lets CI gate
on it.

Since [event-timestamp](event-timestamp.md) (2026-09-30) the recognizers
take their time from the events, so what was parked here is back in
`tests/` on stated times, except the four tests that wait on a timer:
`packages/core/checks/gesture-timers.test.ts` and
`input-map-hold.test.ts`. The two files named above are gone.

### Stage 3 - CI (written 2026-09-30, first run 2026-10-01)

A job of its own, `test-js`, in `.github/workflows/ci.yml`: the JS tests
are a suite, and a suite is one job with the same command on every
platform. The same four runners as `test-flux`.

- checkout, the Rust toolchain, `rust-cache` with a key of its own
  (`test-js-<platform>`), libclang on the Linux rows,
  `windows-msys2-tools` on Windows (for `make`), `setup-bun`,
  `js-install`;
- `make -C flux flux PROFILE=debug KTX2=0`, which builds and stages the
  binary where `srt` looks for it;
- `bun packages/cli/src/main.ts test`, with `SRT_HOME` set to the
  workspace, on that step only: the flux Makefile derives its own, and a
  Windows path from the environment would not survive its `include`.

Every step runs in bash, as the Windows release build does. Checked
here: the workflow parses, and the test command passes on Linux x64 with
`SRT_HOME` set, against the release binary. The debug build was not
repeated after stage 2b.

The binary is built in the job, in the debug profile: the suite is
functional verification, and it passes there (18 s against 11 s on
release, before the gesture tests were parked). No artifact: nothing
outside the release workflow builds a `flux` binary to take one from.
The release workflow calls `ci.yml` as its gate, so the JS tests gate a
release too. Rejected: a step in `test-flux` (a red job would mean
either suite, and a failing cargo test would hide the JS result; the
argument for it was the shared compile).

This is also the first run of `srt test` on Windows and macOS (the bundle
on stdin, the record prefix). A run on the winbox and the Mac through the
builders before the first push is offered and not decided.

App tests join later as a second job where the dev client builds, Linux
first.

**The first run (2026-10-01, on the commit that landed steps 4.5 and
4.6).** `test-app (linux)` passed: the 31 app tests ran on the client
built in the job, so the runner's Mesa gives the offscreen GLES context.
`srt test` ran on Windows and macOS and the record protocol held. Two
findings, both fixed the same day:

- The host's cap per engine (5000 ms) was sized on this machine's release
  binary; CI runs a debug `flux`, and on it the heaviest test, the
  `eachChunkSlice` property test of `packages/2d/tests/tiles.test.ts`
  (1.9 s on release here), timed out on all four runners, and the two
  `bake` tests of `packages/3d/tests/environment.test.ts` (0.5 s each on
  release) on Windows too. The cap is a hang detector, so it is 30 s now
  (`DEFAULT_TIMEOUT` in `flux/src/test_plugins/host.rs`); nothing was
  added to set it, and no test was shrunk to fit a number. Rejected: a
  release binary in CI (debug assertions are part of what the run
  checks); a cap per test or per run (surface for a safety cap).
- `cited(failed[0])` in `packages/cli/src/test/main.ts` failed the
  `types` job under `noUncheckedIndexedAccess`. `srt check` on the cli
  entry misses the file: it roots the program at the entry, and the test
  command is not statically reachable from `main.ts`; the CI job's `tsc`
  over `src` is the gate for that.

**The second run (2026-10-01, "CI fixes")** turned 14 of 16 jobs green
and showed, through the new durations, that the cap was the wrong lever:
the tiles property test took 10.6 s on the Linux debug build and over
30 s on the MSVC one, where the environment bakes took 26 s. A debug
`flux` with QuickJS compiled optimized, through a per-package override
(`[profile.dev.package.rquickjs-sys] opt-level = 3` in the workspace
Cargo.toml, every other crate at the profile's level), runs the same
test in 1.96 s, release speed: the whole slowdown was the engine's C,
which nothing in CI debugs into. The override is in; the 30 s cap stays
as the hang detector. The other red job was `test-forge (darwin)`, a
one-frame host-lateness drop in the stall phase of the release-policy
test (7 against 6; a misclassified stall would drop twelve), which now
judges its drops against the test's lateness budget as the drop phase
already did.

So that a slow test is seen before CI sees it, the command prints time
now: a file's line carries the time its tests took in their engines, the
closing line the run's wall time, and `srt test --durations` lists every
test with its own (the records carried `durationMs` from the start and
the reporter never printed it).

### Stage 4 - the app layer (redesigned 2026-09-30)

Decisions D30 to D35. Prerequisite done:
[event-timestamp](event-timestamp.md). What the code said against the
first shape of this stage: `srt:test` cannot hold `mount` as a native
module (D31); core has no unmount and an entry's top level runs once per
engine (D30 removes both problems); the playback loop renders
unconditionally and reads back every frame (D33); and the per-test cap
and duration rode `setTimeout` and `performance.now()`, which are stepped
and 0 in an app test (the host measures and caps natively now).

**Step 4.1 - the test host (built 2026-09-30).** D30.

| file | change |
| --- | --- |
| `flux/src/test_plugins/host.rs` | the host, exported as `flux::test`: `Session` (one engine: `install` on a builder, then the listing or the result), `run_file` (a whole file, for a host with one builder function), `Record` and its stdout line |
| `flux/src/test_plugins/test.js`, `mod.rs` | `test`, `expect`; `names` and `runOne` kept for the host in context userdata; no `run`, no timer, no native; the module throws when imported outside a host |
| `flux/src/bin/flux.rs` | `--test [--filter <text>] [--seed <n>] <script>` |
| `flux/src/engine.rs` | `eval_module` reported a failed module twice (the promise `on_fulfilled` derived had no rejection handler); fixed, isolates gain from it too |
| `packages/cli/src/test/main.ts` | no runner lines; spawns `flux --test`, reads `loaded`, `failed`, `result` (with `output`) and `done` |
| `flux/src/tests/test_module.rs` | rewritten onto the host, with tests for fresh state, the cap over a synchronous loop, an uncaught error and a failed load |
| `packages/flux-types/modules/test.d.ts`, `packages/cli/src/test/docs.md`, `packages/cli/AGENTS.md`, `docs/runtime/index.md`, `flux/CLAUDE.md` | the model described |

How an engine reports: the file is evaluated with `eval_module`; once its
top level has finished the host reads the names, or starts the one test
and waits for its promise. The end of a test reaches the host two engine
turns later, so a rejection the test left unhandled is reported at the
checkpoint in between. The cap is a timer of the host's plus a watchdog
thread that sets the engine's interrupt flag, which is what unwinds a
synchronous loop; after the interrupt the engine's log is cut.

A file's top level runs once per test plus once for the listing, which is
documented on the types. A file has to register the same tests on every
evaluation; a test missing on its own run fails with a sentence that says
so.

**Step 4.2 - test mode in the runtime (built 2026-09-30).** D33, D34.

| file | change |
| --- | --- |
| `alloy/src/mode.rs`, `stepped.rs`, `app.rs` | `Mode::Stepped`: headless like playback, a loop that emits no frame signal and only answers `EmitInitEvents` (every engine asks); `is_playback` became `is_headless` |
| `alloy/src/raster/` | `capture_frames: bool` became `FrameSink` (`Window`, `Capture`, `Discard`): both headless sinks draw every submitted frame and swap nothing, only `Capture` reads back |
| `alloy/src/clock.rs` | `set_virtual_ns` is public: the embedder steps the clock in stepped mode |
| `flux/src/standards_plugins/time.rs` | `freeze_wall(ctx, epoch_ms)`: `performance.now()` 0, and `Date.now()`, `new Date()` and `Date()` on the epoch plus the frame timeline (a replaced `Date` global over the engine's own, same prototype) |
| `flux/src/pending.rs` | `hold_engine`: an embedder's hold on the engine while its own work for it is in flight (a stepped frame travels through the runner's loop) |
| `flux/src/test_plugins/host.rs` | `FileRun`: the per-file state machine, for a host that builds its engines in a loop of its own |
| `lattice/src/test_host.rs` | the `Stepper` (the frame signal alloy does not emit, the rate, the reset per engine) and the epoch |
| `lattice/src/plugins/test.rs` | native `srt:test`: `frame`, `frameRate`, `setFrameRate`, `time` |
| `lattice/src/lib.rs`, `runtime.rs`, `main.rs`, `Cargo.toml`, `Makefile` | the `test` feature (on for the dev client, `TEST=0` leaves it out), `start_tests`, the engine loop driving a `FileRun`, the frame verb settling the pending frame, `--test`, `--filter`, `--seed` |
| `packages/core/src/test.ts`, `runtime-modules.d.ts`, `package.json` | `@solidrt/core/test`: `test(name, async app => ..., { fps })`, `app.frame(n)`, `app.advance(ms)`, `app.time` |
| `packages/cli/src/test/main.ts` | the binary by the bundle's imports (any `srt:` module or gui `flux:` module, or a `.tsx` file, is an app test); an app test is bundled with the Solid transform and staged under `dist/test/<name>/` with a data root of its own; `TZ=UTC`; the host's stderr is shown only when the file as a whole failed |

The frame is the time resolution of a test: at 60 fps nothing happens
between 483 and 500 ms, so a test that asserts a threshold to the
millisecond sets 1000.

Found while building:

- The engine loop ended under a test awaiting a frame: a promise holds
  nothing, and the frame's signal travels outside the engine. Hence
  `hold_engine`.
- The virtual timers are seeded from the timeline when the engine's
  builder is put together, so the stepper has to be reset before that: a
  test's timers were otherwise due against the previous test's end time,
  which the double-tap test caught (its single fired 100 ms late).
- A test with no window still gets its timers: they fire in flux's frame
  delivery, not in core's render handler. The reactive flush is core's
  and needs a mounted window, so such a test calls `flush()` itself.

Not done in this step: the sandbox emptied per test (the data root is per
file); `test_module`-style Rust tests of the stepper (it is covered by
the four app tests); isolates of a test file's project (the bundler
searches them under the entry's folder, which is `tests/`).

**Step 4.3 - the verbs (built 2026-09-30).** D31, D32, D36, D37, D38.

| file | change |
| --- | --- |
| `alloy/src/rendertree/mod.rs`, `tree/inspect.rs` | `Element::label`; `find` (exact, by kind, text and label, with `Match::Present` for a pattern the caller checks), `is_visible`, `path_to`, `hit_path`; the label in `NodeSnapshot` and in the loose `/tree` query |
| `flux/src/alloy_plugins/inspect.rs` | the node record (`node_record`), moved here from `lattice/src/go/connection.rs`, with `label` |
| `flux/src/alloy_plugins/properties/mod.rs` | the `label` prop, kind-independent |
| `flux/src/test_plugins/gui.rs` | native `flux:test/gui` (under `test` + `gui`): `find`, `node`, `visible`, `path`, `hit`; records cross as JSON text |
| `flux/src/engine.rs`, `test_plugins/host.rs` | `eval_module_or`: the host hears of a failed evaluation at once, instead of when the cap passes (what the file left pending keeps its engine alive) |
| `lattice/src/input_plan.rs` | the input plan, moved out of `go/connection.rs`: `plan(events, frame_ms)` with a `Wait { ms, frames }` per step; the tap, mouse-move and `drag` rules of D37 |
| `lattice/src/go/connection.rs` | the control API runs the shared plan (a frame is one 60 Hz interval there); `/tree` takes `at` |
| `lattice/src/plugins/test.rs`, `test_host.rs`, `runtime.rs`, `lib.rs` | `srt:test` gains `windowReady`, `inputPlan`, `inputStep`; the stepper injects a plan's steps as the control API does; the sandbox is emptied before every engine |
| `packages/core/src/test.ts`, `types.d.ts`, `runtime-modules.d.ts` | the surface: `mount`, `load`, `find`, `findAll`, `ref`, the locator readers, `tap`, `drag`, `key`, `type`, `input`; `label` on `NodeProps`, which every element's props extend |
| `packages/flux-types/gui/test.d.ts` | the types of `flux:test/gui` |
| `packages/cli/src/server/control.ts`, `mcp/main.ts`, `agents/debugging.md` | `at` on `/tree` and `get_render_tree`, `drag` on `send_input`, the gesture rules and the label documented |
| `packages/core/tests/app.test.tsx`, `sandbox.test.ts`, `alloy/src/tests/inspect.rs`, `lattice/src/tests/input.rs` | 12 app tests of the layer itself, 2 of the sandbox, 9 rendertree tests, the plan's tests |

How the pieces answer the decisions:

- `mount` calls core's `render` (content that is no `<window>` is put in
  one) and then waits on `windowReady()`: the window builds its first
  frame on its size, which reaches an engine through two threads, so
  whether it is there when a test starts is a race. The wait costs no
  frame and no app time. `load` awaits the entry's import instead.
- A locator holds a function that answers the ids it names now; the
  readers resolve it on every read and throw unless it names one node. A
  find that matches nothing lists the texts and labels in its scope.
  A string is matched natively; for a RegExp the native side returns the
  nodes that have the field and the pattern is applied in JS.
- A text that is all of its parent's text is one match, the parent's: a
  `<text><span>x</span></text>` is one visible text.
- `tap` accepts reaching the node, something inside it, or the container
  it sits in (a node that takes no pointer events itself); anything else
  at the point is a cover and fails the tap with its name.
- An input verb asks the native side for the plan's waits, runs frames to
  cover each and sends the step; one frame after the last step, so the
  tree is current when the verb returns (D12's first half).
- A database left open at module level and a file a test wrote are gone
  for the next test (`sandbox.test.ts`), and a server left listening ends
  with its engine.

Not done in this step (the first three built on 2026-10-01, see "Step
4.3, the rest" below):

- `app.link(link)` and `app.debug(name, args)`.
- A synthetic gamepad in a test.
- The readers past the tree.
- Window size and display scale per test:
  [test-window-size-and-scale](../backlog/test-window-size-and-scale.md).
- The MCP bridge's new arguments (`at`, `drag`) were exercised through the
  control API with curl, not through a re-spawned bridge; the website was
  not rebuilt.

**Step 4.3, the rest (built 2026-10-01).** D13, D32, D37.

| file | change |
| --- | --- |
| `alloy/src/gamepad.rs`, `lib.rs` | `Gamepads` is public and its SDL subsystems optional: `Gamepads::synthetic()` is the slot table with no devices, driven by the same commands, snapshot and back edge |
| `flux/src/alloy_plugins/inspect.rs` | the GPU inventory record (`gpu_record`, `insert_label`), moved here from `lattice/src/go/connection.rs` like the node record |
| `flux/src/alloy_plugins/mod.rs` | `request_frame(ctx)` for an embedder |
| `flux/src/test_plugins/gui.rs` | `gpu(label, draw)` |
| `lattice/src/plugins/dev.rs` | `call_debug`, the one call path of the control API and the test |
| `lattice/src/test_host.rs`, `plugins/test.rs` | the stepper's plain `send`; `srt:test` gains `link`, `debug`, `capture` (a paint run now through `render_now`, the capture delivered when it ends: no frame, no app time), and a per-engine synthetic pad table behind `inputStep`'s gamepad steps |
| `lattice/src/go/connection.rs` | `gpu_reply` and `debug_call_reply` over the shared code |
| `packages/core/src/test.ts`, `runtime-modules.d.ts`, `packages/flux-types/gui/test.d.ts` | `app.link`, `app.debug`, `app.gpu`, `locator.tree({ props })`, `locator.pixels()`, `locator.pixel(x, y)`; `InputEvent` takes a gamepad event |
| `packages/core/tests/readers.test.tsx` | 7 app tests: a link with and without a handler, a debug command and an unknown one, a pad seated, held, released and gone, a fresh table per engine, the tree text, a pixel read mid-transition, the inventory |
| `packages/core/AGENTS.md`, `packages/cli/AGENTS.md`, `packages/cli/src/test/docs.md` | the verbs and the readers, with the order to reach for them |

What was decided on the way:

- `app.link` and `app.debug` run the frame their effect lands in and
  return after it, as the input verbs do, so the tree is current; `link`
  throws when the app has no handler (the control API answers
  `delivered: false`), `debug` throws on an unknown name with the
  registered ones listed, both the sentence the control API uses.
- A test's gamepads are a table of its own, in the engine: the command
  applies and the snapshot goes into the UI loop's channel in the same
  call, ahead of the next frame, so a pad state is as deterministic as a
  tap. The interactive loop's table is the same type with its SDL
  subsystems present.
- `outline()` (then `tree()`) folds a span that is all of its parent's text, as `find` does,
  and prints no ids: two runs of the same test print the same text, and
  a test can hold a layout as a string.
- `pixels()` and `pixel()` draw the tree now instead of stepping frames,
  the paused-clock path the control API's snapshot takes: a read mid-
  transition passes no time. The pixels are premultiplied, at the display
  scale; `pixel` maps a point in the node's logical box to them.
- Texture reads need no test verb: `readTexture` from core is the
  standard API and reads synchronously in a test as anywhere.

Verified: the 7 tests, 341 JS tests in 35 files; alloy 638, lattice 66,
flux 99 lib tests; `/gpu`, `/debug` and a gamepad `/input` on the
interactive client after the moves.

**Step 4.4 - `settle()`, a runtime facility (built 2026-09-30).** D11,
D39, D40, D41.

| file | change |
| --- | --- |
| `flux/src/pending.rs` | `PendingOps` rewritten: `Hold` guards of two classes with a kind each, counts per kind, `settled()` (nothing in flight and the job queue dry), `turn()`; exported `hold_engine(ctx, kind)`, `settled`, `in_flight`, `describe_in_flight`, `Hold` |
| `flux/src/engine.rs` | the run loop calls `pending.turn()` every time it comes round |
| `flux/src/plugins/marshal.rs` | `with_pending` became `with_in_flight(ctx, kind, fut)` and `with_standing(ctx, kind, fut)`, the hold taken at the call |
| every plugin that held (`events`, `video`, `camera`, `file`, `fs`, `dir`, `net`, `serve`, `websocket` twice, `subprocess`, `p2p`, `isolate`, `mdns`, `sqlite`, `image`, `clipboard`, `crypto`, `fetch`, `body`, `request`, `response`, `time`) | converted to the guards and classified; no raw hold or release is left |
| `flux/src/standards_plugins/time.rs` | `timer_due(ctx)`: a live virtual timer at or before the timeline's reading |
| `flux/src/alloy_plugins/frame.rs`, `tree.rs`, `gpu.rs` | the frame protocol records why it asked for the next frame; `frame::demand(ctx)` reads it (empty: no frame wanted), a pending `captureSnapshot` included |
| `alloy/src/rendertree/transitions.rs`, `tree/inspect.rs` | `running_node`, `running_transition`, `describe(id)`: the node a transition runs on, named by text, label or id |
| `flux/src/test_plugins/mod.rs` | `settle` exported from `flux:test` (native, beside the JS surface) |
| `lattice/src/settle.rs` | the loop: wait for what is in flight, run a frame while a timer is due or a frame is demanded, until all three hold at once; `Cap::AppTime` or `Cap::Wall`; `Unsettled` says what is left |
| `lattice/src/runtime.rs` | the frame verb tells the waiters a frame ran |
| `lattice/src/plugins/test.rs` | `srt:test` `settle(maxMs)`, stepping through the stepper |
| `lattice/src/go/connection.rs` | the `settle` query of the control API, on the display's frames and a wall cap |
| `packages/core/src/test.ts`, `runtime-modules.d.ts` | `app.settle({ maxMs })` |
| `packages/cli/src/server/control.ts`, `mcp/main.ts` | `/settle?max=`, the `settle` tool |
| `packages/flux-types/modules/test.d.ts`, `packages/cli/src/test/docs.md`, `agents/debugging.md`, `mcp/docs.md`, both `AGENTS.md`, `flux/CLAUDE.md`, the root `CLAUDE.md` | the condition documented where each reader meets it |
| `packages/core/tests/settle.test.tsx`, `flux/src/tests/test_module.rs`, `alloy/src/tests/inspect.rs` | 6 app tests, 2 flux tests, 1 rendertree test |

How "the job queue is dry" is known. A task of the engine (a `Promised`,
a `ctx.spawn`) is polled only in a pass over the engine's tasks, and
rquickjs starts a pass only when no job is pending (`idle()` and
`execute_pending_job` both run jobs first). So a settle that is polled
finds the queue dry, except for what tasks polled ahead of it in the same
pass queued by finishing work. A task that wakes itself is polled again
in the same pass, so yielding is not enough: after the in-flight count
reads zero the settle wakes the engine loop, which comes round and wakes
it back (`turn`), from outside any pass, and then looks again. If no work
started or ended meanwhile (a change counter), the jobs of everything
that finished have run and started nothing. Rejected: draining the queue
from inside the task with `ctx.execute_pending_job()`, which swallows a
job's error and moves the rejection checkpoint.

How the classes came out, against the count the step was prepared with:

- Of the 48 `with_pending` sites, five were not work in flight: the socket
  read and the UDP `recv` in `net`, the stream read in `p2p`, the stream
  step in `isolate`, and a child's `status()`. They are standing now. The
  other 43 are in flight, each with its kind.
- Of the direct sites: in flight are `fetch`, the body reads, the file
  read, `connect`, `listen` and the UDP bind in `net`, the p2p `connect`,
  the initial stdin write of a child, and the video open; standing are
  the event listeners, the timers and intervals, the `serve` loops, both
  websocket kinds with their writers, the accepts, the p2p stream writer,
  a child's supervisor and its output streams.
- Three things were in flight with no hold at all: an isolate call (the
  engine lived on its task alone), a camera open, and a
  `captureSnapshot`. The first two hold now. The capture is frame demand
  instead: it is serviced by a paint and settled by the frame after it,
  so it has to be able to ask for frames while nothing else does.
- Image decode, texture upload and font registration are synchronous or
  ride on `fetch` and the body read; nothing of them was outside.

Verified: 334 JS tests in 34 files; flux 49 lib tests (99 with `gui`),
alloy 638, lattice 66, the 21 `flux/tests/` binaries; `/settle` on the
interactive release client through the control API, where a 600 ms
transition reported `a transition on view labelled "drawer"` and settled
after 553 ms, an 800 ms fetch reported `{ "fetch": 1 }` and settled after
841 ms with its text in the tree, and a standing `onFrame` reported
`onFrame` at a 500 ms cap.

Not done in this step (the first two built since: 4.6 and 4.5):

- `srt render --settle`: step 4.6 (D39). The flag still is the wall
  sleep.
- A test that times out while work is in flight says "Timed out after
  5000 ms" and not what was in flight; the kinds are one call away
  (`flux::in_flight`) and belong in the failure output of step 4.5.
- The MCP `settle` tool was exercised as `/settle` with curl, not through
  a re-spawned bridge.
- `startRecognition` (speech, an optional feature) resolves from a frame
  tick like a camera open and holds nothing; not converted.
- A response body read as a stream is in flight per chunk, which a
  server-sent event stream never finishes: such an app does not settle
  while it listens, and says "1 body read".

**Step 4.5 - failure output, docs, CI (built 2026-10-01).** D14. (The
"four parked files moved to `tests/`" this step once listed was done by
4.2.)

| file | change |
| --- | --- |
| `flux/src/test_plugins/host.rs` | `RunOptions::details`, a `DetailsHook` the embedder installs; `TestResult::details` (`[label, text]` pairs) in the record; the host's own detail, `In flight: 1 fetch`; the engine held from a test's end until the relay turn has run |
| `flux/src/alloy_plugins/inspect.rs`, `test_plugins/gui.rs` | `outline`, the one implementation of the subtree as an outline, exported as `flux:test/gui` `outline(id, props)`; `locator.outline()` calls it |
| `alloy/src/raster/cmd.rs`, `mod.rs`, `gl/readback.rs`, `context/capture.rs` | `Context::read_window`: the window as the last frame left it, a raster RPC |
| `lattice/src/test_host.rs`, `main.rs` | the dev client's hook: `At` (app time, frame), `Frames demanded by`, `Outline` (cut after 200 lines), `Snapshot` (the frame, encoded by `forge::image::encode_png`, written under `--failures <dir>`) |
| `packages/core/src/test.ts`, `packages/flux-types/gui/test.d.ts` | the reader on the native outline, renamed from `tree()` to `outline()` the same day: a method named `tree` promised a structure and handed back a string (`expect(root.tree()).toBe("...")` read wrong); an outline is by definition the indented listing, so the string is what the name says |
| `packages/cli/src/test/main.ts`, `lib/args.ts`, `lib/usage.ts` | the details printed under the error (a snapshot's path relative to the cwd), `--failures` passed as `<stage>/failures`, `--only flux|app` |
| `.github/workflows/ci.yml` | `test-js` runs `--only flux`; `test-app (linux)` builds the client (`make client PROFILE=debug KTX2=0`) and runs `--only app` |
| `packages/cli/agents/testing.md`, `src/test/docs.md`, `AGENTS.md`, `packages/core/AGENTS.md`, the scaffold's `AGENTS.md`, `flux/CLAUDE.md` | the testing guide (what to test at which layer, naming, time, seeding state, reading a failure) and the pointers to it; the failure output described |

How the details are read. The host calls the hook inside the failed
test's engine, where the tree and the stepper are: for a test that ended
by itself, in the relay turn two engine turns after the end, where the
uncaught errors are final too; for a timed-out test, through a closure on
the engine's exec channel plus a bounded poll of the engine (1 s), with
the interrupt flag left set, so a hook that runs JS gets an interruption
and nothing can hang again (the tree read and the paint are native; the
logger is cut by then, so what the paint's JS hooks say is dropped). The
engine had to be held between the end and the relay turn: it would
otherwise end as soon as the test's own work was done, with the queued
turns unrun (the end was already reported through `ended`, so nothing
noticed). The hold is taken only at the end, so a test that waits on
nothing still fails when its engine runs out of work. The snapshot is the
real frame (request a frame, `render_now`, read the window), not a node
capture, so root effects and the overlay are in it.

Verified: 51 flux host tests (two new: the details of a failure and of a
timeout naming what was in flight); a scratch file of failing app tests
read back with every detail (an assertion mid-transition names the
transition's node, a fetch nobody answers times out with `In flight: 1
fetch`, a component that throws shows the error window in the tree and
the snapshot); `--only` on both layers; `srt check`.

**Step 4.6 - render on the stepped mode (built 2026-10-01).** D35, D39.

| file | change |
| --- | --- |
| `alloy/src/playback.rs` (deleted), `mode.rs`, `app.rs`, `raster/mod.rs`, `raster/frame.rs`, `backend.rs`, `rendertree/frame.rs`, `rendertree/platform.rs`, `event.rs`, `clock.rs` | `Mode::Playback`, `PlaybackConfig`, the lockstep loop, `FrameSink::Capture`, `FrameOutput::Captured` and the `always_render` gate bypass are gone; `Mode::Stepped` is the headless mode; `headless_init_events`; `App::run` returns nothing |
| `lattice/src/stepped.rs` | the `Stepper` and `WindowReady` (a native waiter beside the JS promise), shared by test mode and the render host; `DEFAULT_FPS`, `EPOCH_MS`; the seed and the entry module name are flux's (`flux::DEFAULT_SEED`, `flux::ENTRY_MODULE`), shared with its test host |
| `lattice/src/render_host.rs` | `RenderRun`, `RenderHost`, `start_render`: the entry evaluated with `eval_module_or`, the driver a task of the engine: wait for the window's size, fail on no window, `--settle` through `settle::settle` (app-time cap 5000 ms, wall bound 30 s on work in flight), then per frame the scripted input due, `request_frame`, step, wait for the frame, `read_window`, a writer thread encoding the PNGs (`forge::image::encode_png`); the app's `exit()` ends the run in order |
| `lattice/src/lib.rs`, `main.rs` | `Hosts` (test, render, the strict tally) behind `start_with`; the render engine built with the stepper, the frozen wall and `seed_random`; `--render` replaces `--playback`, `--settle` is a flag, `--seed` is shared with `--test`; `start`'s mode and strict parameters are gone |
| `packages/cli/src/render/main.ts`, `lib/args.ts`, `lib/usage.ts`, `src/render/docs.md`, `AGENTS.md` | `--settle`, `--strict`, `--seed <n>` passed through and documented; the "what exit 0 proves" paragraph names the flags |
| `okf/done/render-as-a-verification-gate.md`, `okf/done/seeded-random-headless-render.md` | closed |

Frame k stays the app's state after its (k + 1)th frame callback at
(k + 1) / fps, so an existing render reproduces byte for byte (checked on
three apps, the old loop against the host); with `--settle`, app time
passes while the app comes to rest and the frames start from there. The
render demands every frame itself (`request_frame` before the step, what
the dev clock's step does), so the demand gate stays as it is for every
host and the display-list reuse path is in force: a frame that reuses the
retained list leaves the surface as it was, which the readback returns.
A render that fails (no window, no rest, a frame not read back, the app
ended early, errors under `--strict`) exits 1 with the reason logged.

Verified: byte-identical frames on `examples/hello-world`, `examples/spin`
and an animating probe; the probe's loading state without `--settle` and
its loaded state with it; `--strict` turning a contained error into exit
1; two default-seed renders of a `Math.random` color identical and `--seed
7` another; a key script landing in the frame its time names; a settle on
an `onFrame` app failing, named; alloy 637, lattice 66, flux 101 lib
tests; 341 JS tests.

### Stage 5 - migration on the app layer (built 2026-10-01)

- The four GPU rigs are app tests, their numbered claims one test each,
  `fail()` turned into `expect`, the world a plain function the test
  calls: `packages/core/tests/gpu-lease.test.tsx` (5), `packages/2d/
  tests/collision.test.tsx` (5), `packages/3d/tests/collision.test.tsx`
  (6) and `packages/3d/tests/raycast.test.tsx` (7). The `checks/` folders
  are gone, with core's stale `checks/dist`; the pointers in the 2d and
  3d AGENTS.md and in two core test headers follow.
- The capture test for `@solidrt/3d`, `packages/3d/tests/fog.test.tsx`
  (3): the fog example loaded with `app.load`, the camera parked and the
  mode set through its `pan` and `fog` debug commands, a frame, then one
  pixel on each sun through a new `suns` debug command that projects
  them (the example gained a Scene `ref` for it). The probe is the
  channel order: the sun is red above blue, the sky and a sun fogged
  into it blue above red, so no color space is assumed. Fog shipped
  2026-08-30 verified by eye only; this is its first test.
- The camera components, `packages/3d/tests/camera-components.test.tsx`
  (2): `<OrbitCamera>` orbited by a drag over the scene through the
  pointer feed and the input map, `<FirstPersonCamera>` walked by a held
  `w` through the keyboard device and the window's key handlers; the
  motion itself stays tested on the flux binary. (The plan's note on
  `srt:events` glue was stale: the components take an input map and
  wire nothing themselves.)
- The testing guide and the scaffold pointer landed with step 4.5.

What the migration taught, for the guide's readers: anything that
registers a cleanup (GPU buffers, a scene, a layer, a pointer feed, an
input map) is created inside the `mount` callback, which is the
component body of the test's app; outside it the feed's `onSettled`
cleanup throws and the settle never ends. A query on a scene or a layer
flushes the pending writes, so these tests run no frame after the mount;
`updateVertices` is the exception, which needs the geometry on the GPU
(a frame before) and the box's flush (a frame after). An example with a
standing frame callback is stepped, not settled.

Each new file was broken on purpose once (a number, a pixel channel, a
sun's reading) and failed where it should.

## Later, additive

| feature | the shape it takes |
| --- | --- |
| the same tests on a real device | a `--client` option on `srt test`; stepping maps to the dev clock control |
| queries by role and name | `find({ role, name })`, once a semantics layer exists |
| image goldens | a `toMatchImage` matcher with a tolerance |
| recorded input as a test | a generator over `--capture` output |
| a gui test part in flux | the node verbs (`find`, `findAll`, reading, snapshots) compiled under `test` + `gui` in flux; `srt:test` re-exports it and keeps what is Solid (`mount`, `load`, `link`, `debug`) (D26) |

## Owed after stage 2

Stage 2 moved 19 rigs in one pass. The tests passed and the checks bit
(three were broken on purpose), but the pass left work short of "best
quality" and unverified. What came of it:

Settled 2026-09-30:

- Random inputs come from the seeded `Math.random` (D28, D29); the seven
  files lost their seed constants, their generators and the seed their
  `fail()` appended. Checked by a failure forced on one drawn input: the
  same message in the full run and under a filter, another under
  `--seed`; and by the camera shake, whose random direction is now the
  same on every run. The suite passes under five seeds.
- The 2d and 3d dispatch tests build a fake layer (scene) of their own
  per test, `world()`; none depends on the tests before it, and each of
  the 29 passes run alone under `--filter`. The listeners a test added
  and never removed, which saw the events of the tests after it, went
  with the shared layer.
- `gltf.test.ts` parses in a function the twelve tests that read the
  model call (`parsed()`): a parser throw fails those tests by name.
- The types say what "takes the clock" is (a declared parameter; a
  default value or a rest parameter does not count), and that what a
  timed-out test started keeps running beside the tests after it.
- The residue: the `checked` counters nothing read, the `break`,
  `continue` and `return` after a `fail()` that throws (two of them
  narrowed a type and became a plain `throw`), 71 capitalized test names
  (four start with an identifier or a proper noun and keep their case),
  and the headers that said "Check rig for" or "Checks for".

Left as it is:

- The moved tests still read `if (!cond) fail(msg)`, 991 sites, and none
  uses `expect`, so none gets expected and received printed by a matcher,
  and a sweep stops at its first mismatch where the rig listed them all.
  Half of the messages (491) already print the value. No blanket
  rewrite: it risks weakening checks for little; new tests use `expect`.
- The pan, swipe and double-tap tests wait on real timers and assert
  with tolerances. They are parked since stage 2b and gate nothing;
  event-timestamp removes the waits.

Left open:

- Timers due at the same instant fire with no microtask checkpoint
  between them: a second timer runs before the promise chain the first
  started. Measured 2026-09-30 on the real path: two `setTimeout`s of
  one delay (20 ms, and 0) log "a b a-chain b-chain", since both are
  ready in one poll of the executor. The web runs a checkpoint after
  every task. A question for the runtime, not for the harness; unfiled.
- Closed by D30 (2026-09-30): an uncaught error fails the test it
  happened in, and a test that times out ends with its engine.
- CI compiles the `ktx2` feature nowhere; only the release build does,
  so a break in that code reaches main unnoticed. Found while shaping
  stage 3, which builds with `KTX2=0`. Filed in `okf/tiny.md` (DX).

Verified 2026-09-30, after the timer refactor (`set_wall_timeout` split
out of `setTimeout`, `Timers` kept as userdata):

- All 21 integration test binaries of `flux/tests/`, one at a time at 2
  jobs (8 at once had run the machine out of memory): 170 tests pass.
  The other 2 of the 172 are the `ktx2` tests in `image.rs`, which a
  default-feature build leaves out.
- The dev client, rebuilt (release) and run on an app of timers: a
  0 ms and a 50 ms timeout fire, a cleared timeout does not, an interval
  fires three times and stops at its clear; the same again after a
  reload, and the shutdown is clean.
- The website builds, with both pages in it (`tools/test`,
  `runtime/modules/test`). Not read by eye.
- The JS tests on a debug `flux` binary, which is what a CI job that
  builds in the test profile would run them on: 306 pass, 18.4 s against
  11.5 s on release, three runs of three.

Not verified:

- The `test-flux` CI step now passes `--features test`, and the
  `test-js` job is new; the workflow has not run.
- `srt test` has only run on Linux x64. The bundle on stdin and the
  record prefix (a control character) are untested on Windows and macOS.

Lessons of the scope widening, for the next stage's proposal: count and
read before proposing. Three things were proposed and then found wrong
by looking: tests for the flux modules (`flux/tests/` already has 172),
a `parseColor` test (it imports a gui module), "20 pure rigs" (19). And
the must-throw trap was found halfway through the conversion by luck; an
inventory of how the rigs call `fail()` belonged in the proposal.

## Open

- What a test host does with resources a dropped engine held natively.
  Checked 2026-09-30 for `serve()`: a server left listening ends with its
  test's engine (the next test binds the same fixed port). Unchecked for
  the rest (an open database, a child process); shutdown hooks do not run
  on a drop, as under an isolate's `terminate()`.

## Done looks like

- `srt test` runs every `tests/*.test.ts` and `*.test.tsx` of the
  workspace, each on the binary its imports name, and fails CI on any
  failure.
- No file imports `bun:test`, and no `checks/` folder is left.
- A scaffolded app can carry a test that mounts a component, taps it,
  steps frames and asserts on the tree, with no wall-clock wait anywhere.
- A failing test tells an agent what was on screen and when (4.5: the
  app time and frame, the tree, a snapshot).

## Not this item

Rust test structure (exists, fine), coverage reporting, and a mocking
framework.

## Findings

What would be true had the plan never existed is in
[test-harness-findings](../notes/test-harness-findings.md). What stays
here is the plan's own history:

- The stepped clock resolves to the millisecond: the long-press test
  asserts nothing fired at 499 ms and the press at 500, where the rig
  waited 400 and 600 ms of wall time with nothing in between. The
  double-tap, the pan and the swipe still wait on real timers, because
  they read `performance.now()`: event-timestamp removes that.
- The dispatch tests in 2d and 3d shared one fake layer across their
  tests and depended on running in order, as the rigs did. A listener
  one test added and never removed saw every event of the tests after
  it, which nothing noticed. Each test builds its own since 2026-09-30.
- Bringing `tests/` into the packages' typecheck programs (they were
  excluded while they ran under bun) surfaced one type error, in
  `packages/3d/tests/invert.test.ts`, fixed with the migration.
- `PendingOps` counted what keeps the engine alive, standing holds beside
  work that completes by itself, so `is_idle()` was not the `settle()`
  condition. Step 4.4 split the two (D40).
- `srt render --settle` was a wall-clock sleep after the mount frame
  (`PlaybackConfig::settle`) until step 4.6 made it the settle condition.
