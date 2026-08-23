---
title: Where a native 3D layer can beat the browser libraries
description: Both halves of the Three.js comparison beyond feature count - where solidrt can end up ahead, and where it loses. Retained draw list makes per-frame cost O(delta) not O(scene), direct GPU access removes the sandbox tax, and one tree for UI and 3D is the capability the browser forbids; against that, camera motion is O(scene) writes today, structural churn is expensive, and the driver surface is ours. The interpreter losses are a routing decision, not a ceiling: JS, wasm, FFI and into-core are four rungs, and the browser ladder stops at rung two.
created: 2026-08-05
---

# Where a native 3D layer can beat the browser libraries

[scene-graph-3d](scene-graph-3d.md) answered "how would we grow something
Three.js-like" and staged the work. This note answers the other half, asked
2026-08-05 the day after `@solidrt/3d` v1 landed: **given a different
internal model and direct GPU access, where can we end up better?**

The framing matters. A feature-count comparison against a library with a
decade of accumulation is a function of age, not of design, and it points
the roadmap at catching up. The useful question is which of the incumbent's
*constraints* we do not share, because those are the places where effort
compounds instead of merely closing distance.

Ranked by how structural the advantage is.

## 1. The cost model is asymptotically different

A browser scene graph re-walks the scene in JS every frame: world matrices
for every object, a frustum test per object, a sort, then per-object uniform
uploads and draw issuance. That is O(scene) JS per frame regardless of what
changed, and it is why the browser libraries need a JIT. Demand-driven
wrappers over them (render-on-invalidate) only skip whole frames; they do
not make a rendered frame cheaper.

Ours is O(delta). The retained draw list lives in Rust; the bind-and-draw
walk runs on srt-raster; JS pays for what moved. 10,000 static meshes with
five animated ones costs five matrix chains and five param writes.

Three consequences worth building toward:

- **A 3D view that is genuinely free when idle** - not "we skipped the
  frame" but zero passes and zero JS, composited into a UI that is also
  demand-driven. A viewer sitting still costs nothing. That is the app class
  (kiosk, TV, embedded, battery) where the browser libraries are simply the
  wrong tool, and it is the same demand-driven win the rest of the runtime
  already banks.
- **Scene size decouples from frame cost.** The ceiling stops being "how
  many objects" and becomes "how many objects move". Large static worlds
  with a few animated actors are the cheap case - the inverse of the browser
  tradeoff, and a good fit for the interpreter ceiling recorded in
  [scene-graph-3d](scene-graph-3d.md).
- **The walk can move into the engine later without an API change.** The
  draw-list design (../backlog/gpu-draw-list.md) was chosen for that. A
  JS-side renderer can never make that move.

## 2. Direct GPU access removes the sandbox tax

Even the modern browser GPU API is a sandbox: mandatory per-call validation,
shader translation through an intermediate compiler, no persistent mapping,
no bindless, indirect draws gated or absent, and uneven mobile coverage. We
talk to the driver, on a GLES 3.0 floor that can probe upward per device.

- **Compute shaders** (GLES 3.1): GPU skinning, particle simulation, GPU
  frustum and occlusion culling. WebGL2 has none at all.
- **Indirect and multi-draw indirect**: GPU-driven rendering, where the GPU
  decides what to draw. The browser libraries emulate the draw-call half of
  this in JS (a batched-mesh class that merges geometry by hand).
- **Real GPU timing**: pass timing already exists here
  (../backlog/gpu-pass-timing.md). Browsers disabled timer queries for
  side-channel reasons, so their users profile blind.
- **Zero-copy texture sources**: camera frames and decoded images already
  arrive as texture ids without a format-converting upload. In a browser
  every video frame is a re-upload or an external-texture handle valid for a
  single frame.
- **Device-appropriate formats chosen at pack time** - ASTC on Android, BC
  on desktop, float targets, depth textures, MRT - instead of the WebGL2
  lowest common denominator (../backlog/gpu-compressed-textures.md,
  ../backlog/gpu-float-texture-formats.md,
  ../backlog/gpu-sampleable-depth.md).
- **No context-loss theater**, no per-tab context limit, no conservative
  texture-unit floor, and the device's real VRAM budget rather than a tab
  quota.
