---
title: GPU stack review
description: "Merged review of the GPU stack: where it stands (retro-class 3D feasible, statuses refreshed), how its shape compares to WebGL2 and WebGPU (a retained pure target vs a recorded pass; deliberate divergences are the improvements, path-of-least-resistance ones the regressions), ranked lessons, capability gaps by workload, and a file split proposal. Its shortlist closed 2026-07-31, all eight items; see the status section for what shipped and what the do-order is now."
created: 2026-07-30
---

# GPU stack review

One document, merged 2026-07-30 from two earlier analyses so the direction
for GPU support reads in one place: "GPU stack maturity" (2026-07-15, written
from the Doom port the day vertex pipelines shipped - what can you build with
this) and "GPU API against WebGL2 and WebGPU" (2026-07-30, the day the
pipeline object-model split landed - is the shape right). Statuses from the
maturity half are refreshed to 2026-07-30; its workload framing survives in
"capability gaps by workload" below.

Also absorbed: the working session (2026-07-30) reviewing the last five
completed issues, which first asked the WebGPU question. Two of its findings
carry over unchanged - untyped numeric ids (lesson 3) and
texture-id-as-currency (#6 in the beats-both list) - one is closed by the
pipeline split, and one got worse rather than better as a result (see
naming). Its framing is the one to keep: **the divergences that were chosen
deliberately are the improvements, and the divergences that came from the
path of least resistance are the regressions.** That split is healthy - it
means the design attention went where it mattered - and every item below
sorts cleanly into one side or the other.

Scope: `alloy/src/shader.rs`, `alloy/src/texture.rs`, `alloy/src/context.rs`,
`alloy/src/raster.rs`, `flux/src/plugins/gui/texture.rs`,
`packages/core/src/gpu.ts`, `packages/flux-types/gui/gpu.d.ts`.

## Where the stack stands (2026-07-30)

Snapshot as this document was written; the next section records what landed
after it.

Crossed from "shader toy" to a minimal but genuinely usable 3D pipeline on
2026-07-15 - the Doom port retired its fragment-only raycaster for a real
mesh renderer that day (walls, flats, sky, animated doors, depth-occluded
sprites, 60fps, jsMs ~0.3) and remains the stack's most demanding consumer
and de-facto acceptance test. Since then six of that assessment's gaps have
closed. What exists now:

- **Fragment shaders** (`createShader`): GLSL ES 3.00 fullscreen passes into
  FBO-backed RGBA8 textures adopted into Impeller - a shader output is a
  texture id usable anywhere in the UI tree. The complete-source dialect
  (own `#version` passes through untouched, vec3 `iResolution`) runs
  unmodified Shadertoy code.
- **Vertex pipelines**: custom vertex+fragment GLSL over one interleaved
  float vertex buffer with name-resolved attributes, five topologies,
  optional private depth buffer, clear color, mutable draw count.
- **The split object model** (2026-07-30): `compileShader` -> `linkProgram`
  -> `createRenderPipeline` (draw state) -> `createShaderTarget` (target
  state), with `createShader`/`createPipeline` kept as fused conveniences.
  [gpu-pipeline-object-model](../done/gpu-pipeline-object-model.md)
- **Typed uniforms** (2026-07-29): `number | number[]` params dispatched by
  the reflected GLSL type (float/int scalars, vec2/3/4, mat4).
  [gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md)
- **Additive blend + explicit depthWrite** (2026-07-29): the additive-pass
  recipe is `{ depth: true, blend: "add", depthWrite: false }`, stated by
  the app, never inferred.
- **Sampler state** (2026-07-29): per-texture `filter` (linear|nearest) and
  `wrap` (clamp|repeat) declared at creation, honored by shader passes AND
  `<texture>` display - the retro/pixel-art path.
  [gpu-sampler-state](../done/gpu-sampler-state.md)
- **Dependency propagation** (2026-07-29): sampler bindings are live edges;
  the pull-based dirty flush renders affected targets in topological order,
  with synchronous cycle rejection at bind time.
  [gpu-target-dependency-propagation](../done/gpu-target-dependency-propagation.md)
- **Buffers**: `createBuffer` / `writeBuffer(offset)` / `destroyBuffer` plus
  `setDrawCount` cover dynamic geometry (Doom's sprites live in a dynamic
  tail of the level-mesh buffer); writes re-render dependents.
- **Pass timing, stage 1** (2026-07-30): `gpuPasses`/`gpuPassMs` in
  get_stats. [gpu-pass-timing](../done/gpu-pass-timing.md)
- **Engineering quality**: exhaustive GL state save/restore against
  Impeller's cached state on the shared context, `glGetError` after every
  pass; owner-scoped reactive lifetimes with `{ manual }` opt-out and
  `createShaderMemo`; headless self-asserting examples (`pipeline_blend`,
  `shader_uniforms`, `gpu_split`) pin the contracts.

The 07-15 verdict stands, sharpened: **early but disciplined** - the
implemented surface is small, correct, and verified on real clients; the
unimplemented surface is catalogued in the backlog rather than half-built.
The rest of this document is about whether the shape is right and what to do
next.

## Status 2026-07-31

The shortlist at the end of this document is closed: all eight items landed
between 2026-07-29 and 07-31, and ranked lesson 1 (load-op) arrived with item
3 rather than after it. What shipped, in shortlist order:

1. **Branded ids** - five brands in `flux:gpu`, re-exported through
   `@solidrt/core/gpu`.
   [gpu-branded-ids](../done/gpu-branded-ids.md)
2. **The pixel contract, written down** - one named contract in
   `gui/gpu.d.ts`, core `gpu.ts`, `docs/core.md` and scaffold AGENTS.md.
   [gpu-pixel-contract-docs](../done/gpu-pixel-contract-docs.md)
3. **The purity question: decided, option 2.** The invariant is documented,
   and targets created `render: "manual"` are excluded from the flush graph
   and stepped by `renderTarget(id)`, with `loadOp: "load"` (manual-only) and
   `copyTexture` alongside. Linux-verified: the trails example runs at 60 fps
   with exactly one pass per frame.
   [gpu-purity-decision](../done/gpu-purity-decision.md), plan
   okf/plans/gpu-render-verb.md
4. **Buffers held by `Rc`** - the ordered-destroy rule and its doc sentence
   deleted together.
   [gpu-buffer-lifetime](../done/gpu-buffer-lifetime.md)
5. **Call-site validation** - creates validate inside their blocking RPCs,
   fire-and-forget updates against UI-side mirrors; the
   strict-on-inactive-uniforms sub-case split out as
   [gpu-inactive-uniform-two-tier](../backlog/gpu-inactive-uniform-two-tier.md).
   [gpu-callsite-validation](../done/gpu-callsite-validation.md)
6. **Draw range and instancing** - `setDrawCount` replaced by
   `setDraw({ firstVertex, vertexCount, instanceCount })`, drawn via
   `glDrawArraysInstanced`.
   [gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md)
7. **Labels and limits** - a label on every create, surfaced in
   `get_gpu_resources` and raster-side error strings; `GpuLimits` queried at
   raster startup and checked at every create/bind/resize site with the limit
   named. [gpu-labels-limits](../done/gpu-labels-limits.md)
8. **The file split** - `shader.rs` into an alloy `gpu/` folder, flux
   `gui/texture.rs` renamed to `gpu.rs`, the RasterCmd enum, capture path and
   context DTOs lifted. [gpu-file-reorg](../done/gpu-file-reorg.md)

Also since, outside the list: target params became a positional argument
rather than an opts-bag key.

So the do-order is spent, and what remains sorts into four groups.

- ~~**Verification debt.**~~ **Cleared 2026-07-31** on five clients at once
  (Linux, Windows/ANGLE, three Android including the 2017 TV). See the
  verification section below.
- ~~**The naming decision**~~ **Decided and landed 2026-07-31**: the hard
  rename (`createShaderTexture`/`createPipelineTexture`/
  `createShaderTextureMemo`, no aliases) and `iTime` dropped from every
  preamble - the preamble now declares exactly what the runtime fills, and a
  time uniform is the app's own declaration. Rationale and touched files in
  [gpu-fused-create-refactor](../backlog/gpu-fused-create-refactor.md); its
  composition questions stay open there.
- **Capability**, in rough order: multi-pass into one target (unblocked twice
  over - the object-model split gave draw state a home, the purity decision
  gave it a legal shape), the multi-pass chain example
  ([gpu-example-gaps](../backlog/gpu-example-gaps.md), unblocked since 07-29
  and still unwritten), float texture formats and sampleable depth, then MSAA
  ([gpu-target-antialiasing](../backlog/gpu-target-antialiasing.md)). Lesson
  7's pipeline-side format validation lands with whichever of the last two
  arrives first: that is the moment a pipeline and a target can first
  disagree invisibly.
- **Outside the GPU stack**, relative mouse input still outranks all of it
  for the first-person class
  ([relative-mouse-input](../backlog/relative-mouse-input.md)).

Two ranked lessons had no backlog home when the shortlist closed, and were
filed on 07-31: lesson 8 as
[gpu-per-binding-sampler](../backlog/gpu-per-binding-sampler.md) and lesson
11 as
[gpu-async-compile-readback](../backlog/gpu-async-compile-readback.md).

### Runtime verification, 2026-07-31

Everything that had landed typecheck-verified was exercised on five connected
clients simultaneously - Linux (this machine), Windows/ANGLE-D3D11, and three
Android devices including the 2017 MediaTek TV, all on the same release build.
Probes: `sandbox/gpu-verify.tsx` and `sandbox/gpu-manual-verify.tsx`
(throwaway), plus `gpu-particles.tsx` and the trails example.

- **Limits** - every device reported its own ceilings, not a floor:
  16384/32/16 (Linux), 16384/16/16 (Windows), 16383/**128**/32, 16384/16/16,
  and 8192/16/16 (TV). Oversize creates threw naming the limit
  ("16385x16 exceeds this device's max texture size (16384)"), as did an
  over-limit sampler binding ("33 sampler inputs exceed this device's texture
  unit limit (32 per pass)"). The 16383 device is the argument for the check
  being dynamic.
- **Labels** - present on every texture, buffer, program, render pipeline and
  target in `get_gpu_resources`, and inherited by a target's output texture.
- **Sampler state** - all four cases identical on Linux, Windows and the TV:
  a 4x4 source upscaled to 96px displays hard-edged under `filter: "nearest"`
  and smooth under `"linear"` (the display path), and a shader sampling at
  `vUV * 3` tiles 3x3 under `wrap: "repeat"` and smears the edge under
  `"clamp"` (the sampling path). Impeller does not clobber it.
- **Dependency propagation** - a two-stage chain where stage B samples A and
  has no params of its own: B's pass count tracked A's *exactly* (2609/2609
  Linux, 3550/3550 then 7194/7194 TV, 10177/10177 Windows) and B's pixels
  moved with A's phase, while two targets sampling only static data textures
  stayed at **1 pass** for the app's whole life. Live edges propagate, dead
  edges cost nothing.
- **Pass timing** - the counters are real and 1:1 with frames: 2604 passes
  over 43.55 s of frame clock on Linux (59.8/s at 60 fps) and 3644 over
  74.42 s on the TV (48.96/s on a ~50 Hz panel). No redundant re-renders.
- **The manual render verb** - manual targets, `renderTarget` call-order
  stepping, and mutual ping-pong feedback (a cycle the pure graph rejects)
  all work on every device including the TV. `loadOp: "load"` was proven by
  A/B: two targets sharing one pipeline and one draw, differing only in
  loadOp, gave an accumulated arc ("load") against a single dot ("clear").
  That is the first client-side exercise of load-op, and it holds on a
  tile-based mobile GPU, which was the open risk.
- **Additive blend / points topology** - `gpu-particles.tsx` renders
  correctly on Linux and the TV.

No GL errors, shader errors or pass errors on any client.

The one number worth carrying forward is the TV's per-pass cost. A *trivial*
128x128 fragment pass costs 0.5-0.7 ms there against 0.10 ms on Linux and
0.02 ms on the Windows box - so the manual probe's four small passes per
frame spend ~2.2 ms of a 20 ms budget before the app draws anything. The
~8x TV factor in [device-perf-model-docs](../done/device-perf-model-docs.md)
holds for GPU passes too, and pass *count* is the budget on that class of
device, not pass size.

Those TV figures were measured twice, the second time from a freshly loaded
app after the device had been restarted mid-session, and reproduced: 0.714
and 0.463 ms/pass for the two chain stages against 0.105 and 0.044 on Linux
(a 6.8x ratio on the same shader), with the pass rate at 49.3/s on a 48 fps
panel. Method note for anyone repeating this: measure the DELTA between two
per-target `passes`/`passMs` readings within one app run. The `gpuPasses`/
`gpuPassMs` counters in get_stats are client-lifetime - they survive app
reloads and span every app the client has run - so a cumulative ratio taken
from them attributes other apps' work to the one in front of you, and a
single per-target average still folds in the cold first passes.

## Summary

The object model is now closer to WebGPU than to WebGL, and in several places
it is better than both. The one deep divergence is not a missing feature: it is
that **a solidrt target is a retained object that redraws itself, where in both
standards a draw is a recorded command that happens once**. That model is a
genuine improvement for the reactive UI case and it is what makes the sampler
dependency graph possible at all. It also carries an unstated invariant - every
pass must be a pure function of its inputs - and *every remaining deferred
feature that people keep asking for breaks that invariant*: accumulation,
ping-pong feedback, multi-pass into one target, ordered draws. That is the
thing to decide before adding any of them, and it is the main finding here.

Beyond it, the ranked lessons are small and mostly cheap: load-op, call-site
uniform validation, branded ids, labels, limits, one lifetime inconsistency,
a coordinate-and-pixel contract that is documentation rather than code, and
mipmaps. Two of the three regressions are pure type-and-name work with no
engine change at all.

## Vocabulary map

| solidrt | WebGL2 | WebGPU |
| --- | --- | --- |
| `compileShader(stage, src)` | `createShader` + `shaderSource` + `compileShader` | `createShaderModule` |
| `linkProgram(vs, fs)` | `createProgram` + `attachShader` + `linkProgram` | (folded into the pipeline) |
| `createRenderPipeline(program, drawState)` | - (global state machine) | `createRenderPipeline` |
| `createShaderTarget(pipeline, w, h, spec)` | FBO + texture + viewport + clear + `drawArrays` | `GPUTexture` + `createView` + `beginRenderPass` + `setPipeline` + `draw` + `submit` |
| `createShader(fragSrc, w, h, ...)` | all of the above, fused | all of the above, fused |
| texture id (u64) | `WebGLTexture` | `GPUTexture` (+ implicit full view) |
| `createBuffer` / `writeBuffer` | `createBuffer` + `bufferData` / `bufferSubData` | `createBuffer({usage})` / `queue.writeBuffer` |
| `params: {name: value}` | `getUniformLocation` + `uniform*` | uniform buffer in a bind group |
| `textures: {name: id}` | `activeTexture` + `bindTexture` + `uniform1i` | sampled-texture entries in a bind group |
| `filter`/`wrap` on the texture | `createSampler` (separate object) | `GPUSampler` (separate object) |
| dirty flush | - (app calls `draw`) | - (app records and submits) |
| - | transform feedback, queries, MRT, instancing | compute, storage buffers, indirect draw |

Two rows are worth staring at. `createShaderTarget` collapses five WebGPU
concepts into one call, which is the solidrt lens working correctly. And
`createShader` occupies the name WebGL uses for `compileShader`'s job while
returning a *texture* - see naming, below. Note also what the right-hand
column has that the left does not (last row): compute and storage buffers are
out of reach at the ES 3.0 level regardless, but transform feedback is the
standard's own answer at exactly this hardware level and has no entry here.

## Where solidrt already matches WebGPU, or beats both

These are the parts to protect when acting on anything further down.

1. **Pipeline state is an immutable object separate from the program and from
   the target.** This is the WebGPU model exactly, and it is the single
   biggest thing WebGL got wrong (draw state is global, mutable, and invisible
   at the call site). Landed 2026-07-30. `PipelineDesc` even makes the invalid
   depth/depthWrite combination unrepresentable, which WebGPU does not
   (`depthWriteEnabled` without a `depthStencil` format is a validation error
   there, i.e. checked rather than prevented).

2. **Creation errors throw synchronously at the JS call site.** WebGPU cannot
   do this: it is a browser API serving untrusted content across a process
   boundary, so validation errors arrive asynchronously through
   `pushErrorScope`/`popErrorScope` and the app that made the mistake is long
   gone. WebGL polls `getError`. solidrt is a single trusted app with a
   blocking RPC to the raster thread, so a bad shader fails on the line that
   wrote it. That is strictly better and it should stay a design rule: **any
   new GPU call that can fail should fail at the call site, not in a log.**

3. **Destroy is frame-safe and order-free for textures, programs and
   pipelines.** WebGPU's `destroy()` makes subsequent use a validation error;
   WebGL keeps deleted objects alive only while bound. solidrt reclaims a
   texture id once the render tree stops referencing it, and holds programs
   and pipelines by `Rc`, so `destroy` in the same update that repoints
   `<texture src>` never paints a blank frame. Neither standard offers this,
   and it is exactly what a reactive tree needs. (Buffers are the exception -
   see lesson 9.)

4. **Ids are indirected through a registry rather than handed out as
   device-bound handles.** Under-exploited today, but it is what makes
   transparent context-loss recovery *possible* here and impossible in both
   standards (see lesson 10).

5. **Pull-based dependency propagation over the sampler graph.** Nothing in
   either standard has an analogue - both require the app to re-record and
   re-submit every frame. It is the right answer for a demand-driven
   renderer and it is why `<texture src={shaderId}>` "just works".

6. **One texture id is the universal currency.** In WebGL an uploaded image, an
   FBO colour attachment and a camera frame are three different assembly jobs
   with three different setup paths; WebGPU unifies the type but still makes
   you build a view and a bind group entry per use. Here they are one id, and
   `<texture src>`, a `sampler2D` binding, `readTexture`, `blendMode` stacking
   and `captureSnapshot` output all compose without adapters. That unification
   is what makes shader chains cheap to write, and it is a bigger practical
   win than any single feature on the deferred list. It is also what makes
   lesson 3 matter: the currency is only safe if the type system enforces
   which currency it is.

## The structural divergence: a retained target vs a recorded pass

In WebGL and WebGPU alike, the unit of work is an *event*: `drawArrays`, or
`pass.draw()` followed by `queue.submit()`. Nothing persists. The app decides
when, how often, in what order, and into what.

In solidrt the unit is a *thing*: a target holds its program, its buffer, its
uniforms, its clear color and its draw count, and re-renders itself whenever
the flush decides its inputs moved. The app never says "draw".

For the workload this was built for, solidrt's model is better - it removes
the entire per-frame re-record loop, it makes a shader output an ordinary
value in the UI tree, and it makes multi-node chains correct without the app
sequencing anything.

It works because of an invariant nobody has written down:

> **A target's contents are a pure function of its inputs.** Rendering it
> twice is indistinguishable from rendering it once, so the runtime is free to
> render it zero, one, or many times per frame.

Every deferred feature on the list breaks that invariant:

- **Accumulation / load-op** ("don't clear"): output depends on previous
  output. Rendering twice differs from rendering once.
- **Ping-pong feedback** (A reads B, B reads A over frames): a cycle, which
  the graph currently *rejects outright* precisely because the pull model
  cannot order it.
- **Multi-pass into one target** (N pipelines, shared depth): output depends
  on pass order, which "re-render the dirty set in topological order" does not
  express.
- **Ordered/indirect draws**: same.

Worth noting that WebGL2 has its own answer at exactly this hardware level -
**transform feedback**, the ES 3.0 way to evolve state on the GPU (particles,
flocking, physics) without a CPU round trip - and it is non-pure by
construction: a pass writes a buffer that the next pass reads. So the feature
solidrt would most want for GPU simulation lands on the same fault line rather
than beside it, which is more evidence that the fault line is the thing to
settle rather than route around. Today that workload has no path here except
readback or encoding state into textures.

So the sequencing decision is not "which extension next". It is: **does the
model gain an explicit-submission escape hatch, or does it stay pure?** Three
coherent answers, in increasing cost:

- **Stay pure.** Document the invariant, keep rejecting cycles, and tell apps
  that accumulation is a two-target manual swap driven from `onFrame`. Cheapest
  and consistent. Costs: trails, fluid/particle sims, progressive
  refinement, temporal AA - a whole class of effects stays out of reach.
- **Add one imperative verb.** Something like `renderTarget(id)` (WebGPU's
  `submit`, reduced to one target): the target still exists, but a target
  marked `manual` renders only when asked, never from the flush. That is one
  concept, it makes non-idempotent passes legal without infecting the pure
  ones, and ping-pong becomes an ordinary two-target loop the app steps
  itself. **This is the recommended shape** - it borrows WebGPU's explicitness
  exactly where the retained model runs out, and nowhere else. A
  `copyTexture(src, dst)` (WebGPU's `copyTextureToTexture`) composes with it
  for seeding and history buffers - the GPU-side analog of `uploadTexture`,
  an external write into a target that the graph already knows how to order.
- **Go full command-encoder.** Do not. See "what not to take".

Note the same tension already shows up in a shipped feature: `uPrevious` in
the root-layer effect is a history buffer, i.e. a non-pure pass, and it lives
outside the target system entirely.

## Ranked lessons

Grouped by theme, roughly by (value to apps) / (cost to build). Each names what
the standards do and what is worth taking. The numbering here is for reference,
not a work order - the shortlist at the end is the do-order.

### 1. Load-op on the target (WebGPU `loadOp: "clear" | "load"`)

WebGPU makes clear-vs-preserve an explicit per-attachment choice, and
`storeOp: "discard"` an explicit throw-away. WebGL leaves it to whether you
call `glClear`. solidrt always clears: the mesh path clears colour (and depth)
unconditionally, the fragment path relies on the covering triangle.

Value: this is the unlock for the whole non-pure class above, and it is also
what "several draws into one target" needs (pass 1 clears, passes 2..N load).
Cost: a boolean plus the model decision in the previous section. **Do not ship
it before that decision** - a `clear: false` under the current pull model
produces output that silently depends on how many times the flush happened to
run, which is the same invisible-failure shape as the propagation bug of
2026-07-29.

### 2. Validate uniforms at the call site, not at render

WebGL's worst ergonomic property is that `getUniformLocation` returns `null`
for a typo and setting it is a silent no-op. WebGPU eliminated the class:
bindings are structural (`@group`/`@binding`) and a layout mismatch is a
creation error.

solidrt inherited the WebGL flaw and softened it: an unknown param name is
dropped silently at render, and a component-count mismatch logs a warning on
the raster thread - `apply_uniform`, `shader.rs:726`. Neither reaches the app.
But the program's reflected uniform table exists at link time, and the UI
thread already mirrors enough graph state to reject sampler cycles
synchronously. Mirroring `name -> (type, components)` per program lets
`setShaderParams` / the `params` prop throw on the line that wrote the typo,
which is both what the standards converged on and what the project's own
dev-validation policy says.

Cheap, high daily value, and it closes a documented silent-drop class rather
than adding a feature.

The same principle covers geometry. WebGL *mandates* draw-time bounds
validation - `drawArrays` past the end of an attribute buffer is
`INVALID_OPERATION`, its one security-driven check that is also pure
ergonomics - while raw ES 3.0 leaves out-of-bounds vertex fetch undefined.
solidrt validates `writeBuffer` against the UI-side `buffer_sizes` mirror at
the call site, but not the draw: an explicit `vertexCount` over the buffer's
capacity is taken verbatim (`resolve_target_mesh`, raster.rs), and
`setDrawCount` checks only that the id is a pipeline texture. The stride is
known from the pipeline desc and the buffer size is already mirrored, so
`count * stride <= size` is one more call-site check in the existing style,
closing an actual undefined-behaviour hole rather than an ergonomic one.

### 3. Distinguishable handles (`WebGLTexture`, `GPUBuffer`)

WebGL 1.0 got this right in 1996-era JS with no type system at all: a texture
is a `WebGLTexture` and a buffer is a `WebGLBuffer`, distinct opaque objects
that cannot be passed to each other's calls. WebGPU kept it. It is the one
place where both standards are *less* raw than solidrt.

Here every handle is a `number`, across four separate id spaces - textures
(including every target), buffers, shader stages, programs, render pipelines -
and TypeScript cannot tell them apart. `destroyBuffer(textureId)` typechecks.
So does `setDrawCount(bufferId, 3)`, `createShaderTarget(programId, ...)` and
every other cross-space slip; the id spaces all start at 1 and count up, so a
wrong id is usually a *valid* id in the wrong space, which lands as a
mystifying runtime error or, worse, an operation on an unrelated resource.

Branded types close it with no runtime cost, no API change and no engine work:

    export type TextureId = number & { readonly __texture: unique symbol }
    export type BufferId = number & { readonly __buffer: unique symbol }

Applied in `packages/flux-types/gui/gpu.d.ts` and re-exported through
`packages/core/src/gpu.ts`, this is the cheapest available improvement in the
whole surface, and the only ranked item that is purely a `.d.ts` edit. The one
design question is whether the brands are exported for apps to name (they must
be, to write a `let ids: TextureId[]`), which is ordinary.

### 4. Object labels (WebGPU `label` on every object)

Every WebGPU object takes a `label`, which appears in error messages, in
`popErrorScope` results, and in captures. WebGL bolted on `KHR_debug` late and
few use it.

solidrt has numeric ids and a genuinely good `get_gpu_resources` MCP tool
looking at them. A `label?: string` on each create, surfaced in
`GpuTextureInfo`/`GpuRenderPipelineInfo`/`GpuBufferInfo` and in raster-side
error strings, turns "target 7 sampling buffer 3" into "bloom-h sampling
particle-verts". Near-zero cost, pays off the first time anyone debugs a chain
of six targets.

### 5. Limits, queryable (WebGPU `adapter.limits`, WebGL `getParameter`)

Both standards expose the device ceiling, because every one of these is a hard
per-driver cliff: max texture size, max texture image units, max vertex
attribs, max renderbuffer size. `Flux.capabilities` answers "does this feature
exist", not "how big can it be".

Nothing in alloy queries any limit today (no `MAX_TEXTURE_IMAGE_UNITS`,
`MAX_TEXTURE_SIZE` or `MAX_VERTEX_ATTRIBS` anywhere). Two concrete holes:

- A target larger than the driver's max fails as `framebuffer incomplete:
  0x8cd6` rather than "texture size 16384 exceeds this device's limit 8192".
- `run_pass` assigns sampler inputs to texture units by enumeration index with
  no cap; past the fragment unit limit (16 minimum on ES 3.0) the extra binds
  fail and the pass draws with garbage.

A small `gpu.limits` object plus one bounds check at create, with the message
naming the limit, is the whole fix.

### 6. Draw parameters as data (WebGPU `draw(count, instances, first, firstInstance)`)

solidrt draws `[0, drawCount)`, always, one draw per target. Two of the four
WebGPU arguments are worth having:

- **`first`**: sub-range draws from a shared buffer. The Doom port already
  wants this - it keeps sprites in a dynamic tail of one buffer and can only
  express "draw the first N", never "draw the tail".
- **`instanceCount`**: the standard answer to particles, tiles, and repeated
  meshes without duplicating vertices. ES 3.0 has `drawArraysInstanced` and
  `gl_InstanceID` natively.

Both are additive under the current pure model (they change *what* is drawn,
not whether the pass is idempotent), which makes them the safest items on this
list. `first` is trivial.

### 7. Format on the pipeline, so target compatibility can be validated

A WebGPU pipeline declares `fragment.targets[].format`, `depthStencil.format`
and `multisample.count`, and the pass validates the attachment against them.
solidrt's pipeline declares none of this because there is exactly one format
(RGBA8), no MSAA, and depth is auto-provisioned per target from a pipeline
boolean.

That is the correct simplification *today* and it should not be pre-built.
What is worth recording now: the moment float targets
([extensions](../done/gpu-pipeline-extensions.md)) or MSAA
([target-antialiasing](../backlog/gpu-target-antialiasing.md)) land, a
pipeline and a target can disagree, and the disagreement is invisible in GL
(it renders wrong, or the FBO goes incomplete with a hex code). The lesson is
the validation surface, not the fields: whatever format vocabulary arrives
goes on the pipeline, and `createShaderTarget` checks it at the call site.

The auto-provisioned depth renderbuffer is a genuinely nicer design than
WebGPU's here - "pipeline says it depth-tests, every target gets matching
depth storage" removes a whole category of mismatch. Keep it.

### 8. Separate the sampler from the texture (WebGL2 sampler objects, `GPUSampler`)

Both standards deliberately made the sampler independent of the texture -
WebGL2 added sampler objects *specifically* to undo the WebGL1 fusion, and
WebGPU never had it. Reason: the same texture legitimately wants different
sampling in different passes.

solidrt fuses `filter`/`wrap` into the texture id (2026-07-29) - the WebGL1
model - while implementing it with the WebGL2 machinery (four shared sampler
objects bound per unit, `SamplerCache`). The fusion is a deliberate solidrt-lens
call and it is right for the common case: one texture, one look, and it makes
`<texture>` display and shader sampling agree by construction, which a
separate sampler object cannot.

The escape hatch is missing, though, and the case is real: a nearest-filtered
pixel-art atlas cannot be sampled linearly by a blur pass, and a clamped
target cannot be tiled by one consumer. Per-binding override -
`textures: { uTex: { id, filter: "linear" } }` - costs almost nothing because
the cache is already keyed by state and bound per unit. Demand-gated, but
cheap when demanded.

### 9. One lifetime rule, not three

Both standards have exactly one: WebGPU refcounts internally and makes
use-after-`destroy()` a validation error; WebGL keeps deleted objects alive
until unbound. solidrt has three:

- textures: deferred reclaim once the tree stops referencing them, any order safe;
- programs and pipelines: `Rc`, any order safe;
- **buffers: manual, ordered** - "Destroy pipelines drawing from it first"
  (`context.rs:591`). Destroy out of order and the VAO keeps the GL storage
  alive, so targets silently keep drawing stale geometry while writes to the
  id error.

That third rule is a documented footgun in an API whose other two id spaces
made the same problem structurally impossible. Making targets hold their
buffer by `Rc` the way they hold their program deletes the rule, the doc
sentence and the failure mode together. This is the cheapest correctness item
on the list.

### 10. Context loss as a contract - and the advantage solidrt has

WebGL fires `webglcontextlost` (cancellable) then `webglcontextrestored`, and
the app rebuilds everything. WebGPU resolves `device.lost` and every object is
dead. Both standards *require* the app to recreate its resources, because the
handles it holds are device-bound.

solidrt hands out registry ids, not handles. The registry knows each texture's
size and sampler state, each target's pipeline, spec, sampler bindings and
last params, each buffer's size. That means recovery can be **transparent**:
recreate the GL objects behind the same ids, re-render the targets from their
retained specs, and the only thing the app must supply again is content it
alone has (data-texture pixels, buffer contents). Neither standard can offer
this, and it falls out of a decision already made for other reasons.

Worth recording in [gpu-context-loss](../backlog/gpu-context-loss.md), which
is currently at "detect and exit". The app-visible half still needs to exist
(the standards are right that some loss is unrecoverable), but the default
here can be repair rather than teardown.

### 11. Asynchrony where the standards learned it hurts: compile and readback

WebGPU added `createRenderPipelineAsync` for one reason: driver shader
compilation takes tens to hundreds of milliseconds and blocking on it drops
frames. solidrt compiles through a blocking RPC that stalls both the JS
thread and the raster thread, i.e. the frame loop. Invisible today because
compiles happen at startup; it becomes real for the shader-editor /
live-coding workload `createShaderMemo`'s `onError` was built for - exactly
the case where compiles happen while animating.

Readback is the other place both standards refused a synchronous form: WebGPU
has *only* `mapAsync`, and WebGL2 grew the PBO + fence pattern, because
`glReadPixels` drains the whole GPU pipeline before returning. solidrt's
`readTexture` is that stall, deliberately: it is documented as the bake path,
not a rendering path, and for one-shot bakes a stall is fine. Worth noting
that the async shape already exists in the API - `captureSnapshot` returns a
Promise - so if a live readback consumer ever appears (pixel picking, GPU
histograms), the precedent is set and the sync form does not need to grow
into it. Not urgent on either half; recorded because these are the two calls
whose cost class is different from everything else on the surface.

### 12. Colour space and the alpha contract

WebGPU has explicit `-srgb` formats, `GPUCanvasConfiguration.colorSpace` and
`alphaMode: "premultiplied" | "opaque"`. WebGL has `EXT_sRGB` and
`drawingBufferColorSpace`. Both force the app to state what its pixels mean.

solidrt: RGBA8 UNORM everywhere with no colour-space concept, so linear
filtering and additive blending both operate on non-linear values (subtly
wrong, in the usual way that is invisible until it isn't). And a target's
alpha contract is stated nowhere - the particles example writes premultiplied
additive output, implying premultiplied is the rule, but nothing says so and
Impeller composites the result.

Minimum action is documentation, not code: state that targets are
non-linear RGBA8 with premultiplied alpha, so shaders written against the
contract stay correct if a format vocabulary arrives later.

### 13. Index buffers, when they land: reuse `createBuffer`

WebGPU distinguishes usage with flags (`INDEX`, `VERTEX`, ...) on one buffer
type; WebGL distinguishes by binding point (`ELEMENT_ARRAY_BUFFER`). Neither
has a separate `createIndexBuffer`.

When [index buffers](../done/gpu-pipeline-extensions.md) arrive, follow
that: one buffer kind, and the target names `indexBuffer` +
`indexFormat: "uint16" | "uint32"`. It is the smaller API and it keeps
`writeBuffer`/`destroyBuffer` single-purpose.

Related, same file: WebGPU's vertex formats include `unorm8x4` and `snorm16x2`
because vertex bandwidth matters - a colour costs 4 bytes there against 16 in
solidrt's float32-only layout. Also deferred there: multiple vertex buffers,
per-attribute `offset`, and `stepMode: "instance"`. Tight float packing is a
fine starting point; the normalized formats are what large meshes want.

### 14. Coordinate conventions: declare the one you have

WebGPU's least glamorous, most valuable cleanup over WebGL was standardizing
every coordinate convention - texture coordinates top-left origin, framebuffer
origin top-left, and no `UNPACK_FLIP_Y` (the WebGL pixel-store flag that
exists because GL's bottom-left origin disagrees with every image format on
earth). A large fraction of all WebGL questions ever asked are Y-flip
questions; WebGPU deleted the category by fiat.

solidrt is halfway to the same cleanup. The fragment path absorbed it
completely: `vUV` is 0..1 with top-left origin - WebGPU's convention exactly -
and no fragment author ever thinks about it. The pipeline path leaks it: the
target's memory row 0 is clip y = -1 and Impeller samples row 0 as the top,
so the effective clip space is **y-down**, and every vertex author must negate
y in `gl_Position` or render upside down. Today that contract lives in one
example comment (`gpu-pipeline.tsx`, "Clip y is negated..."); `flip_for_fbo`
in raster.rs and the never-Y-reversed-resolve-blit trap are the runtime's own
collisions with the same seam.

The runtime cannot absorb this one - `gl_Position` belongs to the app's
shader. What it can do is what Vulkan did with its identical y-down clip
space: declare it. "Clip-space y points down; row 0 is the top" belongs in the
`createPipeline`/preamble documentation as a named contract, next to the vUV
sentence that already states its half. Documentation, not code, and the
cheapest item in this list after branded ids.

### 15. Mipmaps exist in both standards for a reason

Minification without mipmaps is aliasing, not a style - the 07-15 assessment
already recorded the consequence from the Doom port ("distant surfaces
alias"). WebGL2 solves it with one call (`generateMipmap`); WebGPU
deliberately shipped *without* automatic generation (apps build mip chains
with render passes) and it is one of that spec's most-complained-about
austerities. solidrt's "No mipmaps exist" is a documented axiom, and the new
sampler state applies one filter to both min and mag.

Two things make this cheaper here than in either standard. Sitting on GL,
alloy gets `glGenerateMipmap` for free - take WebGL's convenience, not
WebGPU's austerity. And the retained model gives something neither standard
has: the dirty flush knows exactly when a target re-rendered, so mip
regeneration for render targets can be automatic (regen after render, before
consumers sample), where both standards make the app schedule it. The design
slot is the sampler section that just landed: `mipmap?: boolean` at creation,
min filter goes trilinear when set. Demand-gated - pixel-art wants none, 3D
minification wants them - but the shape is ready.

### 16. Depth you can sample

A pipeline's depth is a private `DEPTH_COMPONENT24` renderbuffer:
unsampleable by construction. Both standards make depth a texture - WebGPU
depth attachments are ordinary `GPUTexture`s that bind as sampled textures,
and ES 3.0 has depth textures plus `sampler2DShadow` comparison sampling in
core. Sampleable depth is the entry ticket to shadow maps, depth-of-field,
SSAO and soft particles; the 07-15 assessment already called out shadow maps
as "only via painful depth-encoding color passes".

Nothing on the demand list asks yet, so defer - but record the shape: the
storage swap (renderbuffer to texture) is small, and the interesting question
is currency, because a target's id names its *colour*. Its depth would need a
name of its own to appear in another target's `textures`, at which point the
dependency graph tracks the edge like any other. The pipeline desc does not
change.

### 17. Compressed textures (ES 3.0 mandates ETC2)

OpenGL ES 3.0 requires ETC2/EAC in core - every GPU at alloy's minimum spec
decompresses it in the sampler for free, cutting texture memory 4-8x against
RGBA8. Both web standards expose compressed formats but gate them (WebGL2
behind an extension, WebGPU behind a feature) for one reason that applies
here too: desktop-emulation backends - ANGLE over D3D, solidrt's own Windows
path - lack the hardware format and must transcode or decline.
`uploadTexture` is RGBA8-only today, so a game-scale texture set pays the
full multiple.

Demand-gated, no field report asks. Filed with the honest platform note
attached: guaranteed native on the GL targets (Linux, Android), possibly
software-expanded under ANGLE on Windows - the exact split that made the web
standards gate it.

## What not to take

Naming these matters as much as the lessons - the standards carry a lot of
complexity that exists for reasons solidrt does not have.

- **Bind groups, bind group layouts, pipeline layouts.** WebGPU's binding
  model exists to validate untrusted content cheaply across a process boundary
  and to let drivers pre-bake descriptor sets. For one trusted app with a
  handful of uniforms, name-keyed params are the right size. Fix the
  *validation* (lesson 2), not the model.
- **An app-visible command encoder and queue.** The retained model is better
  for the common case. Take one imperative verb, not the encoder.
- **Async error scopes.** solidrt's synchronous throw is strictly better here;
  do not adopt a deferred error channel to "match the standard".
- **Adapters and devices.** One device, always. `Flux.capabilities` is the
  right shape for the feature question.
- **Explicit resource state, barriers, staging buffers, `mapAsync`.** GL hides
  all of it, and alloy's single-context single-thread contract makes the
  ordering questions moot.
- **WebGL's global state machine.** Already avoided, deliberately, and the
  exhaustive save/restore in `run_pass` is the price of coexisting with
  Impeller on one context. Nothing about that should leak into the app API.

## Naming

The project's lens is "keep the standard names, simplify the semantics". Three
places where the name was kept but the semantics are different *in kind*
rather than simplified, which is the failure mode that lens has:

- **`createPipeline` and `createRenderPipeline` are one word apart and are not
  the same kind of thing.** `createRenderPipeline` is WebGPU's pipeline
  exactly: inert state, draws nothing, returns a pipeline id.
  `createPipeline` compiles two stages, builds an anonymous pipeline,
  allocates a target, renders it, and returns a *texture* id. Before the
  2026-07-30 split there was only the fused call and the criticism was
  "misleading borrow from WebGPU"; the split fixed the object model and made
  the naming worse, because now both names exist side by side and the shorter
  one is the one that is not a pipeline. This is the most confusable pair in
  the surface.
- **`createShader` returns a texture id.** Same shape of problem. In both
  standards `createShader` creates a shader object - and `compileShader` is
  now the call here that does exactly that. Anyone arriving from either
  standard reads the pair backwards.

  The fused conveniences are worth keeping; they are the reason a Shadertoy
  port is three lines. It is the names that are wrong, and the honest ones say
  what comes back: `createShaderTexture` / `createPipelineTexture` - which is
  already what the engine calls them internally
  (`Context::create_shader_texture`, `create_pipeline_texture`). Breaking, so
  file it rather than doing it in passing, but the internal vocabulary having
  landed on the right name independently is a strong signal.
- **`iResolution` is auto-filled, `iTime` is not.** The preamble declares both,
  the docs list both as things the body "may reference", and `run_pass` fills
  only `iResolution` - `iTime` is an ordinary param the app must drive, so a
  shader that reads it and is never given it silently animates at t=0. Either
  fill it from the frame clock (the runtime has one) or stop declaring it in
  the preamble. Neither standard has built-in uniforms at all, which is why
  neither has this trap.

## Shader language: GLSL, not WGSL

Worth stating plainly because it is the most visible difference from WebGPU
and it is the one thing on this page that is settled: **GLSL ES 3.00 is a bet,
not an oversight.** It is downstream of retiring wgpu for glow - GLSL is what
actually runs on every shipped target (GL on Linux and Android, ANGLE on
Windows), and WGSL would mean either a translation layer or a second backend
for no capability gained.

The cost is real but small and currently favourable: solidrt is on the older
language and inherits the Shadertoy / GLSL-sandbox / Book-of-Shaders corpus
rather than the WebGPU one. Given that the examples are literally Shadertoy
ports, and that the complete-source dialect (`#version 300 es` passes through
untouched, `iResolution` fills as vec3) exists specifically to run that corpus
unmodified, that trade is being collected on, not merely tolerated. Nothing
here suggests revisiting it.

## Capability gaps by workload

The maturity assessment's framing, statuses refreshed 2026-07-30. Games in
the Doom/PS1/stylized-retro class are feasible now, and have been since
07-15; the field-report demand signals since (shadertoy, second-reality,
organism) were all served by items that have landed. The next tiers each
name their blockers:

- **Modern-style 3D** (translucency, large meshes, post-processing,
  shadows): alpha translucency (sorted geometry plus the premultiplied
  question), index buffers, float texture formats (Doom fixed-point-encodes
  16-bit heights across RGBA8 channels today), cull/depth-func, and
  multi-pass targets - all in
  [gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md), now
  unblocked by the object-model split. Mipmaps (lesson 15) and sampleable
  depth (lesson 16) join that list from the standards comparison. MSAA on
  pipeline targets is its own item
  ([gpu-target-antialiasing](../backlog/gpu-target-antialiasing.md)):
  single-sample targets make filled-triangle silhouettes the dominant
  artifact.
- **First-person anything**: still blocked on relative mouse input before
  any GPU gap - no pointer-lock / relative-motion API exists anywhere in the
  surface (re-verified 2026-07-30), and it is an SDL capability away.
  Unchanged since 07-15 and still arguably the biggest gap in the stack for
  this class. Filed 2026-07-31 as
  [relative-mouse-input](../backlog/relative-mouse-input.md).
- **GPU simulation** (particles, fluids, flocking): no path today except CPU
  round trips or encoding state into textures. The ES 3.0 answer, transform
  feedback, is non-pure by construction and lands on the purity fault line -
  see the structural divergence section. Blocked on that decision, not on
  any feature.
- **Modern techniques generally**: no instancing (lesson 6), compute, MRT,
  stencil access, or cube maps. Compute is out of reach at ES 3.0
  regardless; the rest are demand-gated.
- **Platform reach equals the GL backend's reach**: Vulkan and Metal are
  still `unimplemented!()` stubs in `alloy/src/backend.rs` (re-verified
  2026-07-30).
- **CPU ceiling**: QuickJS has no JIT, so per-pixel and per-vertex work must
  stay on the GPU. The Doom port respects this (jsMs ~0.3); CPU-heavy genres
  (physics, large AI populations) would feel it.
- **No 3D scaffolding above the pipeline**: no math library, model loading,
  culling, or scene graph. Defensible scope for a UI framework exposing GPU
  primitives, but every game rebuilds the layer (Doom's mesh.ts is 551
  lines of geometry code).

## File sizes and whether to split

Measured 2026-07-30:

| File | Lines | Concerns held |
| --- | ---: | --- |
| `alloy/src/shader.rs` | 1466 | 6 |
| `alloy/src/raster.rs` | 1425 | 4 |
| `alloy/src/gl.rs` | 1143 | (window/MSAA path, not reviewed here) |
| `alloy/src/context.rs` | 901 | 3 |
| `flux/src/plugins/gui/texture.rs` | 730 | 5 |
| `packages/core/src/gpu.ts` | 393 | 1 |
| `packages/flux-types/gui/gpu.d.ts` | 351 | 1 |
| `alloy/src/texture.rs` | 260 | 4 (small) |

Judgement: **two of these are worth splitting, two are not, and one is a
rename.** The test applied is whether the deferred features would each land in
one file or smear across several - not line count on its own.

**Split `alloy/src/shader.rs`** (yes). It holds six unrelated things: the
draw-state vocabulary and its parsers (`AttrFormat`, `BlendMode`, `Topology`,
`DepthState`, `PipelineDesc`, `ShaderStage`, ~225 lines); stage compile/link
helpers; `GpuBuffer`; `ShaderProgram` + `RenderPipeline`; `ShaderTexture`
(~610 lines, the largest single unit); and `run_pass` plus the window/FBO
entry points (~260 lines). `GpuBuffer` in particular is plainly misfiled - a
vertex buffer is not a shader, and it is only there because that is where
`createPipeline` grew. A folder mirroring `rendertree/`:

    alloy/src/gpu/mod.rs      re-exports
    alloy/src/gpu/vocab.rs    AttrFormat, Topology, BlendMode, DepthState, PipelineDesc, ShaderStage
    alloy/src/gpu/program.rs  compile_stage, link, ShaderProgram, RenderPipeline
    alloy/src/gpu/buffer.rs   GpuBuffer
    alloy/src/gpu/target.rs   ShaderTexture, create_target, create_layer_target
    alloy/src/gpu/pass.rs     run_pass, render_program_to_window, render_program_to_fbo

The payoff is specific, not cosmetic: `vocab.rs` is where every deferred
feature adds a word (cull, depth-func, index format, vertex format,
colour format), `pass.rs` is the file whose exhaustive GL save/restore needs
review on every change, and `target.rs` is where a draw list or a load-op
would land. Today all three are the same file and any of those edits touches
it. Moving `SamplerState`/`SamplerCache` out of `texture.rs` into
`gpu/sampler.rs` is optional; it fits, but `texture.rs` is not big enough to
force it.

**Rename `flux/src/plugins/gui/texture.rs` -> `gpu.rs`** (yes, and cheap). It
registers the `flux:gpu` module and covers textures, the raw shading layer,
pipelines, buffers, capture and readback - five id spaces under a filename
that names one of them. The module and its file should agree. Splitting
further is not warranted: it is a marshalling layer, it is thin per function,
and it is the one place where seeing the whole JS surface at once is useful.

**Do not split `alloy/src/raster.rs`** wholesale (mostly no). Every method is
an inherent `impl RasterState` reaching into shared fields (`dirty`,
`textures`, `shaders`, `pipelines`, `stats`); scattering that across files
buys directory structure and costs the ability to see the state machine. Two
narrow moves are worth it and stop there: lift the ~250-line `RasterCmd` enum
into `raster/cmd.rs` (it is a protocol definition, read far more often than
edited, and it is what a new GPU feature extends first), and move the capture
and readback path - `rasterize`, `rasterize_into`, `flip_for_fbo` - into
`raster/capture.rs`, since it is genuinely independent of the frame loop.
Leave `frame`/`present`/`flush_dirty`/the create-destroy handlers together.

**`alloy/src/context.rs`: lift the DTOs only** (partly). `GpuResources` and
its seven `Gpu*Info` structs are ~100 lines of pure introspection data with no
behaviour, serialized straight out to the MCP tools; `TargetSpec`/
`PipelineSpec`/`WindowShader` are the command-channel shapes. Moving those two
groups to `gpu_resources.rs` and `gpu_spec.rs` leaves `context.rs` as what it
claims to be - the UI thread's handle onto the raster thread - and it is a
move, not a redesign.

**Leave `gpu.ts` and `gpu.d.ts` alone** (no). Both are near-entirely doc
comments over thin wrappers, and the contract reads best in one place. Their
real problem is not size, it is that the same prose is now restated across
`gui/gpu.d.ts`, core `gpu.ts`, core `types.d.ts`, `docs/core.md`, the examples
README and scaffold `AGENTS.md` - already logged as item 6 of
[gpu-pipeline-object-model](../done/gpu-pipeline-object-model.md), and a
generation or parity-check problem rather than a splitting one.

## Shortlist

**All eight closed as of 2026-07-31** (see the status section near the top
for what each one shipped as). Kept as written, because the ordering argument
is the part worth re-reading before proposing the next batch.

If only a few things are done, in this order:

1. **Branded ids** - a `.d.ts` edit, no engine change, closes a whole class of
   cross-id-space slips today. Nothing else on this list is that cheap.
2. **Write the pixel contract down** - clip-space y is down, targets are
   non-linear RGBA8 with premultiplied alpha. Docs only, and it converts two
   silent per-app discoveries into stated contracts.
3. **Decide the purity question** (stay pure, or add one imperative
   `renderTarget` verb). Everything about accumulation, feedback and
   multi-pass is downstream of it, and it should be settled before load-op is
   built rather than discovered after.
4. **Buffers held by `Rc`** like programs - deletes the one ordered-destroy
   rule and its doc sentence together.
5. **Call-site validation: uniforms and draw bounds** - closes the inherited
   WebGL silent-typo class and an actual undefined-behaviour hole
   (`vertexCount`/`setDrawCount` past the buffer end), both from state the UI
   thread already mirrors.
6. **`first` on the draw, then instancing** - additive, safe under either
   answer to (3).
7. **Labels and limits** - cheap, and both make the MCP tooling better.
8. **Split `shader.rs`; rename flux `texture.rs` -> `gpu.rs`.**

Filed separately because they are breaking and want their own decision: the
`createPipeline` / `createRenderPipeline` collision and `createShader`
returning a texture (see naming).

Everything above was filed into the backlog on 2026-07-30 and closed by
07-31, so this list is a reading order rather than the tracker: (1)
[gpu-branded-ids](../done/gpu-branded-ids.md), (2)
[gpu-pixel-contract-docs](../done/gpu-pixel-contract-docs.md), (3)
[gpu-purity-decision](../done/gpu-purity-decision.md), (4)
[gpu-buffer-lifetime](../done/gpu-buffer-lifetime.md), (5)
[gpu-callsite-validation](../done/gpu-callsite-validation.md), (6) the
draw-range/instancing bullet in
[gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md), (7)
[gpu-labels-limits](../done/gpu-labels-limits.md), (8)
[gpu-file-reorg](../done/gpu-file-reorg.md); the naming decisions live in
[gpu-fused-create-refactor](../backlog/gpu-fused-create-refactor.md), and
mipmaps, compressed textures and sampleable depth in
[gpu-mipmaps](../backlog/gpu-mipmaps.md),
[gpu-compressed-textures](../backlog/gpu-compressed-textures.md) and the
extensions file respectively.

Two workload notes sit outside the ranked list but shape the do-order. For
the first-person class, relative mouse input outranks every GPU item and is
an SDL capability away ([relative-mouse-input](
../backlog/relative-mouse-input.md), filed 2026-07-31). And GPU simulation waits on the purity decision
(item 3), not on any feature - transform feedback, instanced particles and
ping-pong state all land on that fault line.
