---
title: Dependency propagation between GPU targets
description: A target sampling another target keeps a stale frame forever unless its own params are written; the API accepts the binding and then quietly stops propagating, an invisible failure mode in any multi-pass chain.
created: 2026-07-29
completed: 2026-07-31
---

# Dependency propagation between GPU targets

Raised by the second-reality field report
(projects/second-reality/SOLIDRT-FEEDBACK.md #3), measured against 0.0.39.

Shader A renders an animated gradient with `uT` written every frame. Pipeline B
samples A via `textures` and has static params of its own. A visibly animates;
B never re-renders - `get_texture` on B returns the same pixels indefinitely.
Writing ANY of B's own params makes B re-render, against A's current pixels.

## Why it matters

Rendering is demand-driven: a shader re-renders when its params change. A
sampler binding is not treated as a dependency, so a multi-pass chain (plasma
target -> cube pipeline) silently freezes unless every consumer happens to
write a uniform per frame. Second-reality's consumers all drive rotation, so it
cost that project nothing - which is precisely the problem. The API accepts the
binding, produces a correct first frame, and then quietly stops propagating.
Nothing fails loudly; the output is just stale.

This is the worst in kind of the 0.0.39 GPU findings. A missing feature is
visible; this looks like it works.

## Proposed shape

The runtime already knows the binding graph - `setShaderTextures` validates
that a sampler cannot source the shader's own target (a GL feedback loop), so
the edges are known and already walked for cycle rejection. Marking consumers
dirty when a bound input re-renders would make chains just work, matching the
demand-driven model rather than fighting it.

Points to settle:

- **Transitive depth.** A -> B -> C must propagate the whole way, and the
  cycle rejection that exists for self-binding needs to hold for longer cycles
  before this becomes a re-render loop.
- **Ordering within a frame.** A consumer must re-render after its input, not
  before, or the chain is merely stale by one frame instead of indefinitely.
- **Cost.** Every producer re-render now schedules its consumers; a wide fan-out
  of expensive passes could be worse than the current behaviour for apps that
  deliberately sample a static target. An opt-out may be wanted.

## Failing that

Document the rule on `createShader`/`createPipeline`'s `textures` option: a
consumer re-renders only when its own params change, so drive one uniform per
frame in every node of a live chain. This is strictly worse than fixing it -
the shape of the bug is that it is invisible - but it is better than the
current silence and is a few lines.

## Resolution (2026-07-29)

Fixed, and further than proposed: target rendering went pull-based instead of
push-propagated. Writes (params, sampler rebinds, draw count, buffer writes,
data-texture uploads, resizes, creates) no longer render eagerly - they mark
the target id dirty in the raster thread's dirty set, and `flush_dirty`
resolves the whole affected subgraph in dependency order (Kahn over the
sampler edges, `propagation_order` in `alloy/src/raster.rs`) at the points
pixels become observable: a drawn frame, an offscreen rasterization
(snapshots/captures), a readback. That answers all three open points at once:

- Transitive depth: the flush walks the sampler graph to a fixpoint, so
  A -> B -> C chains re-render end to end.
- Ordering within a frame: topological order per flush, sources before the
  targets sampling them; a diamond join renders once, after both arms.
- Cost: a target renders at most once per flush no matter how many writes or
  upstream re-renders landed, which also subsumed the old per-batch params
  load-shed machinery (deleted). The deliberate-static-sampling opt-out was
  dropped as unnecessary under this model.

Cycle rejection generalized from self-binding to whole-graph:
`update_shader_textures` walks a UI-side mirror of the sampler edges
(`samples_transitively` in `alloy/src/context.rs`) and throws synchronously on
any binding that would close a cycle. The raster flush keeps a warn-and-
render-once fallback should the mirrors ever diverge (no hang, no silent
skip). Cycles at create are impossible (the new id is unnameable).

Contract documented in `docs/core.md` (sampler bindings are live
dependencies), `packages/flux-types/gui/gpu.d.ts`, and
`packages/core/src/gpu.ts`. Graph logic unit-tested GL-free in
`alloy/src/tests/gpu_graph.rs`.

Runtime-verified 2026-07-31 on five clients (Linux, Windows/ANGLE, three
Android including the 2017 TV). A two-stage chain whose second stage samples
the first and carries no params of its own re-rendered exactly as often as
its source - 2609/2609 on Linux, 3550/3550 then 7194/7194 on the TV,
10177/10177 on Windows - and its pixels tracked the source's animated phase.
In the same app, two targets sampling only static data textures stayed at
**1 pass** for the app's whole life: the pull model neither drops a live edge
nor re-renders a dead one. See the verification section of
[gpu-review](../notes/gpu-review.md).