- **Async compile and link off the critical path** plus PBO readback without
  a sync stall (../backlog/gpu-async-compile-readback.md).

## 3. One tree for UI and 3D

This is the capability the browser architecturally forbids, and the one
worth treating as the differentiator.

The scene's output is already an ordinary texture id, so it takes layout,
transforms, blendMode and pointer events like any element. The inverse
direction is the interesting half: **a rendered UI subtree as a live
material map**. Real, interactive UI on curved or animated geometry, sharing
the scene's depth buffer, with hit-testing back through the mesh once
picking lands. Panels in a 3D space, a device model with a working screen, a
curved wall of live content.

The browser answer is a CSS-transformed DOM overlay that only *looks*
co-located and cannot share a depth buffer, or rasterizing UI to a canvas,
which loses interactivity. Neither is fixable from inside the browser.

Most of the machinery exists. What is missing is specific:

- `captureSnapshot(nodeId)` already renders a node to a texture id, but it
  is one-shot and async ([gpu.d.ts:760](../../packages/flux-types/gui/gpu.d.ts)).
- `repaintBoundary="snapshot"` already retains a boundary's rasterized
  pixels as a GPU texture, but that texture is consumed internally by
  subtree effects and is not exposed as a `TextureId`.

So the concrete unlock is **exposing a snapshot boundary's retained texture
as a texture id that updates as the subtree repaints**. That single addition
turns every existing UI component into 3D-mappable content, and it composes
with the damage model already in place: the boundary re-rasterizes only when
its subtree changes, so a static panel on a spinning mesh costs one texture,
not a repaint per frame. Picking (staged in
[scene-graph-3d](scene-graph-3d.md)) is the second half, routing pointer
events through the mesh back into the subtree.

## 4. Model choices already banked

Advantages that fall out of decisions already made, which the incumbents
cannot retrofit without breaking their users:

- **Material is the pipeline, params live on the draw entry.** Per-object
  uniform values while sharing one program, with no material cloning. The
  browser libraries put uniforms on the material object, so per-object
  variation multiplies material instances.
- **Stable DrawIds with explicit ordering.** Transparency sorting becomes
  "reorder when the camera moves", not "re-sort a list every frame"; the
  same applies to state-change-minimizing order, computed at mutation time.
- **Call-site validation that throws** (../backlog/gpu-callsite-validation.md).
  A typo'd uniform name is an error, not a value silently stuck at zero.
- **One shading-language target.** No WebGL1 legacy, no second shading
  language, no material variants across renderers. A shader composition
  system is *easier* here than there, because there is one target and no
  `#ifdef` matrix - which matters for the design question in section 6.

## 5. The asset pipeline can run at build time

Browsers must parse and decompress at runtime. We have `srt` and a packer,
so models can ship pre-interleaved in the exact layout `addDraw` wants,
pre-transcoded per platform, with bounds and LODs precomputed. Runtime
loading becomes a buffer upload. This also resolves the glTF question in
[scene-graph-3d](scene-graph-3d.md) staging step 3: the mature loaders are
deeply coupled to their own object model but not to their renderer, so they
run fine under Bun in the CLI, which gets us their extension coverage
(including compressed-mesh and transcoded-texture paths that need wasm and
workers) with zero runtime weight and no DOM shims.

## 6. Deterministic capture

Record/playback and frame-clock pacing already exist. Pixel-exact 3D
regression tests and frame-exact video export are straightforward here and
impossible in a browser, which will not hand over the frame clock.

## Where we lose

The same two properties that produce the advantages above produce their own
losses, and the note is only useful if both are on the page.

### The retained model and the FFI boundary

