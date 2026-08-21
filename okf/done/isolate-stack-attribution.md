---
title: Isolate stack traces are attributed to main
description: A throw inside a "use isolate" module reports as `at boom (main:65:13)`; the module is named main like the app bundle and the position is bundle-relative, so the dev server's remap rewrites it against the app's sourcemap and yields a confidently wrong app file and line. Fix is two halves; the runtime half (declare the module under its isolate id) is a few lines and stops the mis-remap on its own.
created: 2026-08-17
---

# Isolate stack traces are attributed to main

## Symptom

A throw at `worker.ts:67` inside an isolate surfaces on the caller (and in
the forwarded log) as `at boom (main:65:13)`. Two things are wrong:

- The module is called `main`, the same name as the app bundle. With
  several isolates in one app every trace claims to be `main`.
- The position is bundle-relative. In dev the server's log remap
  (`packages/cli/server/remap.ts`) rewrites every `main:LINE:COL` against the
  latched app sourcemap, so an isolate frame is remapped to whatever the app
  bundle has at that line: a wrong file and line stated with full confidence,
  which is worse than an unmapped one.

Reported against an app with one isolate, where it is only cosmetic.

## Cause

- Runtime: `FluxEngine::eval_module` (`flux/src/engine.rs`) declares an
  isolate's source module as `"main"`, the same literal `eval_source` uses for
  the entry. On the bytecode path the name is baked at compile time, and
  `fluxc` compiles everything as `"stdin"` (`flux/src/bin/fluxc.rs`), so a
  packed app's frames say `stdin:` for the entry and every isolate alike.
- Toolchain: `bundler.ts` builds each isolate module as its own `Bun.build`
  without a composed sourcemap ("Only the app's build gets a composed
  sourcemap for now"), and `remap.ts` knows one map for one module name.

## Done looks like

- A frame from an isolate names the isolate: `worker:65:13` (id = the
  `"use isolate"` module's id, as `isolate("worker")` sees it), in dev and
  packed; the entry stays `main`.
- In dev the frame remaps to `worker.ts:67` like app frames do; an unknown
  module name is left as it is, never remapped against another module's map.

## Involves

1. Runtime (small, do first; it alone stops the mis-remap because `worker:`
   no longer matches `main:`): `eval_module` takes the module name and
   declares under it; `flux:isolate` passes the isolate id. `fluxc` takes the
   module name (an argument, default `main` rather than `stdin`), and the
   packer / pack-folder compile the entry as `main` and each isolate bundle
   as its id.
2. Toolchain: give each isolate build a composed sourcemap the way the app
   build has one (`bundle()` in `bundler.ts`, the server-side rebuild in
   `packages/cli/server/rebuild.ts`), ship or latch them per module id, and
   make `remap.ts` key maps by module name (`main`, `worker`, ...) and only
   rewrite positions whose module has a map.

## Done

Both halves (2026-08-20): `eval_module` declares under the isolate id,
`fluxc` takes the module name as its one argument (default `main`; `stdin`
is gone), pack compiles each isolate bundle as its id, every dev build
composes a sourcemap, and the server latches maps keyed by module name
(`state.currentMaps`, wire field `maps`) with `remap.ts` rewriting only
positions whose module has a map. Verified end to end with
sandbox/isolate-probe: the raw client frame says `worker:33:35`, the
server's /logs shows `src/worker.ts:32:35` - the actual throw line.

Related: okf/done/isolate-follow-ups.md (errors as data is orthogonal:
that is about `name`/`stack` surviving as fields on the parent's Error, this
is about what the stack text says).