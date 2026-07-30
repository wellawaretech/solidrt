---
type: backlog-item
title: GPU target purity and an explicit render verb
description: The retained target model silently relies on every pass being a pure function of its inputs, and every wanted extension in the accumulation/feedback/multi-pass class breaks that invariant; decide whether the model stays pure or gains one imperative escape hatch before building any of them.
status: decided 2026-07-30 - option 2, implemented; see okf/plans/gpu-render-verb.md
timestamp: 2026-07-30T00:00:00Z
---

# GPU target purity and an explicit render verb

**Decided 2026-07-30: option 2.** The invariant is documented, and targets
created `render: "manual"` are stepped by `renderTarget(id)` - the one
imperative verb. Contract, surface, traps, and the stage-3 consumer list
(loadOp, copyTexture, examples) live in [[gpu-render-verb]]
(okf/plans/gpu-render-verb.md). The gates below are lifted for manual
targets only; loadOp on a flush-rendered target still must not ship.

The central finding of [gpu-review](../analysis/gpu-review.md) (structural
divergence section). In WebGL and WebGPU the unit of work is an event (a
draw, a submitted pass); here it is a thing - a target that re-renders
itself whenever the dirty flush decides its inputs moved. That model is the
right one for this runtime, and it rests on an invariant stated nowhere:

> A target's contents are a pure function of its inputs. Rendering it twice
> is indistinguishable from rendering it once, so the runtime is free to
> render it zero, one, or many times per flush.

Everything in the wanted-but-deferred class breaks it:

- **Accumulation / loadOp "load"** (do not clear): output depends on
  previous output, so output depends on how many times the flush ran.
- **Ping-pong feedback** (A reads B, B reads A across frames): a cycle,
  which the graph rejects outright today because the pull model cannot
  order it.
- **Multi-pass into one target** ([[gpu-pipeline-extensions]]): output
  depends on pass order, which "render the dirty set topologically" does
  not express.
- **Transform feedback** (the ES 3.0 path to GPU simulation): non-pure by
  construction - a pass writes a buffer the next pass reads.

## The decision

Three coherent answers, in increasing cost (full argument in the analysis):

1. **Stay pure.** Document the invariant, keep rejecting cycles. Cheapest;
   forecloses trails, sims, progressive refinement, temporal AA.
2. **One imperative verb** (recommended there): targets created `manual`
   are never rendered by the flush; `renderTarget(id)` renders one, now, in
   call order. Non-idempotent passes become legal without infecting pure
   ones; ping-pong is a two-target loop the app steps from onFrame. A
   `copyTexture(src, dst)` (the GPU-side analog of uploadTexture) composes
   with it for seeding and history.
3. **Full command encoder.** Rejected in the analysis (what-not-to-take).

## Gates

Do not build before deciding: loadOp, multi-pass targets, ping-pong
feedback, transform feedback, any accumulation effect. A `clear: false`
under the current model produces output that silently depends on flush
count - the same invisible-failure shape as the propagation bug of
2026-07-29 ([[gpu-target-dependency-propagation]]).

Related shipped precedent: the window effect's `uPrevious` history layer is
a non-pure pass living outside the target system
(okf/plans/root-layer-effects.md).
