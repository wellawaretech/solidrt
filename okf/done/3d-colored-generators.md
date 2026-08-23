---
title: Colored geometry generates twice
description: Building coloured geometry generated twice (generate, then withColors repacked). Fixed in two stages 2026-08-23 - vertex layouts became open attribute lists (withAttribute, one pipeline per layout per material) and every generator takes a layout option to emit the wider stride in one pass. Split from 3d-geometry-ops when that shipped 2026-08-19.
created: 2026-08-19
---

# Colored geometry generates twice

Symptom: every coloured mesh is built as a standard-layout geometry and then
re-packed by `withColors` into a 12-float interleave - two allocations and a
copy per geometry, for the layout the generator could have emitted directly.

Where it lives: the generators in `packages/3d/src/geometry.ts` all emit the
standard layout; `withColors` is the only path to "colored" and it copies.

## Stage 1 done (2026-08-23, uncommitted): layout is open data

The fixed two-member layout enum was the real inflexibility, not the
interleaving. `VertexLayout` is now `"standard" | "colored" |
VertexAttribute[]` (standard prefix first, any named channels after);
`withAttribute(geometry, attr, fill)` is the generic primitive (Three's
`setAttribute` for an interleave) and `withColors` its aColor spelling;
`fillAttribute` under `fillColors`. Materials carry the attributes their
vertex stage reads (derived from its `in` declarations, instance
attributes excluded) and build ONE pipeline per layout met on one program;
add() throws on a missing channel (name + format) and accepts extra
channels. Check rig + live probe (`probes/layout-probe.tsx`) green.

## Stage 2 done (2026-08-23, uncommitted): generators take a layout

Every generator's trailing `label?: string` became `options?: string |
GeometryOptions` (`{ label?, layout? }`; a string is still the label, no
call site changed). The generators build a `number[]` and copied it into a
Float32Array anyway, so `packGeometry(verts, indices, options)` is that
copy made stride-aware; torus/torusKnot write their Float32Array at the
requested stride directly (`generatorStride` + `finishGeometry`). The
profile kit (extrude/lathe/sweep/tube) takes the same tail. Rig asserts
`box(.., { layout: "colored" })` is byte-identical to `withColors(box())`
for box/sphere/cylinder/torus/torusKnot, plus a custom f32 layout, bad
layouts throwing, and the sweep generators headless. `withColors` stays
for hand-built / already-generated geometry.

Rejected on the way: a `colors` fill option on the generators - sugar at
the wrong level once the layout is open data; fillColors/fillAttribute on
the emitted buffer is the one-pass fill.

## Follow-up pass (2026-08-23, same day, uncommitted)

Five improvements surfaced by the work, all shipped:

1. Generators take ONE options object (every field optional with a
   default, Three's names): `box({ width, height, depth })`,
   `torusKnot({ radius, tube, tubularSegments, radialSegments, p, q })`,
   `extrude(profile, { depth, bevel, bevelSegments })`, `tube(path, {
   radius, radialSegments })`, `shape(profile, options?)`, etc. No
   positional form remains (breaking; in-repo callers updated, demos
   outside the repo not touched). `label`/`layout` ride in the same object.
2. Material attributes are the ENGINE's word: alloy reflects active vertex
   attributes at link (`get_active_attrib`, stored on ShaderProgram,
   mirrored UI-side like uniforms), `RenderPipeline::new` rejects a
   program attribute that attributes + instanceAttributes leave uncovered
   or mis-formatted, and `flux:gpu` exports `programAttributes(program)`.
   `Material.attributes()` derives from it; the GLSL `in` regex is gone.
   Live-verified: a declared-but-unread `in vec4 aColor` no longer demands
   colored geometry.
3. `validateGeometry(geometry)` at add(): layout prefix, stride modulo,
   indices present. No max-index scan (O(indices) per add, deliberate).
4. `FLOATS_PER_VERTEX` renamed `STANDARD_FLOATS` (breaking).
5. `checks/sweep-check.ts` rig for the profile kit; rigs listed in
   AGENTS.md.
