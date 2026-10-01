---
title: What building the test harness found about the runtime
description: Facts about flux, QuickJS, the engine loop and the packages that the test harness work surfaced and that hold without it - where tests run, what a never-settling promise does, Math.random, the fail() trap, the select! bias, a fake backend's deadlock under settle.
created: 2026-10-01
---

# What building the test harness found about the runtime

Cut from [test-harness](../done/test-harness.md) when it closed, as
`okf/README.md` asks: each of these is true whether or not that plan
existed.

- QuickJS's `Math.random` is xorshift64* with 52 random bits a value, its
  state private to the context and seeded from the clock in microseconds
  when the context is created; nothing in its API seeds it. A
  reproducible `Math.random` therefore has to be a function of our own
  (D29).
- Code under test draws random numbers too: `shake()` in
  `packages/core/src/camera-control.ts` picks its direction and phase
  with `Math.random()` when none is given, so the shake tests of core,
  2d and 3d ran on other values every run until `Math.random` was seeded.
  A test-only random source would not have reached it.
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
- None of the core candidates in
  [core-package-review](../notes/core-package-review.md) runs on the
  bare flux binary: `parseColor` imports `flux:rendertree`,
  `createTextBuffer` imports the window and layout bindings. They need
  the app layer, or a split of the pure part from the binding.
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
- A fake backend with a delay in its handler deadlocks a settle: the
  handler's `setTimeout` is an app timer, which fires only with a frame,
  and the settle waits for the fetch (work in flight) with no frame
  stepped. Found with the render probe (2026-10-01); the testing guide
  says a fake backend answers at once and a delay is the test's
  `advance`.
- The engine loop's `select!` is unbiased between the exec channel and
  `idle()`: an engine with no holds and a dry job queue can end with
  closures still queued on its exec channel. The test host's relay turns
  rode on that until 4.5 held the engine for them (2026-10-01).
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
