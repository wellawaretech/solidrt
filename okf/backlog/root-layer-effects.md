---
type: backlog-item
title: Root layer - render the app into a texture effects can read
description: Invert the frame so the app draws into the offscreen MSAA rig and resolves into a sampleable layer texture composited to a single-sample window, giving whole-app effects (warp, glass, transitions) for about the cost of one quad.
status: partial
timestamp: 2026-07-27T00:00:00Z
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
packages/core/examples/window-shader.tsx: warp upright and correct,
identity pass indistinguishable from no shader, resize-while-active clean.
The Android run also covers stage 1's inverted frame path on a tiler; still
open are Windows/ANGLE, playback byte-identity, and minimize/restore plus
background/resume walks. Stages 3-4 (hold + uPrevious, clean-tree raster
skip) are not started.

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