- **Sustained camera motion is the one case where O(delta) does not apply**
  today. A camera change rewrites every visible entry's `uMVP` (a known v1
  cost, recorded in `packages/3d/AGENTS.md`), so orbiting costs a matrix
  multiply and an FFI write per mesh per frame. Worth stating precisely,
  because it is easy to overdraw: the browser libraries pay O(scene) per
  frame *unconditionally* - they recompute a model-view and normal matrix
  per object and upload both per draw whether or not anything moved - so
  our worst case is roughly their permanent case. During orbiting we are at
  rough parity (same matrix count, our writes cross FFI, their arithmetic
  has a JIT); everywhere else we are far ahead. This is an implementation
  shortcut, not a property of the retained model: with a per-entry `uModel`
  and a **target-shared** `uViewProj`, camera motion touches no per-object
  state at all and the GPU absorbs one extra mat4 multiply per vertex. That
  is strictly better than the incumbent design, and not something it can
  copy - per-object CPU matrix computation is baked into every one of its
  materials and every user shader that reads a model-view matrix. What it
  needs from us is a shared-param concept for draw targets (`setDrawParams`
  is per-entry only), whose one real design wrinkle is validation: a shared
  name must apply to the pipelines that declare it rather than being
  rejected as unknown. See the implications section.
- **Structural churn is expensive where theirs is free.** Object turnover
  means addDraw/removeDraw across FFI; a JS renderer simply stops walking an
  object. Particle bursts, projectiles and streaming worlds invert the
  advantage - we are cheap at steady state and expensive at churn, they are
  the reverse. Argues for pooling and reuse patterns, not a redesign.
- **Transparency sorting costs FFI on top of the sort.** They re-sort a list
  each frame and that is the whole cost; we sort and then write the order
  across the boundary. Better only because we can skip it when the camera
  has not moved.
- **No synchronous read-after-write.** Mutations batch to a microtask;
  creates, compiles and readbacks are blocking RPCs to the raster thread. An
  in-process library's writes take effect immediately and can be read back
  in the next statement. Better for throughput, worse for reasoning and
  debugging.
- **Absolute per-write overhead is higher.** Below some object count a
  JIT-hosted renderer is faster in wall-clock terms; our win needs a large
  static fraction. The crossover point is unmeasured and should be measured
  before any performance claim is made.

### The interpreter

- **Per-vertex and per-object-per-frame algorithms are not viable in JS**:
  skinning, morph targets, cloth, CPU particles, physics, procedural
  geometry. The browser libraries do all of these in JS routinely.
- **Algorithmic library code runs roughly 10-30x behind a JIT**: BVH
  construction for fast raycasting, mesh simplification, tessellation, CSG,
  navmesh generation, curve and path extrusion, atlasing. Their addon
  ecosystem is largely this kind of code, and porting any of it carries a
  viability question they never have to ask.
- **Runtime-fetched user content stays hard.** The build-time pipeline
  (section 5) solves shipped assets and does nothing for an app whose users
  supply models at runtime.

These three are the group the next section reframes: they are real, but they
are a routing decision rather than a ceiling.

### Being native

- **The driver surface is ours to own** - probably the largest ongoing
  hidden cost. A browser library delegates every driver bug to vendors with
  enormous QA budgets and a compatibility layer built specifically to paper
  over them. We hit them directly, on every device, permanently.
- **No URL.** Their demo is a link; ours is an install. That shapes
  adoption, how bug reports arrive, and how much casual experimentation
  happens - which is where a library's real feature demands come from.
- **Content tooling is built around their conventions.** We inherit glTF but
  not the DCC export presets, optimizer defaults, inspectors or editors. Our
  MCP GPU inspection is a genuine head start on the debugging axis; content
  tooling is a different one.
- **Knowledge availability.** Their API is in every tutorial and every
  model's training data. Users get materially less help building on ours.

### One self-inflicted ceiling

The fixed 8-float vertex layout. Vertex colours, tangents, skin weights and
any per-vertex custom data all need layout work, where an open
`BufferGeometry`-style model takes arbitrary named attributes. It bought
real simplicity for v1 and will need paying back - probably as a small set
of named layouts rather than a fully open model.

## The escape ladder

The interpreter group above is where the comparison usually stops for a
JS library, because for a JS library it is the end of the road. It is not
the end of ours: solidrt is not restricted to JavaScript, and the honest
statement is that costly work is a **routing decision**, not a ceiling.

Four rungs, each with a different cost:

1. **JavaScript.** Structure, policy, app logic - anything O(changes).
   Fastest to write and to change, and the only rung an app author owns
   without a platform release.
