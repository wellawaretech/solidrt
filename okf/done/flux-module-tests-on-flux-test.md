---
title: The flux module tests are JavaScript inside Rust strings
description: 131 of the 173 cargo integration tests in flux/tests/ only run a JS program and compare its console output, untyped and run on Linux only in CI; move them to flux:test as flux/tests/*.test.ts, run by sol test on all four platforms, and keep in cargo the 42 that observe what a test inside the engine cannot (the logger, uncaught reporting, liveness, exit, the embedding API, isolate spawning, the websocket wire).
created: 2026-10-01
completed: 2026-10-02
---

# The flux module tests are JavaScript inside Rust strings

Symptom: `flux/tests/*.rs` holds 173 cargo integration tests in 21 files.
Most are a JS program in a raw string, run through `run_source`, with the
joined `console.log` output compared in Rust:

- the JS is not typechecked against `@solidrt/flux-types`, so a
  declaration and the runtime can disagree unnoticed;
- a mismatch prints two log strings, not an expected and a received value;
- paths and ports are spliced into the source (`.replace("__DIR__", ...)`,
  `format!` with doubled braces);
- each file is a test binary of its own that links flux: 21 link steps,
  and building several at once has run this machine out of memory;
- CI runs them once, on Linux, in the `check` job (`cargo test
  --workspace`). `test-flux`, on the four platforms, runs only `--lib`.

Since the test harness ([test-harness](test-harness.md)) flux has
`flux:test` on the `flux` binary: an engine per test, a cap per engine,
`expect`, and `sol test --only flux` in the `test-js` job on all four
platforms.

This revises a recorded finding. "What is tested where" in
[test-harness-findings](../notes/test-harness-findings.md) keeps all of
`flux/tests/` in cargo for two reasons: the tests cover the runtime the
runner stands on, and some assert what a test inside the engine cannot
see. The second holds for the 42 tests listed below. The first does not
keep a module test in Rust: a runtime bug that breaks the runner fails the
run loudly (the host requires a record per test and a closing line), and
the host itself keeps its Rust tests in `flux/src/tests/test_module.rs`.

This is a move, not new coverage. The tests stay what they are, tests of
the marshalling over forge cores that carry their own tests.

## Inventory (read 2026-10-01)

### Stays in cargo: 42

| what Rust observes | tests | where |
| --- | --- | --- |
| the embedding API: plugins, userdata, shutdown hooks, bytecode, stack size | 5 | `engine.rs` |
| `ExecHandle` emitting into the engine from outside | 3 | `events.rs` |
| what reaches the logger, at which level, in which format | 11 | `console.rs` |
| an uncaught error is reported (under `flux:test` it fails the test instead, so the report itself is not visible) | 5 | `promises.rs` 4, `time.rs` `queue_microtask_throw_is_reported` |
| a resolver injected by the host, the spawn count, a parent seeded with two different seeds | 8 | `isolate.rs` (`sol test` also stages no isolates for a flux test) |
| process exit: three run the binary and read its status, one needs a host without `ProcessExit` | 4 | `process.rs` |
| the env snapshot of a variable the test sets (`std::env::set_var`) | 1 | `process.rs` |
| an unsubscribe lets the engine go idle (the host drops a test's engine at its end, so idleness cannot be seen from inside) | 2 | `process.rs` `signal_unsubscribe_lets_engine_idle`, `dir.rs` `watch_reports_changes_and_unsubscribe_lets_engine_idle` |
| the server's websocket wire against an independent raw client (flux's own client would test the codec against itself) | 3 | `http.rs` `serve_websocket_*` |

That leaves nine Rust files: `console`, `engine`, `events`, `isolate` and
`process` whole or nearly, and the remainders of `promises`, `time`, `dir`
and `http`, which can be regrouped by what they observe.

### Moves to `flux:test`: 131

| file | tests | what changes |
| --- | --- | --- |
| `time.rs` | 21 | order is collected in an array instead of the log |
| `http.rs` | 16 | `serve({ port: 0 })` and `server.port` replace `free_port()`; the host's cap replaces the 10 s watchdog thread |
| `web_api.rs` | 15 | |
| `path.rs` | 14 | |
| `sqlite.rs` | 13 | `:memory:` databases, as today |
| `file.rs` | 8 | a writable folder (decision 1) |
| `image.rs` | 7 | the two `ktx2` tests registered only when `Flux.capabilities` lists `ktx2`; CI builds without it, as today |
| `dir.rs` | 6 | a writable folder; fixtures through `dir().create()` and `file().write()` (the file's comment "the API has no mkdir" is stale, `FluxDir.create` exists) |
| `wasm.rs` | 6 | |
| `promises.rs` | 5 | |
| `write.rs` | 4 | a writable folder; read back through `file().bytes()` instead of `std::fs` |
| `net.rs` | 4 | loopback only, as today |
| `mdns.rs` | 3 | |
| `websocket.rs` | 3 | the client against flux's own `serve`, as today |
| `crypto.rs` | 2 | |
| `tty.rs` | 2 | surface and validation only, as today |
| `process.rs` | 2 | `alive`; the resubscribe test is Unix-only, registered when `platform` is not Windows |

Twelve files go entirely, so 21 test binaries become 9.

## Decide first

1. **A writable folder for a flux test.** 18 of the moved tests write
   files or lay fixtures (`file.rs` 8, `dir.rs` 6, `write.rs` 4). A flux
   test gets no sandbox today and runs in its package's folder (D38 of the
   harness). Two shapes:
   - D38 for flux tests too: the host empties a folder before every
     engine and runs the engine in it, as the app layer does with its data
     folder, and a test writes relative paths. No flux test reads a path
     relative to its package today (checked: none imports `flux:fs`), so
     nothing in use loses the package folder as cwd. A fixture a later test
     needs comes in through the bundle or is written by the test.
   - The cwd stays the package folder, and the CLI empties a scratch
     folder under the package's build output before each file; each test
     writes under a name of its own. No runtime change, but isolation
     between tests becomes a convention rather than a guarantee.

   Leaning to the first: one rule in both layers, isolation by
   construction. A `tmpdir()` exported from `flux:test` is out (a
   test-only way to name what the platform already names). A standard
   `mkdtemp` in `flux:fs` would need a recursive remove beside it, which is
   more surface than this needs.

2. **Rejections in `expect`.** About 40 try/catch sites remain in the
   moved tests once the http `.catch(e => console.error(...))` wrappers
   go. The synchronous ones become `toThrow`. For a promise, `expect` has
   nothing, so the choice is either Jest's `expect(promise).rejects`,
   additive to D17, or a flag set in the `catch` and checked after the
   `try`. The second is the must-throw trap of the rig migration (see
   test-harness-findings), so the lean is to `rejects`.

3. **Placement and typecheck.** `flux/tests/*.test.ts` beside the `.rs`
   files: cargo compiles only `tests/*.rs`, and `sol test` finds
   `*.test.ts` in any `tests/` folder and runs it in `flux/`. `sol check`
   does not reach them. Its globs cover `tests/` and `packages/*/tests/`
   at the root (`packages/cli/src/check/main.ts`), `flux/` has no
   `package.json` or `tsconfig.json`, and the root `tsconfig.json` names no
   flux types. So this placement needs a `tsconfig.json` for the folder
   and a glob. The alternative, `packages/flux-types/tests/`, is covered
   already, but it puts the tests away from the crate whose plugins they
   test.

## Done looks like

- `flux/tests/*.rs` holds the 42 tests above, and no test there only
  compares the console output of a module call.
- The 131 run under `sol test --only flux` in `test-js` on all four
  platforms, and `sol check` covers them. Windows and macOS see these
  tests for the first time; what they find there is part of the work.
- Each moved test asserts what its Rust version asserted, compared file by
  file: no check dropped or loosened in the move.
- The finding "What is tested where" says where module tests live now.

## Found while shaping

- The `flux` binary builds its test engines with `ProcessExit`, because
  `flux/src/bin/flux.rs` uses one builder for plain runs and test runs.
  So a flux test, or a flux program under test, that calls `exit()` ends
  the file's run, and the CLI reports a run without its closing line
  rather than a failed test. The test host should hold `ProcessExit` back,
  as a windowed app's engine and isolates do. This is independent of the
  move.

## Outcome (2026-10-02)

Built as shaped, with the three decisions taken as follows.

1. **Sandbox**: the first shape, one folder per package rather than per
   file. The CLI makes `dist/test/data` under the test file's package and
   passes it as `--data-root`; the `flux` binary enters it for the run and
   empties it before every engine (the listing included). Files run one
   after another and the wipe is per engine, so one folder per package is
   isolated per test; a per-file folder would only have mirrored the app
   layer's stage, which carries a bundle and failure snapshots a flux test
   does not have.
2. **Rejections**: `expect(promise).rejects`, additive to D17. Each
   matcher awaits the promise, fails if it fulfills (with or without
   `not`: the negation is of the matcher, not of the rejecting) and
   applies to the reason; `toThrow` sees the reason as the thrown value.
   No `resolves`: an awaited value goes into a plain `expect`.
3. **Placement**: `flux/tests/*.test.ts` with a `flux/tsconfig.json`
   (plain TypeScript, `@solidrt/flux-types` as the global surface) and
   the explicit glob `flux/tests/*.test.ts` in `sol check`. The crate dir
   has no package.json, so the typecheck's project-root walk stops at that
   tsconfig; without it the walk reached the root tsconfig, which names
   no flux types, and every `flux:*` import failed.

The `flux` binary now builds its test engines without `ProcessExit` (the
finding above), so an `exit()` in a test throws and fails that test.

Three Rust files held tests of both kinds and were trimmed, not regrouped:
`process.rs` keeps six, `promises.rs` four, `time.rs`, `dir.rs` and
`http.rs` one, one and three. The counts came out as inventoried: 131
moved in 17 files, 42 stay in 9.

Translation found two module behaviours the Rust tests had tolerated
through `try { await ... } catch`: a second body read (`Response.text()`
on a consumed body) throws on the call, not as a rejection, with the
message as a bare string rather than an Error; and `file().write()`
with data of the wrong type throws on the call rather than rejecting.
The moved tests assert what the module does; whether either should
change is not part of this move.
