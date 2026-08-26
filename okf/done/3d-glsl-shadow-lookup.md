---
title: Export the shadow lookup from @solidrt/3d/glsl
description: SHADOW_LOOKUP (shadowAt + lightShadow) joined SHADOW_SLOTS and SHADOW in @solidrt/3d/glsl on 2026-08-27; lit and the one custom receiver compose it, so the sampler if-chain has one generator.
created: 2026-08-27
completed: 2026-08-27
---

# Export the shadow lookup from @solidrt/3d/glsl

`@solidrt/3d/glsl` exports the two ends of receiving a shadow and neither
of the middle steps. `SHADOW_SLOTS` declares the set the scene writes
(`uShadowMap0..N-1`, `uShadowMatrix[N]`, `uShadowCast[N]`,
`uShadowBias[N]`, `uShadowNormalBias[N]`), and `SHADOW` samples ONE named
map (`shadow(sampler2D map, vec4 coord, float bias)`). Getting from a
light index to a shadow factor is left to the caller, and it is not one
line: GLSL ES 3.00 will not index a sampler array with a non-constant, so
selecting map `i` has to be an unrolled if-chain over `MAX_LIGHTS`, and
the per-light factor on top of it has to test `uShadowCast[i]`, offset
the receiving point along its normal by `uShadowNormalBias[i]` and carry
it through `uShadowMatrix[i]`.

Both existing consumers write exactly that, independently:

- `litFragment` in `packages/3d/src/material.ts` generates a `shadowAt`
  if-chain with `Array.from({ length: MAX_LIGHTS })` and inlines the
  cast test and normal offset in its light loop.
- `packages/3d/demos/src/the-third-dimension.tsx`, the only custom
  `shaderMaterial` in the repo that receives shadows, generates the same
  `shadowAt` from the same `Array.from` and wraps it in the same
  `lightShadow(i, worldPos, n)`.

So the package's answer to "how does a custom material receive shadows"
is currently "copy fifteen lines of generated GLSL out of `lit`", which
is the thing the exported-constants design exists to avoid: the
`AGENTS.md` line for every other piece is compose `LAMBERT`, compose
`HEMISPHERE`, and customizing never means leaving the system. It is also
`MAX_LIGHTS` baked into two generators, so raising the light count means
finding both.

## Landed 2026-08-27

`SHADOW_LOOKUP` exports both functions, the second built on the first:
`shadowAt(int i, vec4 coord, float bias)` and `lightShadow(int i, vec3
worldPos, vec3 n)`. The order (`SHADOW_SLOTS`, `SHADOW`, `SHADOW_LOOKUP`)
is stated in the doc comments rather than bundled, so each name stays a
function-level constant like the rest of the file. `litFragment` composes
the three when the material receives and none of them otherwise (the
opt-out still declares no samplers); `the-third-dimension.tsx` composes
the same three and calls `lightShadow` per light, with no `Array.from` in
app code. `AGENTS.md`'s GLSL paragraph names the trio and the order.

## Done looks like

A custom fragment declares the set, composes one more constant, and calls
one function per light - no `Array.from` in app code and no knowledge
that a sampler array cannot be indexed. `lit` composes the same export
instead of generating its own copy, so there is one chain in the package
and raising `MAX_LIGHTS` touches one generator.

## Shape questions

- **Which function is the export.** `shadowAt(int i, vec4 coord, float
  bias)` (just the map selection) leaves every caller repeating the cast
  test, the normal offset and the matrix multiply - which is the half
  that actually differs between a correct and a subtly wrong receiver.
  `lightShadow(int i, vec3 worldPos, vec3 n)` is what both call sites
  really wanted; it takes the world position and normal as arguments, so
  it pins no varying names and stays composable with a custom vertex
  stage. Exporting both, the second built on the first, costs nothing.
- **Ordering trap.** The lookup only compiles after `SHADOW_SLOTS` and
  `SHADOW` are in the source, and getting that wrong is a link error
  naming an undeclared identifier rather than anything about ordering.
  Either the doc comment states the required order the way the rest of
  the constants do, or a single bundled constant (slots + `SHADOW` +
  lookup) removes the question - at the cost of a name that is a bundle
  rather than a function, which nothing else in the file is.
- **Opt-in must survive.** `lit` only pulls the shadow set into its
  program when the material receives, so an opted-out material declares
  no samplers at all. Whatever ships has to keep that property: a
  non-receiving custom material must not end up declaring `MAX_LIGHTS`
  samplers for nothing.

## Not in this

No behaviour change and no new uniforms - the scene already writes every
name involved. Comparison sampling (`sampler2DShadow`, hardware PCF)
would change what the lookup's body does but not its signature, and stays
in [3d-shadow-maps](../plans/3d-shadow-maps.md) stage 4.
