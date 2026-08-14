---
title: Root layer - render the app into a texture effects can read
description: Invert the frame so the app draws into the offscreen MSAA rig and resolves into a sampleable layer texture composited to a single-sample window, giving whole-app effects (warp, glass, transitions) for about the cost of one quad.
created: 2026-07-27
completed: 2026-07-27
---

# Root layer - render the app into a texture effects can read

Promoted to okf/plans/root-layer-effects.md (2026-07-27). The analysis below
stands. The plan settles the open questions it raises: always-inverted, with
the rig's resolve target as the only branch, so the layer is free until
something asks for it; and the effect is a property of the window owning
neither an id nor a texture, rather than an in-tree element or an app-managed
shader. It also records that the blocking-rpc gap does not apply to a
submit-path design.

Decisions settled in the plan, recorded here while the plan file is not yet
committed: the effect takes a compiled handle (`compileShader` splits out of
`createShader`, `createShaderTarget` is the other half, `createShader` stays
as the fused convenience), never a source string primed through a hidden
cache; compiling through impellerc is out of scope (whole-dev-chain
consequences); subtree effects are a separate item
(okf/backlog/subtree-effects.md), as are Impeller's built-in backdrop
filters (okf/backlog/impeller-backdrop-filters.md); holding the layer and a
`uPrevious` previous-frame layer are in-plan stages, not deferred.

## Status: stages 1 and 2 implemented 2026-07-27

Stage 1 (frame inversion) verified on desktop Linux the same day
(orientation, 60fps, launcher AA through the rig resolve, resize to
full-screen); still open are Android and Windows/ANGLE runs, playback
byte-identity, and minimize/restore plus background/resume walks. MCP
get_snapshot renders offscreen and cannot see the present path, so those
runs need real screenshots. What landed: the window is created
single-sample, every frame rasterizes into the shared `gl::OffscreenRig`
and resolves into FBO 0 via `gl::render_display_list_to_window`;
`window_surface` / `ensure_window_surface` and the `disable_msaa`
window-creation retry are deleted. One deviation from the draft, chosen
deliberately: the frame path draws the display list unflipped and the
resolve blit is a straight 1:1 copy - Impeller treats every wrapped FBO as
a bottom-up window target, and a Y-reversed blit from a multisampled source
is a driver-inconsistent operation some GLES implementations reject, so it
is never issued. Only *sampled* offscreen content is top-left origin;
shader-active frames flip the display list, with the single flip back in
the pass's vertex stage.

Stage 2 landed in two halves the same day: the raw shading layer
(`compileShader(stage, source, { header? })` / `linkProgram` /
`createShaderTarget` / `destroyShader` / `destroyProgram`, with
createShader/createPipeline untouched as fused conveniences - their
refactor is okf/backlog/gpu-fused-create-refactor.md), then the window
pass: a `shader` prop on `<window>` (named shader, not effect - "effect" is
overloaded; `filter` stays reserved for Impeller backdrop filters) taking
`{ program, params?, textures?, vertexCount? }`. The frame resolves into a
runtime-owned, exactly-window-sized layer texture and the program draws
attributeless straight into FBO 0 with `uSource`/`iResolution` filled by
name. Verified 2026-07-27 on desktop Linux AND Android via
packages/core/examples/window-shader.tsx (warp upright and correct,
identity pass indistinguishable from no shader, resize-while-active clean),
plus Windows/ANGLE the same day (layer + pass active, clean logs). A
stale-declaration leak (app switch left the launcher warping) was fixed the
same day: the per-app GPU cleanup now clears the window shader
unconditionally. The Android run also covers stage 1's inverted frame path
on a tiler; still open are playback byte-identity and minimize/restore plus
background/resume walks. Stage 3 shipped the same day as the
history half only: an explicit `previous` field on the shader prop binds last
frame's resolve as `uPrevious` (verified on desktop Linux: echo upright,
in-run withdrawal frees the layer). The hold flag (`frozen`) was built,
verified - including the finding that a workable cross-fade must pin only
the history layer while uSource stays live - and then dropped as premature:
modal semantics with no consumer; it returns with a designed transition
helper (findings recorded in the plan). Stage 4 (clean-tree raster skip) is
not started.

