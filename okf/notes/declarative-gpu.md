---
title: Declarative GPU surface
description: Survey of declarative GPU prior art against the current texture/gpu API; conclusion is a Shader component as sugar, subtree effects as the real gap, and keeping the manual path imperative.
created: 2026-08-03
---

# Declarative GPU surface

The GPU API today is a `<texture>` element fed by imperative calls
(`createPipelineTexture`, `setShaderParams`, `renderTarget`, ...). The
question: does it make sense to make any of the GPU part more declarative,
and if so how. Assessed 2026-08-03 from a codebase review plus a prior-art
survey (kept below for reference).

## Recommendation

Narrowly yes, in two places; everything else should stay as it is.

1. **A declarative shader component (sugar, cheap).** The dominant example
   pattern is boilerplate with a fixed shape: create a fragment target in
   the component body, render `<texture src={id} params={...}>`. A
   `<Shader fragment={src} width height params textures>` component
   collapses it. All machinery exists: it is `createShaderTextureMemo`
   composed with `<texture>`. Per layering rules it belongs in
   `packages/components`, not core; the primitive stays first-class.
   Start fragment-only; pipeline options (buffer/attributes) only with a
   driving use case.
2. **Subtree effects (the real gap, grows the renderer).** "Apply this
   shader to this subtree's rendered content" has no good expression
   today: `captureSnapshot` is a one-shot promise, and live content
   cannot be hand-pumped into a sampler without fighting the flush. This
   is the one place declarativity is not sugar but the only sane
   interface, and prior art has converged on it (see survey: effect
   modifiers on views, item-as-sampler binding). The substrate exists
   (`repaintBoundary="snapshot"` renders a subtree to a texture tier) and
   the backlog item is open (../backlog/subtree-effects.md). Shape: a
   prop on the element taking `{ program, params }`, aligned with the
   existing `<window shader>` prop, not a wrapper element.
3. **Deferred: reactive creation for pipeline targets.** A
   `createPipelineTextureMemo` sibling is mechanical but has more rebuild
   edges (buffers, attributes) and no driving use case yet. Wait.

Non-goals, deliberately:

- **Do not declarativize the manual/feedback path.** `render: "manual"` +
  `renderTarget`, per-frame `writeBuffer`, `setDraw` at animation rate
  stay imperative. Every surveyed system either started with or was
  forced to add exactly this frame-rate escape hatch; the purity decision
  (../backlog/gpu-purity-decision.md, option 2) already drew this line.
- **No shader-graph authoring layer** (building shaders from JS node
  expressions). Those exist to abstract over multiple backends; solidrt
  has one shader language (GLSL ES 3.00) and keeps standard vocabulary.
- **No pass-as-component runtime** (every pass/dispatch/binding a
  component). That model exists to serve arbitrary 3D pipelines and
  needed a bespoke reactive runtime; out of scope, and Solid's
  fine-grained signals already provide the granularity it was invented
  for.

## Baseline: what is already declarative

The engine half already is. The unit of GPU work is a retained target,
not a recorded pass (../notes/gpu-review.md, "a retained target vs a
recorded pass"): sampler bindings form a live dependency graph and the
dirty flush re-renders consumers in topological order. That is a frame
graph, derived implicitly from `setShaderTextures` bindings.

On the JS side:

- Creation is imperative but lifetime-reactive: owner-scoped auto-free on
  every create (packages/core/src/gpu.ts).
- Updates already flow declaratively: the idiomatic loop sets a signal
  and `<texture params={{ uTime: time() }}>` carries it; params are
  deferred to build so a fast signal stays paced to real repaints.
  `<window shader={...}>` is the same shape.
- `createShaderTextureMemo` is the one fully reactive wrapper (reactive
  over source/size/params/bindings, routing in-place update vs rebuild).
- The one imperative island is deliberate: manual targets are sources,
  not flush members (../plans/gpu-render-verb.md).

So "more declarative" can only mean: sugar over the create-then-bind
boilerplate, and new renderer capability for effects over live tree
content. Both are covered by the recommendation.

## Prior art survey

One lesson per system, in solidrt vocabulary:

- **Use.GPU** (acko.net "The GPU Banana Stand"): the maximal position:
  passes, dispatches, bindings and shader composition are all components
  in a bespoke React-like runtime ("Live"). Lesson: the runtime had to be
  invented because React reconciliation is too coarse for GPU work;
  Solid's signals already are that runtime. The author explicitly trades
  performance for composability and targets arbitrary pipelines, which is
  out of solidrt's scope.
- **Reactive WebGPU** (mighdoll.dev): fine-grained reactivity pays off
  for expensive rebuild-on-config-change and for cleanup; warns that
  reactivity is contagious and hard to debug when overextended. Validates
  `createShaderTextureMemo` as the right granularity and the "wait for a
  use case" stance on more memo variants.
- **react-three-fiber**: declarative scene structure with one
  load-bearing rule: anything changing at frame rate bypasses the
  declarative layer (refs and direct uniform writes in the frame
  callback). The trails example independently converged on this exact
  split (plain mutable pointer object, imperative `renderTarget`, signal
  only for the front/back swap).
- **QML ShaderEffect**: the closest match to the `<texture>` model: a
  declarative element whose properties auto-bind to uniforms, and other
  UI items bind as `sampler2D` inputs. Qt still grew `UniformAnimator`
  to bypass the property system at animation rate: the escape-hatch
  lesson again. Item-as-sampler is the subtree-effects capability.
- **SwiftUI shader modifiers** (`colorEffect` / `distortionEffect` /
  `layerEffect`): declarative "apply this shader to this view's rendered
  content", extra arguments become uniforms. The taxonomy (color-only,
  coordinate remap, full layer sampling) is a useful reference when
  designing subtree effects.
- **regl**: all state for a draw in one options object, no hidden state
  machine. The fused `createPipelineTexture(vs, fs, w, h, params, opts)`
  already has this shape; nothing to adopt.
- **Three.js TSL**: declarative shader authoring as JS node graphs,
  compiled to GLSL or WGSL per backend. Exists to solve multi-backend
  portability; buys nothing for a single-backend runtime with standard
  GLSL. Rejected above.

## Open questions

- `<Shader>` component: exact prop set, and whether `textures` accepts
  reactive ids (it should; the memo already handles rebinding).
- Subtree effect prop: name (`effect` vs `shader`), interaction with
  `repaintBoundary` tiers, and whether hit-testing sees pre- or
  post-effect geometry (coordinate-remapping effects distort what the
  user sees vs where children think they are).
- Whether root-layer effects (../backlog/root-layer-effects.md) and
  subtree effects share one mechanism once both exist.
