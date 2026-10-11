---
title: Give runtime modules a namespace of their own
description: A custom runtime could register a module only under a flux:* name, so an app's own module read as a platform module, would collide with a built-in SolidRT ships later, and showed in Flux.capabilities under a short name a built-in could share; the modules a runtime adds now register under one fixed scheme, runtime: (runtime:physics, runtime:acme/physics), reported by full name, external to every app bundle, with flux: and sol: reserved for the platform.
created: 2026-10-11
---

# Give runtime modules a namespace of their own

## Symptom

`lattice::Modules::add` (`lattice/src/entry.rs`) accepted only a `flux:*`
name, panicked on anything else, and stripped `flux:` to get the
capability, so a custom runtime's `flux:physics` was `"physics"` in
`Flux.capabilities`. The toolchain only knew the platform schemes too: a
bundle left `flux:*` and `sol:*` external and nothing else
(`packages/cli/src/bundle/bundler.ts`, the test bundle in `test/main.ts`),
and the bytecode compiler's `ExternResolver` (`flux/src/lib.rs`) resolved
only those two. An import under any other scheme failed when the app was
bundled.

[runtime-extension-modules](runtime-extension-modules.md) made app modules
`flux:*`, and that was the wrong choice:

- An app's module read as a platform module. `flux:physics` in app code
  says SolidRT ships it.
- It collided. Once SolidRT ships a module under a name an app already
  took, the two clash on every engine, and the capability means two
  things. Nothing caught it: `Modules::add` checked the name against
  neither the built-ins nor an earlier `add`.
- The short capability name hid which one is there: `includes("physics")`
  could not tell a built-in from an app's module.

## The name

The scheme has one job: say who ships the module. The docs state the split
in one sentence ("the app stays a plain portable .solapp; what differs is
the runtime it runs on"), which gives the partition: `flux:*` the engine's
modules, `sol:*` the toolchain's, `runtime:*` whatever this runtime build
adds on top. On the stock runtime the set is empty.

Rejected:

- `custom:` says the runtime is custom, which is true, but the module is
  not "custom" to the crate that publishes it; `custom:acme/physics` reads
  wrong from acme's side.
- `native:` names the mechanism, which every flux module shares:
  `flux:sqlite` is as native as a Rapier module, and the docs' "Native
  code" section covers `flux:wasm` too. It would not say who ships the
  module.
- `ext:` clashes with docs/extensions, which already means the JS-level
  component sets (Components, 2D, 3D).
- `app:` reads, in a bundled app, like an alias for the app's own source
  tree, the one thing the module is not.
- An author-named scheme (`rapier:physics3d`) kills the fixed externals
  rule, and every new scheme is a potential clash with a platform one.

## Shape

One fixed scheme for every module a runtime adds: `runtime:`.

- **The author names the module, not the app.** A reusable crate registers
  the same `runtime:physics` in every app that links it. A scheme named
  after the app cannot work: a shared crate does not know which app links
  it.
- **An optional scope, npm style.** `runtime:acme/physics`, so two
  third-party crates do not both claim `runtime:physics`. A name is
  lowercase letters, digits, `-`, `_` and `.`, with at most one `/` between
  scope and name; `Modules::add` panics on any other shape.
- **`Modules::add` takes `runtime:*` only.** `flux:` and `sol:` stay
  reserved for the platform; a name in either, a name in no scheme, and a
  name registered twice all panic, naming the `runtime:` rule.
- **Reported by full name.** `Flux.capabilities` lists `runtime:physics`,
  never a short form, so a runtime module cannot be mistaken for a
  built-in. The dev client reports it as is.
- **External everywhere, by a fixed rule.** `runtime:*` joins `flux:*` and
  `sol:*` in the app bundle and the test bundle (`RUNTIME_MODULES` in
  `bundler.ts`, one list) and in the compiler's resolver
  (`RUNTIME_MODULE_SCHEMES` in flux). No per-app list. Flux scripts and
  `sol bundle --flux` keep `flux:*` only: the `flux` binary and `fluxrt`
  carry no runtime modules, so an import there fails at bundle time, where
  it should.
- **`sol test` runs a test importing `runtime:*` on the app's runtime.**
  `APP_MODULE` in `packages/cli/src/test/main.ts` routes it to the dev
  client, which `lib/runtime.ts` takes from the project's own runtime.
- **Types** are `declare module "runtime:physics"` in the project's
  `types/`.
- No alias for the old names: a `flux:*` registration from a custom runtime
  stops being accepted.

## Done

- `Modules::add` panics on a `flux:*` name, a name without a scheme, a bad
  shape and a duplicate, and lists the full name; tested in
  `lattice/src/tests/modules.rs`.
- The compiler accepts a `runtime:*` import and still refuses any other
  unknown scheme (checked with `fluxc` on a hand-written bundle).
- A scratch project naming the `custom_runtime` example as its runtime
  (`"solidrt": { "runtime": "dist" }`) ran a `.ts` test importing
  `runtime:whoami` under `sol test`: routed to the app layer, run on the
  project's dev client, `whoami()` answered and `Flux.capabilities` held
  `runtime:whoami` and not `whoami`.
- docs/runtime "Native code" (the module example, the scheme and its
  shape, the capability line, `types/physics.d.ts`) and
  `lattice/examples/custom_runtime.rs` (`runtime:whoami`) use the scheme.
- Outside the repo: the tower-toppling demo moved to `runtime:physics3d`
  and `runtime:physics2d` (runtime, app, types, Makefile, AGENTS.md); its
  `Program` migration is a separate owed item, so it was not built. The
  vision demo vendors lattice at tag 0.0.72, which has the old rule, so it
  stays on `flux:yolo` with its TODO naming `runtime:yolo` until its tag
  moves.
- Not run: `sol pack` and a derived APK with a `runtime:*` module. Both go
  through the same bundle externals and compiler resolver as above.
