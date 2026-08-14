---
title: Dev-server typecheck
description: Run the project's tsc once at dev-server startup, fire and forget, to catch the unbound identifiers Bun.build and the QuickJS compile accept silently; srt check stays the gate.
created: 2026-07-25
completed: 2026-07-25
---

# Dev-server typecheck

## Motivation

Surfaced while debugging a deliberately-introduced error in `examples/grid.tsx`
(`brrrreak is not defined`). Verified empirically that nothing in the current
pipeline catches an unbound identifier before runtime:

- `Bun.build` (`packages/cli/src/bundler.ts:119-129`) accepts it silently,
  success `true`, empty `logs`, even with `minify: true` (the minifier leaves
  it unmangled rather than flagging it - a bundler can't know the full host
  global surface, so it must treat any unresolved bare identifier as a
  presumed-external global, not an error).
- `fluxc` (QuickJS `Module::declare` in `flux/src/lib.rs:80`) also compiles it
  clean, exit 0. QuickJS's compiler resolves lexical scoping (local vs.
  closure vs. global slot) but a global slot doesn't need to exist at compile
  time - JS resolves free identifiers dynamically at the moment of execution.

Only a type checker's binding resolution catches this class of typo
statically: `tsc` must resolve every identifier to a declaration to type it,
so an unresolved name is always an error (`TS2304: Cannot find name`), not a
runtime-only concern. This is distinct from `feedback_no_typescript_typecheck`
(memory) - that guidance is about Claude Code not invoking `tsc` for its own
verification loop; this plan is about solidrt's own build integrating `tsc`
for app authors.

## What already exists

Both building blocks are already implemented, not net-new:

- **On-demand check**: `srt check` (`packages/cli/src/commands/check.ts`)
  already bundles in-memory (no side effects) and runs the project's own
  local `tsc --noEmit --pretty false` (`typecheck()`, check.ts:48-60),
  filtering out diagnostics whose file path is under `node_modules` (since
  `@solidrt` packages ship raw `.ts` and a strict consumer `tsconfig.json`
  surfaces their internal errors too - counted as `hidden`, not shown).
- **Root tsconfig**: `tsconfig.json` at repo root already has `strict: true`,
  `noEmit: true`; `typescript` is already a dependency (`node_modules/typescript`
  present).

Unverified: whether `flux-types` (see `project_flux_types_parity` memory) is
actually wired into a consuming app's `tsconfig.json` (`types`/`include`).
If not, the first real run of this will surface it immediately as a wall of
`Cannot find name 'Flux'`-style errors - self-diagnosing, no separate
investigation needed.

## Decided

- No persistent `tsc --watch` / language-server-style process. Rejected as
  more moving parts than this needs.
- No typecheck on every hot reload. `tsc` checks the whole program, not the
  changed file - full-program latency on every save is unacceptable for the
  hot-reload loop. The file-watcher rebuild path
  (`packages/cli/src/watcher.ts:14`, `rebuild()`) stays exactly as-is, no
  typecheck added there.
- Typecheck runs exactly once per dev-server process lifetime, at startup.
  Known consequence: diagnostics go stale as soon as the first hot-reload
  edit lands; `srt check` is the answer for a fresh verdict mid-session.

## Staging (bare minimum first)

1. Export `typecheck()` (and `findProjectRoot()`) from `check.ts` instead of
   keeping them private, so `server.ts` can reuse the same logic `srt check`
   already uses - no duplicated tsc-spawning/diagnostic-parsing code.
2. In `runServerCommand()` (`packages/cli/src/commands/server.ts:14-56`), call
   the typecheck once at startup, scoped to the `source && isSource` branch
   only (server.ts:29): the prebuilt `.srt.js` branch has no checkable
   project - a sourcemap is a position-translation table for error display,
   not a program tsc can check (tsc needs the real source tree + tsconfig +
   dependency types, which for a prebuilt bundle may not exist on this
   machine at all; today the prebuilt branch does not even load a .map).
   The command also runs with no source at all (server-only mode). The
   boot sequence in the file is `startServer()` -> initial bundle ->
   `startRepl()`/`startWatcher()`; where the typecheck slots in depends on
   the decision below (blocking before the REPL, or fired-and-forgotten).
3. Print diagnostics the same way `srt check` does (errors to `console.error`,
   `hidden`-count summary line).

## Decision (resolved 2026-07-25: (a), fire-and-forget)

On a startup type error, does the dev server:

- **(a) print and keep running** - the bundle already succeeded and the app
  already works at runtime; a type error is a heads-up, not a hard gate.
  `srt check` already exists as the separate hard-gate command (CI,
  pre-commit, explicit "am I clean" check). Consistency also points here:
  the dev server already keeps running on a failed *bundle* (server.ts:39-41
  `showBuildFailure()`, watcher.ts:23 "Build failed, waiting for changes...");
  only the one-shot paths (`bundleTo`, `srt check`) hard-exit on build errors.
- **(b) refuse to start** - treat type errors as boot-blocking. Note this
  would be *stricter than the existing bundle-error behavior* (see above),
  not consistent with it - a new gate, and one `srt check` already provides
  on demand.

The choice determines the execution shape, not just the message:

- Under (a) the check should NOT be awaited: kick it off after
  `startRepl()`/`startWatcher()` and let diagnostics print when tsc finishes
  (async output over the REPL is already normal - watcher rebuild failures
  print the same way). Full-program tsc takes seconds; blocking the REPL,
  watcher, and welcome line on it every boot buys nothing if the server
  continues regardless.
- Under (b) it must block the boot, adding that latency to every dev-server
  start - a further argument for (a).

Decided (a), fire-and-forget.

## Implemented + verified 2026-07-25

- `check.ts` exports `findProjectRoot`, `typecheck`, and a new `reportTypes`
  (shared diagnostic printer, takes optional printers; `srt check` uses the
  console defaults, the server passes the repl-aware `print`/`printErr`).
- `server.ts` fires `startupTypecheck(source)` un-awaited after
  `startRepl()`/`startWatcher()`, guarded by `source && isSource`.
- End-to-end run against `examples/grid.tsx` (with the motivating typo in
  place): boot is not delayed (welcome line, `srt>` prompt, watcher all up
  first), then `examples/grid.tsx(14,22): error TS2304: Cannot find name
  'brrrreak'.` prints over the prompt, followed by the `36 type errors in
  app code` summary.
- Finding fixed along the way: the repo-root `tsconfig.json` had no
  `exclude`, so an in-repo run swept `lattice/target/**` where CMake emits
  fake-`.ts` dependency files (`compiler_depend.ts`), drowning output in
  thousands of junk syntax errors. `srt check` from the repo root had the
  same latent problem. Added `"exclude": ["**/node_modules", "**/target"]`
  (setting exclude replaces the node_modules default, so it must be listed).
- The flux-types wiring concern above did not materialize in-repo (no
  `Cannot find name 'Flux'` wall); a scaffolded project's first run remains
  the real test. The 36 in-repo diagnostics are genuine pre-existing type
  errors across examples/ and packages/, out of scope here.