2. **flux:wasm.** Not a speed rung: wasmi is an interpreter too, a small
   constant factor at best (the point [scene-graph-3d](scene-graph-3d.md)
   already makes). It is a **portability and adoption** rung - it runs
   proven C/C++/Rust libraries (a mesh simplifier, a decoder, a solver)
   without porting them to JS, sandboxed and shippable per app. A slow
   proven library still beats a slow hand-written one. It also carries a
   free future: swapping wasmi for a JIT/AOT engine on capable platforms
   would speed up everything built on this rung without an API change.
3. **flux:ffi.** Native speed via dlopen, for an app with a specific native
   dependency. Costs per-platform binaries and packaging (Android needs them
   shipped as `lib*.so`), and gives up the sandbox.
4. **Into core.** The rung nothing in the browser has. Work that is
   *platform* work rather than *app* work moves into Rust and stops being
   anybody's per-app problem: the scene walk, culling, BVH construction and
   query, glTF parsing, skinning. The draw-list design was deliberately
   chosen so the scene walk in particular can make this move **without an
   app-facing API change**.

The asymmetry worth naming: the browser ladder tops out at rung 2 - wasm in
a sandbox, with JS glue and a copy at every boundary. Ours tops out at
native code in the same process as the GPU context, with no sandbox and no
marshalling. A BVH built in core lives beside the rendertree and answers
queries without crossing anything.

The counter-pressure, so this is not read as a free pass: every rung down
costs iteration speed and cross-platform build burden, and moves code from
"an app author can change it" to "the platform team must ship it". Descend
deliberately. A workable rule of thumb:

- **Stays in JS** if it is O(changes) and app-specific.
- **Goes to core** if it is O(vertices) or O(scene)-per-frame, or if every
  app would otherwise write it.
- **Goes to wasm** if a proven library already exists and per-app shipping
  matters more than peak speed.
- **Goes to FFI** if one app needs one heavy native dependency on a known
  set of platforms.

Applied to the losses above: the interpreter group is mostly a core
question, and even the camera-motion regression has two answers on the
ladder - shared uniforms (a small GPU-layer addition, rung 1 stays) or
moving the scene walk itself into core (rung 4). The native group and the
no-URL problem are genuinely inherent to the bet and no rung fixes them.

## Implications for staging

Nothing here changes the [scene-graph-3d](scene-graph-3d.md) staging, but it
reorders emphasis:

1. **Shared (target-level) uniforms, and `uModel` + `uViewProj` in place of
   a premultiplied `uMVP`.** Turns camera motion from O(scene) writes into
   one, which is the last case where the O(delta) advantage does not apply,
   and it generalises: with world-space lighting the normal matrix derives
   from `uModel` alone and stays off the camera path too. It changes the
   `shaderMaterial` uniform contract, so it is far cheaper now than after
   apps depend on `uMVP`. Filed as ../backlog/gpu-shared-draw-params.md.
2. **Expose the snapshot boundary texture as a texture id.** Small, and it
   is the load-bearing piece of the section-3 differentiator. Filed as
   ../done/snapshot-boundary-texture-id.md.
3. **Picking**, which turns section 3 from a rendering trick into an
   interaction model, and needs only bounding volumes plus the localX/localY
   the texture element already delivers.
4. **Finish the standard uniform set** that item 1 starts - normal matrix
   and camera position alongside `uModel`/`uViewProj` - *before* lit
   materials exist. Together they decide whether a custom material can do
   lighting at all, and doing it first is what stops custom shaders becoming
   second-class the way they are in the browser libraries, where reaching
   for a custom material means giving up the lighting system and the
   workaround is string-patching built-in shader source. The policy that
   avoids that fork: compose our own lit materials from exported GLSL string
   constants an app can import and recombine, so there is no inside and
   outside to escape between. Plain JS template composition, no preprocessor
   and no include resolver - affordable precisely because of section 4's
   single-target advantage. The cost to accept knowingly is that those
   exported fragments become public API and pin the varying and uniform
   naming.

Cross-references: [scene-graph-3d](scene-graph-3d.md) (the design and
staging this note extends), [declarative-gpu](declarative-gpu.md) (the
layering rules both uphold), ../notes/gpu-review.md (the capability
audit), ../backlog/gpu-draw-list.md (the retained list section 1 rests on).
