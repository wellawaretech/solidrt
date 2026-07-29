---
type: backlog-item
title: Dependency propagation between GPU targets
description: A target sampling another target keeps a stale frame forever unless its own params are written; the API accepts the binding and then quietly stops propagating, an invisible failure mode in any multi-pass chain.
status: open
timestamp: 2026-07-29T00:00:00Z
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
