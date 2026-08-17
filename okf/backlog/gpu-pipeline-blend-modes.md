---
title: More pipeline blend modes
description: The blend vocabulary on createPipeline is "none", "add", "multiply" and "alpha"; the rest of GL's fixed-function space (screen, subtract, min/max) is unexposed, demand-driven.
created: 2026-07-29
---

# More pipeline blend modes

The `blend` option on createPipeline/createShaderTarget
([gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md)) shipped 2026-07-29
with exactly two values: `"none"` and `"add"` (`glBlendFunc(ONE, ONE)`),
because additive was the mode with three independent field requesters.
GL's fixed-function blend stage (glBlendEquation x glBlendFunc factors)
reaches considerably more; each named mode below is one `BlendMode` enum arm
in alloy/src/shader.rs plus one func/equation call in `run_pass`'s mesh arm,
so any of them lands as a two-line change when a project asks.

Order-independent (commutative like "add": no sorting, no depth question
beyond the existing explicit `depthWrite`):

- `"multiply"` - `(DST_COLOR, ZERO)`: darkening accumulation (shadow or dust
  passes). LANDED 2026-08-17, see below.
- `"screen"` - `(ONE, ONE_MINUS_SRC_COLOR)`: inverse of multiply; a softer
  glow that saturates toward white instead of clipping the way add does.
- `"subtract"` - reverse-subtract equation with `(ONE, ONE)`: additive
  darkening; dst - src accumulates commutatively.
- `"min"` / `"max"` - blend equation only; niche (data/heightfield tricks).

Order-dependent, the big one:

- alpha-over - `(ONE, ONE_MINUS_SRC_ALPHA)` premultiplied, or
  `(SRC_ALPHA, ONE_MINUS_SRC_ALPHA)` straight. The GL call is equally
  trivial; what defers it is correctness: it needs sorted geometry (which the
  pipeline API has no story for) and an answer to straight-vs-premultiplied
  against how Impeller composites the target. Tracked as the remaining
  blending piece in [gpu-alpha-translucency](../done/gpu-alpha-translucency.md);
  do not add the mode without deciding those two. The premultiplied half now
  has a first step that costs nothing: document the target pixel contract
  ([gpu-review](../notes/gpu-review.md) lesson 12 - premultiplied,
  non-linear RGBA8), which decides the factor pair by declaration.

Out of scope on ES 3.0: the fancy end of the tree-level `<texture blendMode>`
set (overlay, hue, color-dodge, ...). Those are Skia shader-level composites;
fixed-function GL cannot express them within a draw short of
KHR_blend_equation_advanced, which is not baseline. The two layers stay
honestly different: full Skia set between stacked targets, a small
factor/equation vocabulary within one draw.

Demand-driven: leave the vocabulary as-is until a field report names a mode.

## Demand recorded 2026-08-17: multiply, then alpha

The gate above is met, and both named modes are on this list.

`"add"` covers glows and does it well, but the two-value vocabulary cannot
**darken**, so the single most common depth cue in 3D content - a shadow
under a moving object - has no path, and neither does fading anything out
(a dissolving surface, a distance fade on a prop). The workaround was a 4x4
Bayer screen-door: compute coverage, compare against an ordered dither
threshold from `gl_FragCoord`, `discard` below it. No sorting, no blend
state, works today, and it reads as a deliberate retro effect rather than as
breakage - but it is not a soft shadow and it does not fade.

`"multiply"` is the one to take first. It is what a projector shadow wants,
it is order-independent for a single layer, and per the classification above
it needs none of the sorting or premultiplied answers that defer alpha-over.
It can land on its own.

Alpha-over stays behind [gpu-alpha-translucency](../done/gpu-alpha-translucency.md)
and its two prerequisites, which now have a named owner for the sorting half
(the scene graph) - see that item.

## Landed 2026-08-17: multiply

`"multiply"` = `glBlendFunc(DST_COLOR, ZERO)` on all four channels (Skia's
modulate; no separate alpha factor). One `BlendMode::Multiply` arm in
alloy/src/gpu/vocab.rs, one func call in `run_pass`'s mesh arm, the type
union in flux-types, docs in core.md / gpu.ts. Verified on Linux via the
control API: white-cleared target, two overlapping 50% gray triangles read
back 255 / 128 / 64 with alpha 255, and `/gpu` reports `"blend":"multiply"`.

Deliberate: alpha multiplies too. On the premultiplied target that is the
useful reading - `vec4(k, k, k, 1)` darkens color only (the shadow), a
uniform factor across rgb and alpha fades what is already there (the
dissolve), and neither breaks the rgb <= alpha invariant. It is documented
as "scales, all four channels", not as an alpha-over substitute.

Screen, subtract, min/max stay demand-driven.

## Landed 2026-08-17: alpha

`"alpha"` = `glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA)`, premultiplied - the
factor pair was decided by declaration once the pixel contract said targets
are premultiplied ([gpu-pixel-contract-docs](../done/gpu-pixel-contract-docs.md)),
and the sorting question was answered by ownership, not by the engine: at
the GPU layer the app orders the draw list (`before`, `setDrawOrder`), above
it the scene graph sorts. Same shape as multiply: `BlendMode::Alpha` arm,
one func call, type union, docs. Documented as the one order-DEPENDENT mode
with premultiplied output (`vec4(color * a, a)`), normally after the opaques
with `depthWrite: false`; nothing sorts for you.

Verified on Linux via the control API: opaque green clear, half-alpha red
and blue quads, two targets in opposite draw order read back exactly the
over-composite values (overlap 64,63,128 vs 128,63,64) and `/gpu` reports
`"blend":"alpha"`.

Naming: `"alpha"` (glTF `alphaMode: BLEND`, common usage) rather than the
tree's Skia `"source-over"`; the pipeline vocabulary already diverged with
`"add"` vs `"plus"`.

The library half - transparent materials, the scene-owned back-to-front
sort and `renderOrder` in `@solidrt/3d` - landed the same day; see
[gpu-alpha-translucency](../done/gpu-alpha-translucency.md).
