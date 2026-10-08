---
title: Build an app's isolates under sol test, and reject a missing one
description: An app test is bundled from the test file, so the bundler looks for "use isolate" modules under tests/ and every isolate() the app makes fails with "no such isolate module"; that failure is also thrown from the call itself instead of rejecting its promise, which sends an app that handles rejections into the error window.
created: 2026-10-08
---

# Isolates under sol test

## 1. sol test does not build the app's isolates

An isolate id is a path relative to the source root, the directory of the
project's entry (`solidrt.entry`, default `src/index.tsx`):
`isolate("mesher")` names `src/mesher.ts`. `bundleWith` scans
`dirname(opts.entry)` for `"use isolate"` modules
([bundler.ts](../../packages/cli/src/bundle/bundler.ts),
`findIsolateModules`). `sol run`, `sol render` and `sol pack` pass the app's
entry, so the scan covers `src/`. An app test passes the test file
(`stageApp` in [test/main.ts](../../packages/cli/src/test/main.ts)), so the
scan covers `tests/` and stages none of the app's isolates. Even if it found
them, their ids would be counted from the wrong root.

So every isolate call fails under `sol test` with
`isolate 'mesher': no such isolate module in this app`, whether the app is
loaded with `app.load(() => import("@/index.tsx"))` or the test calls
`isolate()` itself. An app that follows performance.md and moves heavy
synchronous work into an isolate cannot be tested as it runs.

**Done when:** an app test stages the same isolates `sol run` would, under
the same ids, and `isolate("mesher")` works from the app and from the test.
A test file in a library package with no app entry keeps today's behaviour.
A CLI test with a fixture project that holds an isolate covers it.

**Involves:** let `bundleWith` take the isolate root apart from the entry,
and have `stageApp` pass the project's source root, resolved the way
`resolveMode` resolves the entry ([mode.ts](../../packages/cli/src/lib/mode.ts)).
Isolate modules that live under `tests/` only are out of scope until a test
needs one.

## 2. A missing isolate module throws instead of rejecting

The resolver's contract says its `Err` is "the message the caller's promise
rejects with" (`IsolateResolver` in [engine.rs](../../flux/src/engine.rs)).
But a call spawns the child on first use and turns a spawn failure into a
synchronous throw: `self.instance(&ctx).map_err(|m| Exception::throw_message(..))?`
in [isolate.rs](../../flux/src/forge_plugins/isolate.rs), on the call path
and in `exited`. So `worker.mesh(model)` throws from the call expression when
the module is missing, the id is malformed, or the runtime cannot spawn. An
adapter that maps rejections (`.then(ok, fail)`, `.catch`, a Worker-shaped
wrapper turning failures into "error" events) never sees it, and the throw
escapes to the error window. The already-aborted branch just above it shows
the right shape: settle the rejection from a task, so the caller can attach
its handlers first.

**Done when:** every failure to start an isolate (unknown or malformed id,
undecodable module, a runtime that cannot spawn, a failed spawn) rejects
the call's promise and ends a stream with that error, and `exited` settles
instead of throwing. Covered in `flux/tests/isolate.rs`.

## Built (2026-10-08)

| file | change |
| --- | --- |
| `packages/cli/src/bundle/bundler.ts` | `BundleOptions.isolateRoot`: the directory isolate ids count from, `dirname(entry)` when unset |
| `packages/cli/src/lib/mode.ts` | `projectEntry(projectDir)`: the declared or default entry, resolved, not checked; `resolveMode` uses it |
| `packages/cli/src/test/main.ts` | `stageApp` passes the project's source root when its entry exists; a package with no app entry keeps the test file's folder |
| `packages/cli/tests/fixtures/isolate-app/` | the fixture app: `src/mesher.ts` ("use isolate"), an entry that calls it, `tests/app.test.tsx` calling it from the loaded app and from the test |
| `packages/cli/src/test/main.ts` | discovery walks on below a `tests/` folder (it used to stop there), so a fixture project under `tests/fixtures/` runs as a project of its own |
| `packages/cli/src/check/main.ts` | `sol check` covers the test files of fixture projects under a package's `tests/fixtures/` |
| `flux/src/forge_plugins/isolate.rs` | `failed_call`: a child that cannot start rejects the call from a task, with `next()` rejecting the same way (marking the promise observed) and `return()` done; `exited` resolves with the message |
| `flux/tests/isolate.rs` | unknown id rejects and is asked again, unknown id ends a stream with the error and no uncaught report, `exited` settles with the failure, a runtime without a resolver rejects |
| `packages/flux-types/modules/isolate.d.ts`, `packages/cli/src/test/docs.md` | the contract in words |

Not changed: `sol check <test file>` still scans isolates under the file's
own folder (`check/typecheck.ts`), so an isolate module nothing imports is
not a program root when the entry checked is a test; a tiny.md item if it
bites.
