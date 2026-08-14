---
title: GPU purity contract and the manual render verb
description: Resolves the gpu-purity-decision backlog item with option 2 - the retained target model stays pure and documented, and one imperative escape hatch (render "manual" targets stepped by renderTarget) legalizes the accumulation/feedback class without infecting pure targets.
created: 2026-07-30
completed: 2026-07-30
---

# GPU purity contract and the manual render verb

Plan for okf/backlog/gpu-purity-decision.md, following the recommendation in
okf/notes/gpu-review.md (structural divergence section).

## The decision (stage 1)

Option 2 of the three in the backlog item: the model stays pure AND gains
exactly one imperative verb. The invariant is now stated in the docs
(docs/core.md render contract, flux-types gui/gpu.d.ts header):

> A target's contents are a pure function of its inputs. The runtime renders
> it zero, one, or many times per flush, at its discretion, so rendering
> twice must be indistinguishable from rendering once.

Full command encoders remain rejected (gpu-review, what-not-to-take).

## The semantics contract

- **Manual targets are sources, not flush members.** `TargetSpec::manual`
  excludes the target from the flush graph entirely (`flush_dirty` builds
  edges only over pure targets): the flush never renders one and never
  propagates *through* one. After an explicit render the target's id seeds
  the dirty set, so pure targets sampling it re-render at the next flush -
  the same shape as an `uploadTexture` content change.
- **`renderTarget(id)` renders one target, now, in call order.** It is a
  fire-and-forget RasterCmd on the single ordered channel, so ordering
  against frames, writes, and blocking readbacks is free: two renders run
  twice in order, and a `readTexture` issued after a render observes it.
  The handler flushes first (the pixel-observer rule), so the pass samples
  fresh inputs.
- **Writes to a manual target do not mark dirty.** `setShaderParams`,
  `setShaderTextures`, `setDrawCount`, and `writeBuffer` fold state but
  render nothing and propagate nothing; the values apply at the next
  explicit render.
- **Creation and resize clear instead of render** (`ShaderTexture::clear`:
  clear color + depth, full Impeller save/restore). A manual pass may be
  non-idempotent, so it must never run implicitly; the clear keeps undefined
  storage unobservable. Resize therefore loses accumulated history (new
  storage) - the app re-seeds.
- **Every render still clears first** (mesh-pass semantics are unchanged),
  so stage 2 state lives across a ping-pong pair, never in one target's own
  previous pixels; in-place accumulation is exactly the stage-3 loadOp item,
  not something renderTarget alone provides.
- **Cycle rule relaxed, not removed.** `samples_transitively` takes the
  manual set as barriers: a sampling cycle is rejected only when every
  member is flush-rendered. Ping-pong is two manual targets bound to each
  other. Direct self-binding still throws for every target - a pass sampling
  the very texture it writes is same-pass GL feedback (undefined pixels),
  not a scheduling problem.
- **Determinism under load shedding and playback.** Only Frame commands are
  ever shed; RenderTarget commands always execute, in call order, in both
  interactive and capture/playback mode. Steps count calls, never frames or
  flushes - which is exactly why cross-flush accumulation is manual-only.

## Surface

- flux:gpu: `render?: "auto" | "manual"` on `createShaderTarget` AND
  `createPipeline` (shared `collect_target_spec`; vocabulary validated at
  the call site), plus `renderTarget(id)`. The fragment-fused `createShader`
  does not take it.
- `loadOp?: "clear" | "load"` on the same two creates (stage 3): "load"
  keeps the color contents under each draw - single-target accumulation
  with the pipeline's `blend: "add"`. Manual-only, enforced in alloy
  Context (`validate_load`); depth stays per-render scratch and always
  clears; creation/resize still clear.
- `copyTexture(src, dst)` (stage 3): overwrite manual target `dst` with any
  texture's pixels via a shared lazily-compiled fullscreen copy program - a
  sampling draw into dst's FBO, never a blit. Exact and same-size only
  (mismatch throws; scaling copies are ordinary passes); dst must be
  manual, src != dst; counts as a pass in stats; seeds dst dirty like an
  upload. Row order verified preserved end to end.
- Named `render`, not `manual`: the core layer's create options already use
  `manual: true` for the lifetime opt-out (skip auto-free), a genuine
  collision at the same call site. `render` says who renders, `manual` says
  who frees.
- @solidrt/core/gpu re-exports `renderTarget` raw; the option types carry
  `render` through.
- `get_gpu_resources` reports `manual` per pipeline entry.

## Stages

1. **Record the decision** - DONE 2026-07-30 (this file, docs contract
   statements, backlog item closed).
2. **The verb** - DONE 2026-07-30: gpu_graph unit tests (barrier walk) plus
   `flux/examples/gpu_manual.rs`, a headless assertion probe over the whole
   contract (no self-render, call-order readback, folded params, pure
   samplers as live deps across explicit renders, ping-pong evolution, and
   all four guard rails), green on Linux/Mesa.
3. **First consumers** - DONE 2026-07-30 except the visual run:
   - `loadOp: "load"` on manual targets only - DONE, probe-verified
     (accumulation across renders, rejection off manual, bad-word throw).
     Within a future pass list, a pure target's first pass must clear.
   - `copyTexture(src, dst)` - DONE, probe-verified (exact content + row
     order, pure-sampler propagation, all three guards). Sampling draw,
     never a blit (the Adreno 0x502 lesson).
   - `examples/trails/`: ping-pong fading trails stepped from onFrame -
     verified live on a Linux client 2026-07-30: trails paint and fade,
     60 fps, gpuPasses rate 60.6/s at 60 fps (exactly one step per frame,
     no redundant target re-renders), fenceTimeouts 0, textures 2 (the
     pair), no orphan nodes.
   - transform feedback lands inside this model when it comes (a TF pass is
     manual by construction).

## Traps

- Never let the flush render a manual target "just once to initialize" -
  creation clears instead; a manual pass running outside renderTarget is the
  bug class this whole decision removes.
- The barrier walk relaxation must never drop the direct self-bind check
  (same-pass feedback is a GL hazard, independent of scheduling).
- uPrevious in the window shader remains a separate non-pure mechanism
  outside the target system (okf/plans/root-layer-effects.md); do not
  conflate the two.