There is no way to run a shader over what is already on screen. An app can
warp, refract or dissolve GPU content it produced itself (bind another
pipeline's target as a `sampler2D` and chain), but nothing hands a shader
the pixels of the widget tree beneath it - no backdrop filter, no
transition that distorts the screen it is leaving. `captureSnapshot` is not
that mechanism and should not be bent into one: it is a one-shot bake that
rasterizes, reads back to the CPU and re-uploads, per call.

## What already exists

Most of the machinery is in place, which is why this is worth writing down
rather than treating as a rewrite.

- **A snapshot repaint boundary is already a layer.** `snapshot_node`
  (alloy/src/rendertree/composite.rs) records the subtree into its own
  display list, rasterizes it into a texture, and composites it as one
  `draw_texture_rect` quad. The boundary's transform and group opacity are
  hoisted out of the raster onto the quad, so the texture itself stays
  pose-free - exactly the property an effect source wants.
- **The texture registry supports replacing at a stable id.**
  `create_texture_at` exists so a stream texture can be resized without
  invalidating the id handed to consumers.
- **Shaders re-resolve their inputs per render.** alloy/src/shader.rs: a
  sampler binding is "resolved to a live GL texture at each render by the
  owner, so an input whose contents or registry entry changed is picked up
  automatically". A shader bound to id N follows whatever is at id N with
  no rebinding.
- **The GL name is available where the layer is produced.** `RasterizeDl`
  runs on the raster thread, and the name exists inside
  `render_display_list_to_texture` before adoption into Impeller (the
  function currently returns only the adopted `Texture`, so surfacing the
  name means a signature change there). The raster thread keeps its own
  `textures: HashMap<u64, GpuTexture>` (which is what
  `resolve_sampler_bindings` actually reads), so registering a layer for
  sampling is an insert on that side, not a cross-thread handoff.
- **Ordering is already same-frame, not lagged.** Texture params are
  dirty-marked, not rendered on write: `set_params` stores into
  `pending_params` and returns `Damage::Paint`
  (alloy/src/rendertree/kinds/texture.rs), and the GL pass happens in
  `build()` during the paint walk. A boundary earlier in tree order is
  rasterized (a blocking rpc, so it has completed) before a later sibling's
  shader renders, and both reach the raster thread in order. An effect
  sampling the layer sees this frame's content.

## The inversion

The naive shape - keep everything as it is and add a layer plus an effect
pass on top - is the expensive one: the app resolves into the layer, the
layer is sampled by the effect, the effect's output is drawn as a quad into
a 4x MSAA window surface, and that surface is resolved again at present.
Two MSAA targets, two resolves, and the window's multisampling accomplishes
nothing because a screen-aligned opaque quad has no edges. Roughly
+165 MB/frame of traffic at 1440p, about 10 GB/s at 60fps: negligible on a
discrete GPU, 15-40% of total bandwidth on integrated or mobile silicon
sharing 25-60 GB/s with the CPU.

Inverted, it costs almost nothing:

- create the window surface **single-sample**
- render the app's display list into the shared offscreen MSAA rig
- resolve into the layer texture
- run the effect, output straight to FBO 0 (or one screen-aligned quad if
  no effect is active)

That is one MSAA target and one resolve - the same count as today, since
the driver was already resolving the window's MSAA at swap. The extra cost
over today is the final full-screen sample-and-write, about 15 MB read +
15 MB write at 1440p, under 2 GB/s at 60fps. The sampleable full-window
layer falls out as a side effect instead of costing a pass.

It also deletes machinery: we stop asking SDL for a multisampled window
config, which retires the `disable_msaa` retry path in alloy/src/gl.rs that
exists for Android drivers exposing no multisampled EGL config. We would
own the only MSAA allocation in the process rather than depending on the
driver having one.

## What is missing

- **Registering the layer at a stable id**, so a shader can bind to it. Per
  above this is raster-side bookkeeping; the awkward part is deciding who
  owns the id and what the app names.
- **Input-dirty propagation.** A shader is marked dirty by its own param
  writes; nothing marks it dirty because a texture it samples changed
  contents. Camera textures paper over the same hole imperatively
  (the runtime calls `platform.request_frame()` after `camera::tick`,
lattice/src/runtime.rs). Today an effect only
  re-renders because it happens to write `iTime` every frame. "Texture id N
  changed" should dirty every shader bound to N.
- **Declared ordering.** Correct sequencing currently falls out of tree
  order and is undeclared: put the effect before the layer and it silently
  samples the previous frame.
- **The blocking rpc mid-walk.** `render_display_list_to_texture` waits on
  a reply channel from inside the paint walk. Per-frame at full-window size
  that is a synchronous stall against a raster thread that may still be
  presenting. Stage 2 of angle-cross-context-impeller-textures.md covers
  the general fix and names snapshot boundaries specifically: move boundary
  rasterization raster-side entirely, shipping the boundary DL with the
  frame (the Flutter model).
- **Prerequisite:** snapshot-offscreen-rig-churn.md. Without retained
  storage and a pooled rig, a root layer reallocates a full-window rig on
  every frame anything in the app changes.

## Open questions

- **Mode or always-on?** On a tiler the final composite is a real extra
  render pass, so the default probably stays "render straight to FBO 0"
  and the layered path switches on when something wants the layer. But an
  always-on layer is simpler to reason about and makes partial-damage
  compositing possible (see below). Measure before deciding.
- **Where does the app declare it?** A root-level effect chain (a prop on
  the window) gets the cheap path, defines ordering by construction, and
  sidesteps recursion entirely, because the effect lives outside the tree.
  An in-tree `<texture>` sibling composes better with regional effects (a
  frosted panel over part of the UI) but inherits tree-order ordering and
  the compositing gap in texture-element-compositing.md. These may both be
  wanted, for different scopes.
- **Recursion is the app's problem, and is not always an error.** An effect
  inside the subtree it samples reads the previous frame, which is feedback
  - trails, echoes, accumulation - a legitimate effect. The engine needs to
  not crash and perhaps log once, not to prevent it.

## The argument that cuts the other way

A retained root layer can make things *cheaper* than today. On frames where
only the effect's uniforms change - a ripple running over a static UI - the
app's geometry does not need re-rasterizing at all; only the shader runs.
Today the full display list is drawn every present. For animated-effect-
over-static-content, which is most of what this feature is for, the layered
path is the faster one.

## Implementation record

Merged from the separate plan file that tracked the build.



Plan for okf/backlog/root-layer-effects.md. The goal is generic: hand a
shader the window's contents and let it do anything with them before they
reach the screen. A full-screen warp is the case that prompted it, but
nothing in the design is about warping - the effect is an arbitrary fragment
program over the finished frame, and the plan's job is to make those pixels
available at all, cheaply, and to make the pass that consumes them the last
thing before present.

## Prerequisites: all met

- **snapshot-offscreen-rig-churn**: done 2026-07-27 (all 3 stages). This plan
  is built directly on its stage 2 product, `gl::OffscreenRig` owned by
  `RasterState`: one retained rig, grown monotonically, with MSAA storage,
  resolve blit, invalidate and the msaa-unavailable latch already in it.
  Runtime verification of that plan is still pending and should land first,
  since stage 1 here puts the rig on the critical path of every frame.
- **texture-element-compositing**: done 2026-07-27 (blendMode on texture).
- **gpu-sampler-rebinding** (`setShaderTextures`), **gpu-in-place-resize**
  (`setShaderSize`, `resizeTexture`): done.
- **angle-cross-context-impeller-textures stage 2** is listed in the backlog
  item as a gap ("the blocking rpc mid-walk"), but it is **not** a
  prerequisite for this plan and never becomes one. That gap only exists for
  a root layer built as an in-tree snapshot boundary, where `snapshot_node`
  blocks the paint walk on `RasterizeDl`. The design below produces the layer
  inside `RasterState::frame`, on the raster thread, from the display list
  that was already being shipped there. There is no new RPC, so there is
  nothing to make non-blocking.

## The shape

Today `frame(dl)` wraps FBO 0 in an Impeller surface, draws the display list
into it, and presents; the window is created with a 4x multisampled config
(gl.rs:622) so the driver resolves at swap.

Inverted: the window is created single-sample, the app's display list is
rasterized into the retained rig exactly like a snapshot boundary, and the
rig's resolve blit picks its target:

- no effect: resolve straight into FBO 0. The blit that already happens now
  targets the default framebuffer instead of a texture. Same MSAA allocation
  count, same resolve count, no extra full-screen traffic. This is the
  important property: the inversion is free in the common case, so it can be
  unconditional and there is no mode to reason about.
- effect active: resolve into the retained layer texture, then draw the
  effect as one full-screen pass into FBO 0.

The layer therefore costs nothing until something asks for it, without a
second code path for "layered" versus "direct": the only branch is the
resolve target.

An active effect costs exactly one extra full-screen sample-and-write,
roughly 15 MB read + 15 MB write at 1440p, under 2 GB/s at 60fps. That is
the floor for any effect that reads a pixel other than the one it writes
(warp, blur, refraction), on every architecture: same-pixel effects can
stay in tile memory via framebuffer fetch or subpasses, non-local ones
cannot. For scale, the app's own paint into a 4x multisampled full-window
target already moves several times that per repainted frame.

### Ordering is declared, not inherited from tree order

The backlog item flags undeclared ordering as a gap. Under this design it
cannot be expressed wrongly: the app's rasterization happens in `frame`,
after the entire paint walk, so a shader rendered during the walk (a
`<texture>` element's params write) necessarily samples the *previous*
frame's layer. The window effect is therefore not a tree element and not an
app-owned shader resource. It is a property of the window, drawn by the
raster thread between the layer resolve and the present.

That also disposes of the input-dirty gap for this feature: an active effect
re-renders every frame by construction, so nothing needs to learn that
"texture id N changed". The gap remains real for in-tree effects sampling
another shader's target; it is not on this plan's path.

## Stage 1: invert the frame, no new API

**Implemented 2026-07-27. Verified on desktop Linux the same day**: correct
orientation and steady 60fps (organism, GPU-pipeline content), clean AA on
launcher text and rounded corners (the rig resolve doing the window's MSAA),
resize to full-screen fine. A window-transparency scare turned out to be the
compositor dimming unfocused windows. Still open: Android and Windows/ANGLE
runs, playback byte-identity, minimize/restore and background/resume walks.

Pure refactor of the present path. Output must be pixel-identical to today
and cost the same. Nothing user-visible ships; this stage exists to isolate
the risky parts (MSAA config, Y orientation, resize, Android surface rebind,
playback capture) from any API.

- `gl::configure_opengl` stops requesting a multisampled config.
  `disable_msaa` and its retry in `app::setup` retire with it: the process
  now owns the only MSAA allocation it uses, instead of depending on the
  driver exposing a multisampled EGL config (the Android emulator case that
  path existed for).
- `RasterState::frame` rasterizes into the rig rather than into a wrapped
  FBO 0, via new `gl::render_display_list_to_window` (a sibling of the
  texture-target entry points, not a widening of `draw_offscreen`: the
  FBO-0 target attaches nothing and never checks resolve completeness). It
  draws into the rig's renderbuffer pair at the window's physical size with
  MSAA, then blits the window-sized rect into the default framebuffer.
- **The frame path draws the display list unflipped**, and the blit is a
  straight 1:1 resolve. This deviates from the drafted "flip + Y-swapped
  blit coordinates" on purpose: Impeller treats every wrapped FBO as a
  bottom-up window target (the reason `flip_for_fbo` exists for sampled
  textures), so unflipped content in the rig is already in window
  orientation - and a Y-reversed blit from a multisampled source is a
  driver-inconsistent operation that some GLES implementations reject. The
  orientation rule is therefore: everything *sampled* offscreen is top-left
  origin; the frame path, which nothing samples in this stage, stays in
  window orientation end to end. When stage 2 activates an effect, that
  frame's display list gets flipped so the layer texture is top-left for
  `uSource`, and the single flip to FBO 0 happens in the effect pass's
  vertex stage, never in a blit.
- The msaa-unavailable latch covers the window path with the same storage:
  `ensure_msaa` at `samples == 0` is plain single-sample renderbuffer
  storage per the ES 3.0 spec, so one code path serves both and the
  emulator fallback needs no FBO-0 wrap.
- `window_surface` / `ensure_window_surface` / `last_wrap` are gone: nothing
  wraps FBO 0 any more. `rebind_window_surface` keeps its
  `gl_remake_current` work and drops the re-wrap. The failed-swap
  redraw-and-retry path redraws by re-running the rig rasterization.
- `read_fbo0_pixels` (playback capture) is unaffected and gets simpler
  content to read: a single-sample default framebuffer.

Accepted trade: the rig now grows to the full window on the first frame and
stays there (one full-window MSAA color + depth-stencil allocation, about
133 MB at 1440p at 4x). Today that same memory exists as the window's
multisampled backbuffer, allocated by the driver, so this is a move rather
than an addition, but it is now our allocation and it is shared with
snapshot boundaries.

## Stage 2: the layer and the window effect

**Implemented in full 2026-07-27 and verified the same day on desktop Linux
and Android** (packages/core/examples/window-shader.tsx: warp visibly
correct and upright, the identity pass indistinguishable from no shader,
resize-while-active clean; gpu resources showed the layer at each window's
exact physical size, and a reload reclaimed the old program and swapped to
the new handle without residue). Windows/ANGLE verified later the same day
(layer + pass active at 2560x1417, clean logs). One leak found by walking
away from the example: the declaration is raster-thread state cleared only
by an explicit command, so switching to an app that never sets the prop (the
launcher) kept warping with a program the app cleanup had already destroyed
(held alive by the pass's Rc). Fixed in the per-app-instance GPU cleanup
(flux gui texture.rs Drop): `set_window_shader(None)` unconditionally,
ordered safely because the old engine drops before the next app evaluates.
Verified on Linux and Windows: after an app switch the windowShader entry is
gone and the program reclaimed. Not yet explicitly walked: removing the prop
at runtime within one app, and swapping between two precompiled handles with
a compile-counter check. The program-handle half shipped
first, reshaped along the way (see the raw-layer bullet below): what landed
is the raw GL model - `compileShader(stage, source, { header? })` compiling
single stages from complete GLSL ES, `linkProgram(vs, fs)` producing shared
program handles, `createShaderTarget(program, w, h, opts)` as the target
half, `destroyShader` / `destroyProgram` per id space - with
`createShader`/`createPipeline` untouched as fused conveniences on top. The
layer and the pass itself landed the same day. **The prop is named `shader`, not `effect`** (decided
2026-07-27: "effect" is overloaded - Solid's createEffect - while `shader`
says what it takes, scales to the subtree feature, and leaves `filter` free
for the Impeller backdrop-filter work).

Implementation notes (what is where):

- `context::WindowShader { program, params, textures, vertex_count }` is the
  one descriptor type: rendertree `Window` (prop half), `Context::
  set_window_shader` (mirror-validates the program id, fire-and-forget
  `RasterCmd::SetWindowShader`), and the raster thread all speak it.
- The prop write follows the texture-params pattern: `Window::set_shader`
  records the change and returns Damage::Paint; the pending change flushes in
  `Window::build`, so the command is ordered ahead of the frame that shows it
  and reactive params stay paced to real frames. A redeclaration with the
  same program keeps the layer and just adopts new params (the per-frame
  path); a new program (or None) releases and starts fresh.
- Raster side: `WindowShaderState { spec, program: Rc<ShaderProgram>, layer }`.
  The layer (`shader::create_layer_target`) is an exactly-window-sized RGBA8
  texture + FBO - exact, not 64-aligned, because the shader samples 0..1 and
  padding would leak into the contract. Allocated by the first shaded frame,
  reallocated on resize, freed on clear; never adopted, no id.
- The frame path branch: `draw_to_window` calls `draw_to_window_shaded` when
  declared - flip the DL (`flip_for_fbo`, so the layer reads top-left like
  every sampled texture), `gl::render_display_list_to_layer` (the
  `draw_and_resolve` internal shared with the window path, dst = layer FBO),
  then `shader::render_program_to_window` draws the program attributeless at
  `vertex_count` straight into FBO 0 with `uSource` = the layer and
  `iResolution` = physical size, clearing FBO 0 to opaque black first. Any
  failure falls back to the unshaded path (visible app beats black window).
- `ShaderTexture::render`'s body was extracted into `run_pass` (program +
  target fbo + viewport + PassDraw enum) so the window pass and target
  renders share the one Impeller-state save/neutralize/restore block instead
  of duplicating it.
- `get_gpu_resources` reports `windowShader: { programId, layerWidth,
  layerHeight }` while declared (the verification bullet's counter).
- Example: packages/core/examples/window-shader.tsx (warp with click-to-
  identity toggle). Docs: core.md "Window shader"; WindowShaderProps in
  core types.

```tsx
let vs = compileShader("vertex", FULLSCREEN_VERTEX)
let fs = compileShader("fragment", WARP_FRAG, { header: true })
let warp = linkProgram(vs, fs)        // compiles/links here, errors here

<window shader={{ program: warp, params: { amount: amount() } }}>
  ...
</window>
```

(A one-line shorthand for "fragment program over the standard fullscreen
vertex stage" is deliberately not built yet; the decision lives in
okf/backlog/gpu-fused-create-refactor.md and can land before or with the
effect.)

The descriptor takes a handle rather than a source string on purpose. The
tempting alternative was `effect={{ source: WARP_FRAG }}` plus a
`precompileEffect(WARP_FRAG)` warm-up call priming a source-keyed cache, so
that switching effects mid-transition would not stall on a compile. That is
the sqlx implicit statement-cache pattern this project already rejects once:
action at a distance, and silent when it misses. An explicit handle costs the
app one lifetime and makes the compile a visible event.

- **Layer texture.** Retained, window-sized, owned by the raster thread,
  resized in place when the window resizes. It is never handed to JS: no id,
  no registry entry, no adoption into Impeller, no lifetime for the app to
  manage. It exists only as the source of the effect pass, and it is
  allocated only while an effect is declared.
- **Built-in shader inputs.** The effect's fragment source reads the window
  through `uniform sampler2D uSource`, a named-uniform contract like
  `iResolution` (filled by name at render if declared): there is exactly one
  thing a window effect can sample, so binding it by name is a property of
  the effect pass rather than inference about intent. Varyings (`vUV`) are
  the program's own, carried from its vertex stage as with any raw-linked
  program. Extra inputs (a noise texture, a mask) come from an optional
  `textures` field on the descriptor, bound explicitly by name as everywhere
  else.
- **One pass, straight to FBO 0.** After the rig resolves into the layer, the
  effect program draws a full-screen triangle into the default framebuffer at
  the window's pixel size. No intermediate target and no closing blit, so an
  active effect costs one pass and allocates nothing beyond the layer.
- **The raw shading layer replaced the drafted mid-level split (done
  2026-07-27).** The draft here was `compileShader(fragmentSrc)` /
  `compilePipeline(vs, fs)` returning program handles; during review the
  layering rule ("raw basics first, shorthand on top") reshaped it into GL's
  own model: stages are compiled individually from complete sources (the
  standard header an explicit `{ header: true }` opt-in, never sniffed), and
  programs are linked from stages. Alloy-side, `ShaderProgram` (GL program +
  reflected uniforms + kind) split out of `ShaderTexture`; targets hold
  their program by Rc, so programs and targets can be destroyed in either
  order and the GL program dies with its last user. One program backs many
  targets, precompiling is "compile and link early", and a target creation
  compiles nothing - asserted by the verification bullet below. Every
  raw-linked program carries its own vertex stage (pipeline-kind); the
  fragment-kind fullscreen path currently exists only inside fused
  `createShader`.
- **A pipeline program as the window effect gives mesh warp for nothing**:
  the app's own vertex stage over the layer, which turns a per-pixel
  dependent texture fetch into an interpolated varying. Since raw-linked
  programs all carry their own vertex stage anyway, the effect pass should
  take the program as-is and draw it attributeless at a declared vertex
  count (default 3, the covering triangle), rather than special-casing a
  fragment-only kind.
- **Declaration reaches the raster thread** as a fire-and-forget
  `RasterCmd::SetWindowEffect` on the same ordered channel as frames, so a
  source or params change lands cleanly between two frames. Precedent for a
  window prop that pushes a command rather than storing a plain field:
  `Window::set_fullscreen` (alloy/src/rendertree/kinds/window.rs:35).
- **Params are declarative**, with the same defer-to-the-next-real-repaint
  pacing every other params prop gets (see the TextureProps note in
  packages/core/src/gpu.ts). This lands the root effect on the *preferred*
  path rather than on `setShaderParams`, the imperative exception documented
  for shaders with no element to hold a params prop: here the window element
  is that element.
- **Compile and link errors throw from `compileShader` / `linkProgram`**, at
  a call site the app chose, not from a prop write during an animation.
  Compiles are a blocking RPC today, so the error reaches JS synchronously,
  matching the throw-in-dev validation policy. Swapping the prop to a
  different handle is then free of compilation entirely.
- **Removing the prop** frees the layer and restores the
  resolve-straight-to-FBO-0 path, with no frame in between that shows
  neither. The program outlives it, owned by the app.

A note on what precompiling actually buys, since it is the reason for the
handle: compile plus link is a few ms on desktop but routinely tens of ms on
Adreno and Mali, and compiles block both the JS and raster threads. But
drivers also specialize on first *use*, so a program compiled at startup can
still hitch on its first draw. If a warm-up is wanted later it has to issue a
throwaway draw, not just compile - and even then specialization can key on
the target's format and sample count, so it is a mitigation, not a guarantee.
`KHR_parallel_shader_compile` is the better long-term answer and is its own
small item.

Debugging has both ends covered without exposing anything: MCP
`get_snapshot` renders the tree offscreen, which is exactly the pre-effect
image, and playback capture reads FBO 0, which is the post-effect image.

What this shape gives up, deliberately: an app cannot get the window's pixels
as a texture id, so it cannot assemble its own multi-pass effect. Holding a
frozen copy and reading the previous frame are the other two things a texture
id would have bought, and both are better answered by the runtime that
already owns the layer (stage 3) than by handing out storage the app must
then size, resize and free.

## Stage 3: hold the layer, and the previous frame

"Do anything with the window's contents" includes keeping them. A transition
that dissolves the screen it is leaving needs the outgoing content while the
incoming tree is already painting, and accumulation effects (trails, echoes,
motion blur by feedback) need the previous frame while shading the current
one. Neither is reachable in stage 2, because the layer is overwritten every
frame.

Both fall out of the runtime owning the layer, which is why this belongs in
the plan rather than in a someday list:

- **Hold**: a flag that stops the rig resolving into the layer, so the
  effect keeps sampling the frame it froze while the tree underneath carries
  on changing. Costs nothing beyond not doing work.
- **Previous**: a second retained layer, ping-ponged each frame and exposed
  as `uPrevious` next to `uSource`. Costs one more full-window texture,
  allocated only when the effect declares it wants it.

The open part is not the mechanism but the declaration: hold is a state, not
a value, and an app that forgets to release it silently freezes its own UI.
Whether that is a boolean prop, a duration, or scoped to a transition helper
needs deciding before it is built.

### Status: `previous` shipped 2026-07-27; hold/`frozen` built, verified, and dropped the same day

What shipped is the history half only. `previous: true` (an explicit field on
the shader prop - nothing is sniffed from the program source, consistent with
the header opt-in) retains the last resolved frame as a second
exactly-window-sized layer bound as `uniform sampler2D uPrevious`, rotated
(std::mem::swap with the current layer) before each resolve. Until a second
frame exists it samples opaque black - `create_layer_target` now clears every
new layer to the pass's clear color, so undefined storage never reaches a
program. Declaring the uPrevious uniform without the flag leaves the uniform
at unit 0, aliasing uSource (documented, not detected). Withdrawing the flag
frees the history layer on the next shaded frame; `clear_window_shader` frees
both layers; `get_gpu_resources` reports `previous` (true only once the
history layer is allocated). Example:
packages/core/examples/window-shader-history.tsx (one-frame motion echo,
click toggles the echo term).

Verified on desktop Linux 2026-07-27: the echo trails the orbiting square
upright (no double-flip in the history layer), and in-run withdrawal via a
timered scratch app flipped `get_gpu_resources` windowShader.previous
true -> false (layer freed) with zero warnings. Still open: Android/Windows
runs (both need rebuilt binaries).

The hold flag was built as `frozen`, went through two designs, was verified
end to end, and was then dropped as premature. Findings worth keeping for
when transitions bring it back:

- The naive semantics (freeze = stop the resolve) cannot do cross-fades: the
  incoming tree has nowhere to render, so the shader can see the outgoing
  screen or the incoming one, never both. Fade-out-then-pop is its ceiling.
- The semantics that works: with `previous`, freeze pins only the history
  rotation - uPrevious holds the outgoing frame while uSource keeps
  resolving the live tree, and `mix(uPrevious, uSource, t)` is the classic
  cross-dissolve. This was implemented and user-verified on screen (pinned
  frame held, swapped theme's live content dissolved in, clean release).
- Why it was dropped anyway: that makes one flag mean two different things
  depending on another flag (pin history vs pause source) - modal semantics
  needing a doc paragraph - and nothing pulls on cross-transitions today.
  The right shape is probably a transition helper owning
  freeze/mix/release as one gesture, designed when transition work actually
  happens; the layer mechanics proven here (a skipped swap, a skipped
  resolve) are trivial to reintroduce under whatever declaration that
  design picks.

## Stage 4: skip the app raster when the tree is clean

The payoff the backlog item argues for: a ripple running over a static UI
should not re-rasterize the UI. A frame requested only by an effect params
change reuses the retained layer and runs the effect pass alone, which is
strictly cheaper than today, where every present redraws the full display
list.

Needs a "nothing in the tree changed since the last layer" signal travelling
with the submit, plus invalidation of that signal on resize and on display
scale change. Deliberately last: every earlier stage is correct without it,
and it is the one part that can silently show a stale frame if the signal is
wrong. Note it is the same machinery as stage 3's hold, arrived at from the
other side - hold is the app deciding the layer is still good, this is the
runtime deciding it.

### Status: implemented 2026-07-27, runtime verification pending

The signal turned out to mostly exist already: lattice's renderFrame has a
present-only reuse path (resubmit the cached display list when tree
revision, texture generation, window size, scale and the stats flag are all
unchanged) - the problem was that every `setProperty` bumped the revision,
so an animating shader defeated it every frame. What landed:

- `Damage::Present`, between None and Transform: pixels may change, no tree
  content changed - request a present, invalidate nothing. `Window::
  set_shader` reports it. The revision bump moved from `element_write` into
  `apply_damage` (Transform and up), so None/Present writes stop forcing
  rebuilds (title writes got cheaper as a side effect).
- The reuse path flushes the pending shader declaration itself
  (`RenderTree::take_pending_window_shader`, the walk-free mirror of the
  flush in `Window::build`) and resubmits via `Context::submit_clean`, a
  `tree_clean` flag on the Frame command. Rebuild-path submits stay
  not-clean, so playback/capture (always_render bypasses reuse) is untouched
  by construction.
- Raster side: a `content_dirty` bit, set by every command except Frame and
  SetWindowShader (texture uploads, target renders, buffer writes - anything
  that can change sampled pixels; this keeps camera-into-shader correct,
  since present-only texture updates also ride the reuse path), also set by
  any non-clean Frame including load-shed ones, cleared by a real resolve.
  The shaded draw skips resolve+rotation and runs only the pass when
  `!content_dirty` and the layer matches the window size. The dirty bit
  subsumes the per-frame flag, so `frame()`/`draw_to_window` signatures are
  untouched.
- Deliberate v1 limit: frames with `previous` declared never skip (a skipped
  resolve would freeze uPrevious on stale content; the upgrade is one
  layer-to-history copy on the first skipped frame, if echo-over-static ever
  matters). Stats-overlay-enabled runs still rebuild once per second
  (okf/backlog/stats-overlay-post-shader.md).
- Verification counter: `windowShader.passOnlyFrames` in gpu resources,
  cumulative pass-only (skipped-raster) frames.

To verify on a client: passOnlyFrames climbing (and get_stats `reused`
climbing in step) during a params-only animation; a tree change mid-run
showing instantly; identity shader still pixel-identical; camera-into-window-
shader still live (dirty bit path).

## Deferred

- **Multi-pass effects.** `effect` takes an ordered list of descriptors and
  the runtime ping-pongs between two textures it owns: N passes, 2 textures,
  no app-side lifetime or resize duty. The trigger is simply an effect that
  cannot be written as one fragment program - a separable blur (horizontal
  then vertical) or a downsample pyramid. Everything else should merge into
  one source, since a pass costs the same whether it does one effect or five,
  which is also why the singular prop is the right starting point.
- **The same effect over a subtree instead of the window.** A different
  feature that reuses this machinery rather than a widening of it: it has its
  own semantics, its own prop, and a limit this plan does not have (a subtree
  texture cannot see what is behind it). Split out to
  okf/backlog/subtree-effects.md so this plan stays one thing.
- **Custom GLSL over the backdrop** (warp or refract *what is behind* a
  panel, rather than the panel itself). Not reachable by sampling a subtree
  texture, since those pixels are not in it. Two routes, both real work: an
  effect declared at a *point* in the tree rather than on a subtree, where
  everything painted before it goes through the effect and everything after
  draws on top (this generalizes the window effect, which is the degenerate
  case with the split point at the end); or Impeller's own filters, which is
  the next bullet.
- **Impeller fragment programs are ruled out** as a route for custom
  shaders. `ImageFilter::new_image_filter_from_fragment_program` and
  `ColorSource::new_color_source_from_fragment_program` exist in impellers
  0.4.2 and would give correct backdrop semantics inside the display list,
  but the crate is explicit: the bytes must be compiled by `impellerc`, raw
  GLSL is unsupported, and Impeller does not compile shaders at runtime.
  Adopting that would mean shipping impellerc in the toolchain and turning
  shaders from runtime strings into build artifacts, which lands on the whole
  dev chain (srt build, dev-server hot reload, the `createShader` contract).
  Decided out of scope 2026-07-27. Impeller's *built-in* filters are a
  different matter and do not need impellerc: see
  okf/backlog/impeller-backdrop-filters.md, which is independent of this plan
  and can land on its own.
- **Partial-damage compositing**, which a retained always-on layer makes
  possible. Not attempted here.
- **EXT_multisampled_render_to_texture** (implicit resolve): already noted
  as its own item under the rig plan; it changes the rig shape and would
  change the resolve-target branch in stage 1.

## Verification

- Stage 1 is a regression test, not a feature test: the same app must be
  pixel-identical before and after (MCP `get_snapshot` cannot see this - it
  renders offscreen - so a real desktop screenshot is needed, per the
  angle-cross-context repro notes), at the same fps, on Linux GL, Windows
  ANGLE and Android. Android is the one that matters most: it is the
  platform whose driver may have been giving us a multisampled window we now
  stop asking for, and the tiler where an extra pass would show.
- Resize, minimize/restore, and Android background/resume all exercise the
  removed `window_surface` re-wrap and must be walked explicitly.
- Playback capture (`srt record` / `srt playback`) must still produce
  byte-identical frames.
- Stage 2: an example app with a warp over an ordinary UI. An identity
  effect (`texture(uSource, vUV)`) must be indistinguishable from no effect
  at all, which is the one test that catches an orientation or half-pixel
  error immediately. Compare `get_snapshot` (pre-effect) against a desktop
  screenshot (post-effect) to see the effect in isolation.
- Adding, changing and removing the prop at runtime, plus resizing the
  window while an effect is active, since both move the layer allocation.
  Swapping between two handles compiled at startup must not compile anything,
  which is the whole point of the handle and is worth asserting in a counter
  rather than assuming.
- Stage 3: the history layer must be last frame's resolve exactly (a
  one-frame echo trails motion, upright), and withdrawing `previous` must
  free the layer. (The hold criteria that stood here applied to the dropped
  `frozen` flag; they were met before the drop - see the stage 3 status.)
- Stage 4: `get_stats` must show the app's rasterization skipped across a
  run of effect-only frames, and any tree change during that run must show
  up immediately rather than at the next unrelated repaint.
- `get_gpu_resources` should show exactly one rig-sized MSAA allocation plus
  one layer texture while an effect is declared, no layer at all without one,
  and no per-frame growth either way.

## Open questions

- **Descriptor shape.** One object prop, `effect={{ source, params, textures }}`,
  where absent means no effect; versus separate `effectSource` / `effectParams`
  props. The object keeps a source and its uniforms from drifting apart and
  makes "no effect" a single absent prop. Leaning object.
- **`uSource` as the built-in name**, and whether `iResolution` for an effect
  is the window in physical pixels (what the pass actually covers) or in
  logical points (what the rest of the API speaks). Physical is what a
  texel-accurate effect needs; the mismatch with the rest of the API needs a
  doc line either way.
- **Does anything still need an Impeller surface over FBO 0?** Stage 1
  assumes not. Worth confirming there is no debug or overlay draw path that
  bypasses the display list.
