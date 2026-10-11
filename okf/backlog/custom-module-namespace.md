---
title: Give custom runtime modules a namespace of their own
description: A custom runtime can register a module only under a flux:* name, so an app's native module reads as a platform module, would collide with a built-in SolidRT ships later, and shows in Flux.capabilities under a short name a built-in could share; custom runtime modules move to one fixed scheme, custom: (custom:physics, custom:acme/physics), reported by full name, external to every bundle, with flux: and sol: reserved for the platform.
created: 2026-10-11
---

# Give custom runtime modules a namespace of their own

## Symptom

`lattice::Modules::add` (`lattice/src/entry.rs`) accepts only a `flux:*`
name, panics on anything else, and strips `flux:` to get the capability, so
a custom runtime's `flux:physics` is `"physics"` in `Flux.capabilities`. The
toolchain only knows the platform schemes too: a bundle leaves `flux:*` and
`sol:*` external and nothing else (`packages/cli/src/bundle/bundler.ts`, the
app build and `bundleFlux`; `lib/flux-script.ts`; the test bundle in
`test/main.ts`), and the bytecode compiler's `ExternResolver`
(`flux/src/lib.rs`) resolves only those two. An import under any other
scheme fails when the app is bundled.

[runtime-extension-modules](../done/runtime-extension-modules.md) made app
modules `flux:*`, and that was the wrong choice:

- An app's module reads as a platform module. `flux:physics` in app code
  says SolidRT ships it.
- It collides. Once SolidRT ships a module under a name an app already
  took, the two clash on every engine, and the capability means two
  things. Nothing catches it: `Modules::add` checks the name against
  neither the built-ins nor an earlier `add`.
- The short capability name hides which one is there:
  `includes("physics")` cannot tell a built-in from an app's module.

## Shape

One fixed scheme for every module a custom runtime adds: `custom:`, the
docs' own term ("a custom runtime").

- **The author names the module, not the app.** A reusable crate registers
  the same `custom:physics` in every app that links it. A scheme named after
  the app (`myapp:physics`) cannot work: a shared crate does not know which
  app links it.
- **An optional scope, npm style.** `custom:acme/physics`, so two
  third-party crates do not both claim `custom:physics`. A name is lowercase
  letters, digits, `-`, `_` and `.`, with at most one `/` between scope and
  name; `Modules::add` panics on any other shape.
- **`Modules::add` takes `custom:*` only.** `flux:` and `sol:` stay reserved
  for the platform; a name in either, a name in no scheme, and a name
  registered twice all panic, naming the `custom:` rule.
- **Reported by full name.** `Flux.capabilities` lists `custom:physics`,
  never a short form, so a custom module cannot be mistaken for a built-in.
  The dev client reports it as is.
- **External everywhere, by a fixed rule.** `custom:*` joins `flux:*` and
  `sol:*` in every bundle's externals and in the compiler's resolver. No
  per-app list.
- **`sol test` runs a test importing `custom:*` on the app's runtime.** The
  `flux` binary has no custom modules and a custom runtime does not build
  one; today the routing (`APP_MODULE` in `packages/cli/src/test/main.ts`)
  knows only the platform's GUI modules.
- **Types** are `declare module "custom:physics"` in the project's `types/`.
- No alias for the old names: a `flux:*` registration from a custom runtime
  stops being accepted.

## Done looks like

- A custom runtime registers `custom:physics` and `custom:acme/physics`; an
  app imports both, and runs under `sol run`, `sol test` and `sol pack`
  (bytecode included), on desktop and in a derived APK, with both full names
  in `Flux.capabilities`.
- `Modules::add` panics on a `flux:*` name, a bad shape and a duplicate.
- docs/runtime "Native code" (the module example, its capability line and
  `types/physics.d.ts`) and `lattice/examples/custom_runtime.rs`
  (`custom:whoami`) use the scheme, and the custom runtimes outside the repo
  have moved over.

## Order

1. lattice: the name rule, the duplicate check and the full-name capability
   in `Modules::add`, its doc comment; flux's `ExternResolver`.
2. sol: the externals at every bundle site, the test routing.
3. Docs, the example, the runtimes outside the repo.
