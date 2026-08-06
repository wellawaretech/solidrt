---
type: backlog-item
title: Shared (target-level) params for draw targets
description: setDrawParams is per-entry, so a value every entry shares - a camera's view-projection above all - must be written once per mesh from JS, turning camera motion into O(scene) FFI crossings. Target-level params generalize what createShaderTarget already has for the single-draw case, and the GL layer's apply-if-declared semantics already do the hard part.
status: done
timestamp: 2026-08-05T00:00:00Z
---

Landed 2026-08-06: `shared_params` on the draw-list target (alloy), applied
per entry before its own params (entry overrides shared - specific beats
general); `setTargetParams(target, params)` plus a positional `params`
argument on `createDrawTarget` (the positional-params convention, matching
`createShaderTarget`); validation is coverage-based as proposed below
(declared by at least one current entry's pipeline, accepted as-is with no
entries, never retroactive). `@solidrt/3d` now writes per-mesh `uModel` +
target-shared `uViewProj`; the `shaderMaterial` vertex contract requires
both. Introspection reports a draw target's shared params in the flat
`params` field.

Shared SAMPLER bindings landed the same day (stage 2, ahead of the lit-tier
consumer): `setTargetTextures(target, textures)` + `createDrawTarget`
`opts.textures` seed, same precedence and coverage rules; shared edges join
the sampler graph (propagation and cycle rules unchanged, recorded under
entry key 0 - draw ids start at 1), and the unit budget checks each entry's
own bindings PLUS the applicable shared names at `setTargetTextures`,
`addDraw`, and `setDrawTextures`, so overflow throws at the call site.
Both halves are pixel-asserted headlessly in `alloy/examples/draw_list.rs`
(apply/override/partial-coverage, seed-then-add persistence, live
dependency through a shared edge, self-bind and coverage guard rails).
Still deferred: program-sorted draw ordering (separate optimization, as
argued above).

# Shared (target-level) params for draw targets

A draw target's uniform values are per entry: `addDraw` seeds them and
`setDrawParams` updates them. Anything every entry shares therefore has to
be written once per entry from JS, and the cost is O(entries) matrix
arithmetic plus O(entries) FFI crossings for a value that is logically one.

The motivating case is the camera. `@solidrt/3d` gives each mesh a
premultiplied `uMVP` (projection * view * world), so a camera change
rewrites every visible entry: orbiting a 500-mesh scene costs 500 matrix
multiplies and 500 param writes per frame. With a per-entry `uModel` and a
target-shared `uViewProj` it costs one write, and the GPU absorbs one extra
mat4 multiply per vertex, which is free. That is the last case where the
retained model's O(delta) cost profile does not apply (see
../research/3d-differentiators.md).

Two second-order wins come with it:

- **Lighting stays off the camera path.** With world-space lighting the
  normal matrix derives from `uModel` alone, so it too becomes camera
  independent - relevant before the lit-material tier arrives.
- **Shared values survive entry rebuilds for free.** They are target state,
  so a geometry or material swap (which re-adds the entry) cannot lose them,
  unlike the per-mesh param dictionary the scene graph re-applies by hand
  today.

## Why this is small

The GL layer already has the semantics. `apply_program` in
`alloy/src/gpu/pass.rs` binds the program, fills `iResolution` when the
program declares it and skips it otherwise, then walks the param list
applying only names present in the reflected uniform table. `iResolution`
*is* a shared, runtime-filled uniform already; this generalizes it to
app-declared names. A shared slice applied before the per-entry slice, in
the same function, is the whole execution-side change.

Worth being precise about what it buys, so the item is not oversold: the
saving is the **JS arithmetic and the FFI crossings**, not the GL-side
uniform calls. Entries bind their program individually today, so a shared
uniform is still applied per entry on the raster thread - cheap, local, and
off the JS path. Dropping that too means sorting entries by program and
skipping redundant binds, which is a separate optimization and should not be
bundled in.

## Shape questions

- **Verb and placement.** A `params` option on `createDrawTarget` plus a
  `setTargetParams(target, params)` update, mirroring
  `setDrawParams`/`setDrawTextures`. Note this is a generalization rather
  than a new concept: `createShaderTarget` already takes target-level params
  because it is the one-draw case, and `setDraw` updates them. Multi-draw
  targets should end up with the same idea spelled the same way.
- **Precedence.** Entry params overriding shared ones is the intuitive
  layering (specific beats general) and lets a single mesh deviate without
  leaving the shared channel. Needs stating either way, since both are
  defensible and silent disagreement would be a nasty bug.
- **Validation.** Call-site validation throws on any name absent from the
  reflected table ([gpu-callsite-validation](gpu-callsite-validation.md)),
  which cannot hold as-is here: a target legitimately mixes material classes
  and a shared name will be declared by some pipelines and not others.
  Proposed rule - throw if the name is declared by *no* pipeline currently
  in the target, tolerate partial coverage, and let the apply-if-declared
  behaviour handle the rest. Two wrinkles to settle: shared params written
  before any entry exists (nothing to validate against yet, so defer or
  accept), and an entry added later whose pipeline does not declare a name
  already set (must not retroactively error). Interacts with
  [gpu-inactive-uniform-two-tier](gpu-inactive-uniform-two-tier.md), which
  is the same "declared but not reflected" question from the other side.
- **Textures.** Whether shared *sampler* bindings come along at the same
  time (an environment map, a shadow map, a shared LUT) or wait for a
  consumer. The lit and environment tiers will want them; nothing does yet,
  so shipping params alone first is reasonable if the naming leaves room.

## Consumer-side change

`@solidrt/3d` swaps its `uMVP` contract for `uModel` plus a shared
`uViewProj`. That is app-facing: `shaderMaterial` currently *requires* a
vertex stage declaring and using `uniform mat4 uMVP`, and every custom
material in the wild would need updating. Doing it before apps depend on
`uMVP` is dramatically cheaper than after, which is the argument for picking
this up early rather than after the lit-material work.

Related: [gpu-draw-list](gpu-draw-list.md) (the retained list this extends),
[gpu-callsite-validation](gpu-callsite-validation.md),
[gpu-inactive-uniform-two-tier](gpu-inactive-uniform-two-tier.md),
../research/3d-differentiators.md, ../research/scene-graph-3d.md.
