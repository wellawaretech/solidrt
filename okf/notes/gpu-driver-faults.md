---
title: The GPU driver faults register
description: Known OpenGL ES and EGL driver faults on any GPU that runs OpenGL ES 3.0, sorted by the GL path they touch, the paths we use and the ones we may use; our own finds with their code sites, then the faults Chromium, ANGLE and Skia work around. Read before GPU work and when a device misbehaves.
created: 2026-11-20
---

# The GPU driver faults register

The register behind
[gpu-driver-faults](../backlog/gpu-driver-faults.md). The target is any GPU
with OpenGL ES 3.0, the context ore asks for (native on Android and Linux,
through ANGLE on Windows and macOS). The devices we test on are where we
can see a fault, not where we support one.

## How to use it

- Before a step that touches GPU state (a new pass, a new framebuffer
  pattern, a new extension, a new GLSL feature), find the step's paths
  below and read their entries. Name the entries that apply in the step
  brief, with the device check that would show each.
- When a device misbehaves, look up its GPU and the path in question here
  before you debug from scratch.
- A workaround in code names its entry here (a line of its comment); an
  entry with a workaround names its code site.
- A new find goes in "Our finds" with the GPU, the driver version, the
  commit and the code site.

The upstream lists sort by GPU; this register sorts by path, so a path we
do not use yet already has its faults listed when we start to use it. A
GPU condition upstream is a lower bound: a fault seen on one driver
version often lives on in the versions nobody retested.

Sources, fetched 2026-11-20 (refs/heads/main):

- Chromium `gpu/config/gpu_driver_bug_list.json` (most of its GL entries
  moved into ANGLE; what stays is mostly video and D3D, dropped here).
- ANGLE `include/platform/gl_features.json` for the descriptions and
  `src/libANGLE/renderer/gl/renderergl_utils.cpp` (`InitializeFeatures`)
  for the GPU conditions: the faults ANGLE's GL backend works around in
  the native driver under it, which are the faults our native contexts
  meet with no ANGLE in between.
- Skia `src/gpu/ganesh/gl/GrGLCaps.cpp`
  (`applyDriverCorrectnessWorkarounds` and the vendor checks elsewhere).

To refresh: `curl "https://chromium.googlesource.com/<repo>/+/refs/heads/main/<path>?format=TEXT" | base64 -d`.

Entry shape: `source | path | symptom | GPUs | upstream's answer`.

## The paths

What we use, and where. "Not used yet" names what a step may add.

- **context**: GLES 3.0 context through SDL and EGL (`ore/src/window/open.rs`,
  `alloy/src/gl/context.rs`); robustness and reset status
  (`alloy/src/gl/robustness.rs`, `EXT/KHR_robustness`,
  `EGL_EXT_create_context_robustness`); `GL_CHROMIUM_lose_context`;
  `EGL_KHR_surfaceless_context` and EGL device enumeration for headless
  runs. Not used yet: shared contexts, ES 3.1/3.2 contexts.
- **present**: swap and swap interval; `EGL_EXT_buffer_age` and
  `EGL_KHR_partial_update` damage regions (`alloy/src/raster/buffer_age.rs`,
  `alloy/src/raster/repaint.rs`); `EGL_ANDROID_get_frame_timestamps`;
  a multisampled window surface on Android, single-sample elsewhere.
  Not used yet: `eglSwapBuffersWithDamage`, post sub buffer.
- **fbo**: framebuffer switches within a frame (anvil layers, alloy
  targets), texture and renderbuffer attachments, completeness checks,
  `glInvalidateFramebuffer` (`anvil/src/renderer/layers.rs`,
  `alloy/src/gl/target.rs`, `alloy/src/gl/draw.rs`), clears. Not used yet:
  multiple render targets (`glDrawBuffers`), `glClearBuffer*`, layered
  attachments (`glFramebufferTextureLayer`).
- **msaa**: `glRenderbufferStorageMultisample` (`alloy/src/gl/rig.rs`,
  `alloy/src/gl/storage.rs`, anvil layers),
  `EXT_multisampled_render_to_texture` (in-tile, anvil layers and the
  rig), resolves by blit and by a sampling draw. Not used yet:
  multisampled textures (ES 3.1), sample shading.
- **blit-read**: `glBlitFramebuffer` (resolves, layer copies, readbacks),
  `glReadPixels` into pixel-pack buffers (`alloy/src/gl/tap.rs`,
  `alloy/src/gl/readback.rs`). Not used yet: `glCopyTex(Sub)Image2D`.
- **texture**: `glTexImage2D`, `glTexStorage2D/3D`, sub-image uploads
  through pixel-unpack buffers (`alloy/src/gl/staging.rs`), 2D arrays, 3D,
  cube, depth (`DEPTH_COMPONENT24`, `DEPTH24_STENCIL8`), `RGBA16F` and
  `RGBA32F` (with `EXT_color_buffer_(half_)float`), `SRGB8_ALPHA8`, `R8`,
  `RG8`, ETC2 and BPTC compressed, `glGenerateMipmap`, samplers with
  anisotropy and comparison. Not used yet: ASTC, `R11F_G11F_B10F`,
  `RGB10_A2`, texture swizzle, integer render targets.
- **buffer**: `glBufferData`/`SubData`, `glMapBufferRange` with orphaning
  (`alloy/src/gl/buffer.rs`, `alloy/src/gl/staging.rs`). Not used yet:
  uniform buffer objects, transform feedback, `glCopyBufferSubData`.
- **draw**: `glDrawArrays(Instanced)`, `glDrawElements(Instanced)`, VAOs,
  float and integer attributes, divisors, `gl_VertexID` and
  `gl_InstanceID` (`anvil/src/renderer/exec.rs`, `alloy/src/gl/pass.rs`,
  `alloy/src/gl/entry.rs`). Not used yet: primitive restart, lines and
  points beyond the GPU API's, `glDrawRangeElements`.
- **state**: blend (separate functions, equations), depth function, mask
  and range, stencil (anvil clips), scissor, viewport, color mask, cull
  and front face. Not used yet: polygon offset, sample coverage, alpha to
  coverage.
- **shader**: GLSL ES 3.00 compiled at runtime, anvil's own and the app's
  (the GPU API takes app shaders, so any GLSL feature is a path we meet):
  `discard`, `texelFetch`, `textureSize`, derivatives, `sampler2DShadow`,
  `sampler3D`, `sampler2DArray`, `samplerCube`, `usampler2D`, `highp`,
  `layout(location)`, uniform arrays, reflection of attributes and
  uniforms (`alloy/src/gl/program.rs`). Not used yet: program binaries,
  uniform blocks, `#extension` directives.
- **sync**: fences and `glClientWaitSync` (present pacing, readbacks),
  `glFlush`, `glFinish`, `EXT_disjoint_timer_query`
  (`alloy/src/gl/timing.rs`), work split across submissions
  (`alloy/src/gl/slicing.rs`). Not used yet: occlusion queries.
- **threads**: one context, current on the raster thread
  (`alloy/src/raster/mod.rs`, `alloy/src/threads.rs`). Not used yet: a
  second context, shared objects across contexts.

## Our finds

- ours (99e1f2a1) | fbo | A frame that leaves the multisampled window's
  framebuffer for a layer and comes back presents a stale buffer two
  frames in three, and a layer sampled with its passes still queued
  composites black | Adreno 610, GLES 3.2 V@0502.0 | a `glFlush` after the
  layers drawn ahead, and at an inline layer's close
  (`anvil/src/renderer/exec.rs`, `Step::Target`); `DRAW_AHEAD` in the same
  file is the switch to draw every layer inline.
- ours | shader | The first program in the process to draw through a
  `sampler2DShadow` against a cleared, never drawn depth texture returns
  1.0 from every comparison for the rest of its life | Adreno 610, GLES 3.2
  V@0502.0 | a throwaway comparison draw at raster start
  (`alloy/src/gl/texture.rs`, okf/notes/adreno-first-comparison-draw.md).
- ours | context | The core `glGetGraphicsResetStatus` resolves to a
  wrapper stub that returns "no reset" forever; only the EXT spelling is
  real. Android's libGLESv2 answers every known name with a trampoline |
  Adreno 610, GLES 3.2 V@0502.0; any Android driver that lacks a spelling
  | choose the entry by the extension string, never by which name resolves
  (`alloy/src/gl/robustness.rs`).
- ours | context | A long pass (about 2 s GPU time) trips a kernel GPU
  fault and the window goes black with no reset reported by GL | Adreno
  610 (KGSL) | split heavy draws into submissions under a time budget
  (`alloy/src/gl/slicing.rs`), a present-fence watchdog that calls a stuck
  fence a loss, a cap on guilty resets (`alloy/src/raster/mod.rs`;
  okf/done/splat-adreno-gpu-fault.md).
- ours | shader | Attribute reflection lists two nameless FLOAT attributes
  for a program that reads `gl_VertexID` under instancing (the driver
  emulates the built-ins) | Adreno 610, GLES 3.2 V@0502 | skip nameless
  and `gl_`-prefixed attributes (`alloy/src/gl/program.rs`,
  `reflect_attributes`).
- ours | msaa | FBO 0 of a multisampled window surface reports no sample
  buffers | Adreno | ask the surface's EGL config when GL denies it
  (`alloy/src/gl/rig.rs`, `window_samples`).
- ours | blit-read | A blit out of an `EXT_multisampled_render_to_texture`
  attachment through a wrapper FBO fails with `GL_INVALID_OPERATION` on
  frames where the draw did internal texture maintenance | Adreno |
  resolve by a sampling draw, not a blit (`alloy/src/gl/draw.rs`).
- ours | msaa | A driver reports `MAX_SAMPLES` yet rejects the multisampled
  framebuffer as incomplete, and every later try fails too | any | latch
  single-sample for the process on the first rejection
  (`alloy/src/gl/rig.rs`, `anvil/src/renderer/layers.rs`).
- ours | fbo | Fresh render target storage holds leftover tile data from
  unrelated content | Android tilers | clear every new or reused target
  before use (`alloy/src/gl/draw.rs`).
- ours | context | The EGL config scorer picks an RGB565 window buffer when
  the request leaves the color sizes at SDL's defaults | Mali (2017
  MediaTek TV, Mali-T860) | ask for 8-bit color explicitly
  (`ore/src/window/open.rs`, `configure_opengl`).
- ours | context | No EGL config with a multisampled window buffer | the
  Android emulator's GLES translator | fall back to a single-sample window
  (`ore/src/window/open.rs`).
- ours | sync | `glClientWaitSync` never blocks, and a fence made after the
  swap reaches the GPU about two swaps late | ANGLE over D3D11 | flush the
  fence at once (`alloy/src/raster/frame.rs`, `alloy/src/gl/tap.rs`); the
  pacing degrades to check-and-proceed
  (okf/backlog/angle-present-fence-pacing.md).
- ours | present | `eglSwapInterval(1)` does not block the swap, and SDL's
  macOS swap wait covers only CGL | ANGLE over Metal |
  open (okf/upstream/sdl-macos-egl-swap-unpaced-on-angle.md).
- ours | threads | Textures shared across contexts black the window, and a
  teardown racing a GL call crashes | ANGLE over D3D11 | one context on the
  raster thread (`alloy/src/raster/mod.rs`;
  okf/backlog/angle-cross-context-impeller-textures.md).
- ours | threads | Destroying a context after it was released from its
  thread faults (by `eglDestroyContext` or the driver's exit handler); so
  does destroying a context lost to a reset | NVIDIA proprietary, Wayland
  EGL | keep the context current on a parked raster thread, and forget a
  dead context instead of destroying it (`alloy/src/threads.rs`;
  okf/done/context-recovery-nvidia-wayland.md).
- ours | context | Unloading ANGLE's libraries tears down its D3D state,
  and a later `eglInitialize` cannot recover | ANGLE on Windows and macOS
  | keep the libraries loaded for the process life
  (`ore/src/window/open.rs`, `pin_gl_libraries`;
  okf/done/gpu-context-loss.md).
- ours | context | No `EGL_EXT_device_enumeration`, so SDL's offscreen
  video driver cannot start | ANGLE | a headless EGL pbuffer context
  (`ore/src/window/open.rs`; okf/backlog/playback-headless-angle.md).
- ours | sync | Timer queries book a pass's deferred tile work to a later
  query, so per-pass GPU times invert against ground truth | Adreno, and
  tilers in general | a startup probe that measures the attribution
  (`alloy/src/gl/timing.rs`, `attribution_probe`;
  okf/done/gpu-timer-attribution.md).
- ours | present | A driver refuses `eglSetDamageRegionKHR` | any |
  declare once, and on a refusal draw whole frames and say so in the log
  (`alloy/src/raster/repaint.rs`).
- ours, not a fault | msaa | A composite into the multisampled window
  blends one level differently on partly covered pixels after a resolve
  read earlier in the same pass | NVIDIA RTX 3070, desktop driver | none: a
  pixel diff across a change of pass structure shows a one-level edge
  difference, and that is this, not a bug.

## Open checks

Upstream entries that touch a path we use today, on a GPU we run on
today. Each needs a look at our code, and a device check where the code
cannot settle it. A check that finds a fault moves to "Our finds" with
its workaround; a check that finds none says why here.

- **Timer queries around every pass, on Mali.** `PassTimer` wraps each
  pass in a `TIME_ELAPSED` query while the passes switch framebuffers.
  Mali crashes when a user framebuffer is unbound or deleted with queries
  pending (angle flushQueriesBeforeDeletingOrUnbindingFbo, chromium 488;
  Valhall, so the Pixel 7's G710), and needs a complete framebuffer bound
  at `glBeginQuery` (angle bindCompleteFramebufferForTimerQueries). ANGLE
  calls `EXT_disjoint_timer_query` broken on non-Mesa Mali-T8xx and older
  (angle disableTimestampQueries), so the TV's Mali-T860 numbers may be
  garbage, beside what okf/done/gpu-timer-attribution.md found on Adreno.
- **A draw from a nonzero first vertex, on Adreno.** The GPU API's draw
  range passes `first_vertex` to `glDrawArrays(Instanced)`
  (`alloy/src/gl/pass.rs`); Skia calls that broken on every Adreno (skia
  fDrawArraysBaseVertexIsBroken). anvil always draws from 0.
- **A clear dropped across an invalidate, on Mali and Adreno 6xx or
  older.** The driver drops a clear whose value equals an earlier clear
  across an invalidate and keeps stale contents (angle
  doubleClearForRobustInit). anvil's layers invalidate at the end of a
  pass and clear to transparent at reuse; alloy clears new and reused
  targets the same way (`alloy/src/gl/draw.rs`). A pooled layer showing
  its last frame's picture is the symptom to look for on the tablet and
  the TV.
- **A texture moved between sample counts, on Mali-G.** Attaching a texture
  to a framebuffer with a sample count other than its last corrupts it
  unless texture 0 is bound between (skia
  fBindTexture0WhenChangingTextureFBOMultisampleCount; every Mali-G,
  Adreno 3xx). Check whether a pooled layer texture or the rig's resolve
  texture ever changes between `EXT_multisampled_render_to_texture` and a
  plain attachment.
- **A readback from an in-tile multisampled framebuffer, on Adreno.**
  `glReadPixels` is broken on `EXT_multisampled_render_to_texture`
  framebuffers on Adreno drivers above 237 (chromium 280). Check what the
  control API readbacks and the backdrop capture read from.
- **Readbacks across a reset, on Mali.** A readback cut by a GPU reset
  completes without error and delivers unwritten memory (chromium 503).
  The readback paths should check the reset status before they trust
  the bytes.
- **Mipmaps of a render target, on Mali-G710 and G715.** A repeated
  `glGenerateMipmap` on a texture attached to a framebuffer loses the
  context (angle useIntermediateTextureForGenerateMipmap; Pixel 7, Pixel
  8). The GPU API allows a mipmapped render target.
- **Mipmaps of a mutable texture, on Mali and Mesa.** `glGenerateMipmap`
  can silently fail unless the full chain is defined (angle
  recreateMipmapLevelsBeforeGenerate, chromium 498). alloy's mutable
  textures (`glTexImage2D`, `alloy/src/gl/texture.rs`) regenerate the
  chain that way.
- **Buffers deleted in flight, on Mali.** `glDeleteBuffers` on a buffer
  the GPU still reads goes wrong (angle deferGlDeleteBuffers). Check
  anvil's and alloy's buffer releases and the staging ring.
- **Blit regions outside the framebuffer.** On any Linux GPU, Mali and
  NVIDIA, a blit whose regions reach outside the framebuffer misbehaves
  (angle adjustSrcDstRegionForBlitFramebuffer, chromium 292, 499). Check
  that every blit of ours is clipped to both framebuffers.
- **Anisotropy on the sampler object, on Mali-G drivers below r19.** A
  GL error (skia fAnisoSupport). alloy sets it with
  `glSamplerParameterf` (`alloy/src/gl/texture.rs`).
- **The in-tile multisampled path itself.** Skia turns MSAA off entirely
  on Adreno 615, 620 and 621 (b/143074513) and never uses
  `EXT_multisampled_render_to_texture` on NVIDIA; ANGLE turns the
  extension off on older Adreno 4xx and 5xx Android releases, Vivante and
  Windows NVIDIA (angle disableMultisampledRenderToTexture). We use the
  extension wherever it is listed. The latch catches only a driver that
  refuses the framebuffer, not one that draws it wrong.
- **Work reordered across targets, on Adreno 620 and 640 below driver
  571.** `DRAW_AHEAD` in `anvil/src/renderer/exec.rs` is that reordering
  (skia fAvoidReorderingRenderTasks). The tablet's 610 draws it correctly
  with the flush; the 620 and 640 are untested.
- **A framebuffer switch without a flush, on PowerVR.** The same fault as
  ours on the Adreno 610 (angle flushBeforeDrawFramebufferChange). Our
  flush covers the switch back from the layers drawn ahead, not every
  switch.
- **No shader translator on the native drivers.** On Windows and macOS
  ANGLE rewrites GLSL before the driver sees it; on Android and Linux our
  shaders and the app's reach the native compiler as written. The shader
  section below lists the rewrites ANGLE and Chromium apply. The ones
  that touch common GLSL: dynamic indexing of swizzled vectors (all
  Android), vector arguments in `vec`/`mat` constructors (Mali, NVIDIA),
  loops that initialise variables (Adreno), matrix comparisons (Adreno 5xx
  and 6xx), fragment outputs without explicit locations (Qualcomm). The
  GPU API takes app GLSL, so this is a question for its design: run the
  GLSL through a translator, document the traps for app authors, or both.
- **Context loss recovery on Adreno, PowerVR and ANGLE on Android.**
  Chrome gives up on in-process recovery there and restarts its GPU
  process (chromium 299, 365, 367). Our recovery is in-process; it should
  count failed recoveries and have an answer when they repeat.

Confirmed by upstream, no check needed: one real context only, since
multiple contexts are unreliable on every Android driver (chromium 31,
`use_virtualized_gl_contexts`).

## Upstream entries

One line per entry, by path. The GPUs column copies the upstream
condition, so a broad one ("all Android") is as wide as upstream made it.
A Skia entry "set by the Chromium driver bug list" follows the Chromium
entry of that flag.

### context

- chromium 283 (crbug 893177) | Some drivers cannot recover after GL_OUT_OF_MEMORY and context loss | Linux, all GPUs | exit_on_context_lost: restart the GPU process instead of recreating the context
- chromium 299 (crbug 942106) | Context-loss recovery often fails | Android, Imagination PowerVR (GL_VENDOR Imagination.*), GL_RENDERER PowerVR Rogue GE8.* | exit_on_context_lost
- chromium 365 (crbug 1079161) | Context-loss recovery often fails (with SkiaRenderer GL) | Android, Adreno (GL_RENDERER Adreno.*) | exit_on_context_lost
- chromium 367 (crbug 1180962) | ANGLE cannot recover from context loss | Android, GL_RENDERER ANGLE.* | exit_on_context_lost
- chromium 370 (crbug 992286) | Context-loss recovery often fails | ChromeOS, all GPUs | exit_on_context_lost
- angle disableGPUSwitchingSupport (crbug 1091824) | The driver is unstable when the discrete GPU is used on dual-GPU MacBook Pros with NVIDIA | macOS desktop GL on a switchable-GPU Mac that has an NVIDIA GPU | Uses only the low-power GPU (decided at display init; the feature advertises it)
- skia fExternalTextureExtensionString (no GL_ prefix) | A driver reports OES_EGL_image_external_essl3 without the GL_ prefix in the extension string | GLES with GL_OES_EGL_image_external; at least one unnamed driver | Also accepts the extension name without the GL_ prefix
- skia fTiledRenderingSupport (flutter 47164, 47804) | GL_QCOM_tiled_rendering is advertised but glStartTilingQCOM/glEndTilingQCOM are not provided | GLES; some devices advertising GL_QCOM_tiled_rendering (unnamed) | Turns off tiled rendering if either function pointer is missing

### present

- chromium 68 (crbug 339493) | Partial swaps (post sub buffer) on onscreen surfaces are unreliable on Mesa (from the workaround name) | Linux, ANGLE on desktop GL (gl_type angle_gl), GL_RENDERER .*Mesa.* | disable_post_sub_buffers_for_onscreen_surfaces
- chromium 168 (crbug 613722) | Partial swaps are not supported correctly | Linux, VirtualBox virtual GPU (vendor 0x80ee) | disable_post_sub_buffers_for_onscreen_surfaces
- chromium 190 (crbug 339493) | Partial swaps on onscreen surfaces are unreliable on Mesa (from the workaround name) | Linux, ANGLE on desktop GL (angle_gl), GL_VERSION string .*Mesa.* | disable_post_sub_buffers_for_onscreen_surfaces
- angle clampMscRate (crbug 1042393) | GetMscRate returns bogus values (XWayland defaults to 1 Hz when the surface is not visible) | Linux under XWayland | Clamps the rate to 30 Hz when the value is bogus
- angle disableSyncControlSupport (crbug 1137851) | Exposing GLX_OML_sync_control makes the browser unusable on Linux/Wayland (speculative) | Linux Intel on Mesa 20.x | Does not use GLX_OML_sync_control
- skia fBindDefaultFramebufferOnPresent (skbug 398631003) | The default framebuffer must be bound when presenting (no detail in source) | macOS; vendor Apple, native GL only (not ANGLE, not even its GL backend) | Binds the default framebuffer before present

### fbo

- chromium 44 (crbug 301988) | Framebuffer discard (glDiscardFramebufferEXT/invalidate) causes jumpy scrolling | ChromeOS, ARM Mali (GL_VENDOR ARM.*, GL_RENDERER Mali.*) | disable_discard_framebuffer
- chromium 95 (crbug 421271) | glClear does not always work | Android, Imagination PowerVR (GL_VENDOR Imagination.*), GL_VERSION = 3.0 | gl_clear_broken: clear by drawing a quad instead of glClear
- chromium 102 (crbug 457027) | Deleting the bound FBO loses the contents of its attachments | Android, Adreno 4xx (GL_RENDERER Adreno (TM) 4.*) | unbind_attachments_on_bound_render_fbo_delete: detach attachments before deleting the bound FBO
- chromium 103 (crbug 443060) | Draw calls are dropped after FBO invalidation | Android, Adreno 4xx (GL_RENDERER Adreno (TM) 4.*) | disable_discard_framebuffer
- chromium 127 (crbug 518889) | Crash when an incomplete cube map texture is attached to an FBO | Android, Adreno (GL_RENDERER Adreno.*) | force_cube_map_positive_x_allocation: make sure the +X face is allocated
- chromium 159 (crbug 570897) | Framebuffer discard hurts performance on non-tiling GPUs | Linux, NVIDIA (0x10de, GL_VENDOR NVIDIA.*), ANGLE on desktop GL (angle_gl) | disable_discard_framebuffer
- chromium 160 (crbug 601753) | Framebuffer discard gives no benefit on NVIDIA Kepler and later | Linux, NVIDIA (0x10de, GL_VENDOR NVIDIA.*), ANGLE on GLES (angle_gles), GL_VERSION >= 3.0 | disable_discard_framebuffer
- chromium 161 (crbug 601753) | Framebuffer discard gives no benefit on NVIDIA Kepler and later | ChromeOS, NVIDIA (0x10de, GL_VENDOR NVIDIA.*), ANGLE on GLES (angle_gles), GL_VERSION >= 3.0 | disable_discard_framebuffer
- chromium 162 (crbug 601753) | Framebuffer discard gives no benefit on NVIDIA Kepler and later | Android, NVIDIA (GL_VENDOR NVIDIA.*), GL_VERSION >= 3.0 | disable_discard_framebuffer
- chromium 227 (crbug 713854) | Stencil buffers leak | macOS, Intel (0x8086) device ids 0x0412 0x0d26 0x0a2e 0x0a26 0x0d22 0x161e 0x1626 0x162b 0x1622, any GPU of a multi-GPU system | avoid_stencil_buffers
- chromium 315 (crbug 964010) | GL_MESA_framebuffer_flip_y kept off on desktop GL, no fault stated | Linux, ANGLE on desktop GL (angle_gl) | disable GL_MESA_framebuffer_flip_y
- chromium 355 (crbug 1127387) | Float color-buffer renderability is not reported reliably; float formats are taken as renderable | macOS, Intel (0x8086) | force_enable_color_buffer_float
- chromium 356 (crbug 1127387) | Float color-buffer renderability is not reported reliably; float formats are taken as renderable | macOS, AMD (0x1002) | force_enable_color_buffer_float
- chromium 357 (crbug 1127387) | Float color-buffer renderability is not reported reliably; float formats are taken as renderable | macOS, NVIDIA (0x10de) | force_enable_color_buffer_float
- chromium 358 (crbug 1127387) | Float color-buffer renderability is not reported reliably; float formats except RGB32F are taken as renderable | Android, all GPUs | force_enable_color_buffer_float_except_rgb32f
- chromium 471 | Driver can still reference a previously bound complete framebuffer after a rebind | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | ensure_previous_framebuffer_not_deleted
- chromium 476 (crbug 507508103) | Crash when invalidating an incomplete framebuffer | Android, Qualcomm Adreno (GL_VENDOR Qualcomm.*), driver_version < 881 | dont_invalidate_incomplete_fbos
- chromium 480 (crbug 520656244) | A reallocated depth/stencil attachment must be reattached to the FBO (from the workaround name) | Android, Qualcomm Adreno (GL_VENDOR Qualcomm.*), driver_version < 878 | reattach_fbo_depth_stencil_on_reallocation
- chromium 487 (crbug 499602793) | A texture attachment goes stale when the texture layer count increases; reattach needed | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | reattach_texture_to_fbo_after_layer_increase
- chromium 488 (crbug 534468209) | Crash when a user FBO is unbound or deleted with GPU jobs/queries pending (Valhall/Avalon) | Android, ARM Mali (GL_VENDOR ARM.*) | flush_queries_before_deleting_or_unbinding_fbo
- angle allowClearForRobustResourceInit (crbug 848952, crbug 883276) | glClear for resource init is buggy on some drivers and corrupts textures; on macOS uploads are very slow | Clears used only on Apple OS; elsewhere assumed unsafe (drivers not named) | Initializes with data uploads except on Apple OS
- angle alwaysUnbindFramebufferTexture2D (anglebug 42264072) | Attaching a renderbuffer where a texture is attached goes wrong unless the texture is detached first | NVIDIA on Windows or Linux, GL and GLES | Calls framebufferTexture2D with 0 before binding the renderbuffer
- angle clearsWithGapsNeedFlush (anglebug 42266803) | Clearing an FBO with multiple attachments (gaps in draw buffers) can race without a flush | Qualcomm proprietary driver (not Mesa) with GL_VERSION "V@" number < 490 | Flushes around such clears
- angle disableTiledRendering (skbug 9491) | QCOM_tiled_rendering gives rendering artifacts (Nexus 5), or is exposed without its start/end tiling entry points | Adreno 3xx; or any GLES driver exposing GL_QCOM_tiled_rendering without glStartTilingQCOM/glEndTilingQCOM | Disables QCOM_tiled_rendering
- angle doesSRGBClearsOnLinearFramebufferAttachments | Clears of linear attachments go wrong while GL_FRAMEBUFFER_SRGB is enabled | Intel or AMD, GL and GLES | Handles sRGB write control around clears of linear attachments
- angle dontInvalidateIncompleteFBOs (crbug 507508103) | glInvalidateFramebuffer on an incomplete framebuffer misbehaves | Qualcomm proprietary driver with "V@" number < 881 | Skips invalidate calls on incomplete framebuffers
- angle doubleClearForRobustInit (crbug 550489336) | The driver drops a clear whose value equals an earlier clear (clear dedup) across an invalidate, leaving stale contents | Mali, or Adreno 6xx or older (GL_RENDERER "Adreno (TM) N" or "FDN", N < 700), GL and GLES | Robust init clears twice with distinct values
- angle ensurePreviousFramebufferNotDeleted (crbug 569156483) | The driver keeps referencing the previously bound complete framebuffer, so deleting it leaves a dangling reference | PowerVR on Android | Binds an always-complete placeholder framebuffer between every bind transition
- angle finishBeforeDeletingEGLImageRenderbuffer (crbug 587099668) | Deleting an EGLImage-backed renderbuffer still in GPU use leaves a ghost that later frees the EGLImage compression metadata after its source freed it: crash | PowerVR, GL and GLES | Calls glFinish before deleting such renderbuffers
- angle flushBeforeDrawFramebufferChange (crbug 584036299) | Changing away from a non-default draw framebuffer without a flush leaves stale cross-framebuffer attachment state | PowerVR, GL and GLES | Flushes before changing a non-default draw framebuffer binding
- angle flushOnFramebufferChange (crbug 1181068) | Switching framebuffers without a flush can crash | Apple OS on a system with a 9th-gen Intel GPU | Flushes on every framebuffer change
- angle flushQueriesBeforeDeletingOrUnbindingFbo (crbug 534468209) | Unbinding or deleting a user FBO with pending GPU jobs/queries crashes the driver | Mali (any; description names Valhall/Avalon), GL and GLES | Flushes before unbinding or deleting a user framebuffer
- angle reattachFboDepthStencilOnReallocation (crbug 520656244) | FBO depth/stencil attachments go stale when their storage is reallocated | Qualcomm proprietary driver with "V@" number < 878 | Reattaches depth and stencil attachments after reallocation
- angle reattachTextureToFboAfterLayerIncrease (crbug 499602793) | A texture layer attachment goes stale after TexImage3D increases the layer count | PowerVR on Android | Reattaches the texture layer to the framebuffer
- angle recreateFboOnAttachmentRespecification (crbug 551065275) | Respecifying attachments, draw buffers or read buffer of an FBO causes driver hash collisions | PowerVR, GL and GLES | Recreates the native FBO on respecification
- angle rejectRenderbufferStorageWrappingDriverSize (crbug 546644426) | Large renderbuffer allocations wrap an integer in the driver's size computation | PowerVR, GL and GLES | Rejects such allocations with GL_OUT_OF_MEMORY
- angle RGBA4IsNotSupportedForColorRendering | GL_RGBA4 is not color renderable | Intel, desktop GL only | Does not use GL_RGBA4 for color-renderable storage
- skia fInvalidateFBType (Adreno 4xx) | Calling invalidate before a dst copy gives incorrect dst copies | Adreno 430 and other Adreno 4xx (seen on Nexus 6) | Disables framebuffer invalidate/discard
- skia fInvalidateFBType (disable_discard_framebuffer) | Framebuffer discard/invalidate is broken | Set by the Chromium driver bug list (flag disable_discard_framebuffer) | Disables framebuffer invalidate/discard
- skia fMaxPreferredRenderTargetSize | Render targets above 4k are slow | Vendor ARM (seen on Mali-G71) | Prefers render targets no larger than 4096
- skia fPerformColorClearsAsDraws (PowerVR, Adreno 3xx) | glClear of color is not trusted (no reason in source) | Not iOS; PowerVR Rogue, Adreno 3xx (not over Chrome command buffer); also PowerVR SGX54x (no ES3) | Performs color clears as draws
- skia fPerformColorClearsAsDraws (skbug 40038435) | Full screen clears are unreliable | AMD Radeon HD 7xxx and Radeon R9 M4xx renderers | Performs color clears as draws
- skia fPerformColorClearsAsDraws (crbug 768134, 773107, 1039912) | Full screen clears are not always performed; crash rate in glClear after OS update | macOS native GL; vendor Intel with driver < 10.30.12, or Intel Broadwell any driver | Performs color clears as draws
- skia fPerformColorClearsAsDraws (crbug 969609) | glClear sometimes segfaults (mostly macOS 10.13/14, GT 650M, driver 12+) | macOS native GL; vendor NVIDIA, all | Performs color clears as draws
- skia fPerformColorClearsAsDraws (crbug 755871, 768134) | Clears clear too much (partial clears) and full clears are skipped (seen on NVIDIA K620) | ANGLE D3D11 backend | Performs color clears as draws
- skia fPerformStencilClearsAsDraws | Stencil glClear is broken | Adreno 430 and other Adreno 4xx with driver <= 219.0 (fixed between 145.0 and 219.0) | Performs stencil clears as draws
- skia fPerformColorClearsAsDraws (gl_clear_broken) | glClear is broken | Set by the Chromium driver bug list (flag gl_clear_broken) | Performs color and stencil clears as draws
- skia fClearToBoundaryValuesIsBroken (crbug 710443) | Clears to boundary color values are broken (no detail in source) | macOS native GL; Intel Broadwell | Avoids clears to boundary values
- skia fTiledRenderingSupport (skbug 40040812) | Rendering artifacts when GL_QCOM_tiled_rendering is used | Adreno 3xx (seen on Nexus 5) | Disables QCOM tiled rendering
- skia ensure_previous_framebuffer_not_deleted (b/532941869) | Deleting the previously bound framebuffer causes a fault (no detail in source) | Vendor Imagination or Imagination driver | Keeps the previous framebuffer alive (sets the workaround flag)
- skia fRebindColorAttachmentAfterCheckFramebufferStatus (skbug 40043721) | After glCheckFramebufferStatus on a new FBO (status COMPLETE) later transfers fail | Mali-T, ARM driver < 1.26 (seen T880 r22p0; not T860 r26p0) | Binds color attachment 0, then the original, after the check

### msaa

- chromium 132 (crbug 527565) | MSAA performance too low for GPU rasterization | Intel (0x8086), active GPU, every OS except Fuchsia | msaa_is_slow: avoid MSAA
- chromium 163 (crbug 607130) | Multisample renderbuffers in GL_RGB8 are slow | macOS, Intel (0x8086) | disable_webgl_rgb_multisampling_usage: avoid RGB8 MSAA renderbuffers
- chromium 231 (crbug 696126) | Multisampled color renderbuffers cannot be resized (reallocated) | Android, Adreno 4xx/5xx (GL_RENDERER Adreno (TM) [45].*) | multisample_renderbuffer_resize_emulation: recreate instead of reallocating
- chromium 241 (crbug 759471) | MSAA performance too low for GPU rasterization (Android copy of 132) | Android, Intel (GL_VENDOR .*Intel.*) | msaa_is_slow
- chromium 254 (crbug 875471) | MSAA must stay at 4 quality samples and 2 EQAA storage samples (from the workaround name) | ChromeOS, AMD (0x1002) device 0x98e4 (Stoney) | max_msaa_sample_count_4, use_eqaa_storage_samples_2
- chromium 259 (crbug 812071) | 8x MSAA is slow (seen with alpha:false WebGL contexts) | macOS, Intel (0x8086), primary GPU | max_msaa_sample_count_4
- chromium 268 (crbug 797243) | MSAA capped at 4x, reason not stated (from the workaround name) | Android, all GPUs | max_msaa_sample_count_4
- chromium 280 (crbug 890002) | glReadPixels is broken on EXT_multisampled_render_to_texture framebuffers | Android, Adreno (GL_RENDERER Adreno (TM) .*), driver_version > 237.0 | use_copyteximage2d_instead_of_readpixels_on_multisampled_textures
- chromium 316 (crbug 1003860) | MSAA capped at 4x, reason not stated (from the workaround name) | ChromeOS, Intel gen >= 9, Mesa driver, GL_VENDOR Intel.* | max_msaa_sample_count_4
- chromium 317 (crbug 1003860) | MSAA capped at 2x on older Intel, reason not stated (from the workaround name) | ChromeOS, Intel gen < 9, Mesa driver, GL_VENDOR Intel.* | max_msaa_sample_count_2
- chromium 360 (crbug 1145793) | 8x MSAA is slow for WebGL contexts | Windows, Intel (0x8086) | max_msaa_sample_count_4
- chromium 369 (crbug 1184340) | MSAA must stay at 4 quality samples and 2 EQAA storage samples (from the workaround name) | ChromeOS, AMD (0x1002) device 0x15d8 revisions 0xe9/0xea (Pollock) | max_msaa_sample_count_4, use_eqaa_storage_samples_2
- chromium 398 (crbug 527565) | MSAA performance too low for GPU rasterization on pre-Ice Lake Intel | Intel (0x8086) gen < 11, active GPU, every OS except Fuchsia | msaa_is_slow_2
- chromium 399 (crbug 759471) | MSAA performance too low for GPU rasterization on pre-Ice Lake Intel (Android copy of 398) | Android, GL_VENDOR .*Intel.*, Intel gen < 11 | msaa_is_slow_2
- chromium 440 (crbug 376094900) | Implicit MSAA resolve (dmsaa in OOP canvas) produces rendering artifacts | ChromeOS, ARM Mali (GL_VENDOR ARM.*, GL_RENDERER Mali.*) | msaa_is_slow: MSAA not used
- angle disableMSAASampleCount1 (anglebug 467164245) | Multisample sample count 1 triggers driver bugs | Huawei Maleoon (vendor HUAWEI, renderer Maleoon) | Does not expose sample count 1
- angle disableMultisampledRenderToTexture (anglebug 40096530) | GL_EXT_multisampled_render_to_texture is buggy | Android Adreno 4xx with SDK < 22 or SDK 24; Android Adreno 5xx with SDK < 24 or SDK 25; Linux Vivante; Windows NVIDIA | Disables the extension
- angle limitMaxMSAASamplesTo4 (crbug 797243) | Rendering bugs at MSAA sample counts above 4; on Apple switchable graphics GL_MAX_SAMPLES differs between GPUs | Any GPU on Android; Apple OS with Intel, AMD or NVIDIA | Caps sample counts at 4
- angle resetSampleCoverageOnFBOChange (crbug 408364831) | The driver caches values from glSampleCoverage and the bound FBO's sample count, and applies wrong coverage after binding an FBO with a different sample count | Qualcomm (any driver), GL and GLES | Re-applies sample coverage on every FBO change
- skia fMSFBOType (ES3 MSAA on tiled GPU) | ES3 multisampled renderbuffers have driver bugs | GLES; seen on at least one tiled-GPU device (N10); applies to any driver with EXT/IMG_multisampled_render_to_texture | Prefers EXT/IMG multisampled-render-to-texture over ES3 MSAA renderbuffers
- skia fDisallowDynamicMSAA (anglebug 6030 / crbug 466515405) | EXT_multisampled_render_to_texture is incomplete (now removed); large MSAA attachments regress speed | ANGLE D3D11; Intel Ice Lake+ (native or ANGLE vendor) vs all others | Ice Lake+: standard MSAA with explicit resolve; others: no dynamic MSAA
- skia fMSFBOType (b/143074513 / skbug 40042528) | MSAA is not usable (no detail in source) | Adreno 615, 620 and 621 | Disables MSAA entirely
- skia fMSFBOType (skbug 503013389) | Multisampled-render-to-texture extensions are not used (no detail in source) | Driver NVIDIA (GL or GLES) | Uses standard MSAA renderbuffers with explicit resolve, or no MSAA without ES3/extension
- skia fEnforceImageFootprintFitsIn32Bits | MSAA allocation sizes must be checked with 32-bit power-of-two arithmetic | Vendor Imagination, ANGLE vendor Imagination, or Imagination driver | Rejects images whose footprint does not fit in 32 bits
- skia fDisableSRGBRenderWithMSAAForMacAMD | Creating MSAA sRGB render buffers fails | macOS (native GL); vendor ATI/AMD (seen on Mac Pro) | Makes SRGB8_ALPHA8 renderable without MSAA only
- skia fMSFBOType (crbug 527565, 983926) | MSAA is very slow before Gen9 and drivers crash even on Gen9 | Vendor Intel or ANGLE vendor Intel, renderer older than Ice Lake (or Ice Lake+ without fAllowMSAAOnNewIntel) | Disables MSAA
- skia fAvoidLineDraws (anglebug 7796) | Line draws with MSAA are faulty | Intel Ice Lake or newer (native or ANGLE vendor Intel), when MSAA is allowed by option | Avoids line primitives when MSAA is in use
- skia fBindTexture0WhenChangingTextureFBOMultisampleCount | Rebinding a texture to an FBO while switching MSAA and non-MSAA corrupts it; blend issues | Adreno 3xx with Qualcomm driver (seen 330 V@127.0); all Mali-G (seen G72 r19p0, G76 r16p0) | Binds texture 0, then rebinds the original
- skia max_msaa_sample_count_4 | Sample counts above 4 are faulty | Set by the Chromium driver bug list (flag max_msaa_sample_count_4) | Clamps render target sample counts to 4
- skia fColorSampleCounts (iOS simulator) | The iOS simulator reports incorrect sample counts | iOS simulator (TARGET_OS_SIMULATOR) | Rounds each reported sample count down to a power of two

### blit-read

- chromium 93 (crbug 340882) | Pixel buffer objects misbehave, breaking async glReadPixels (Android emulator) | Android, GL_VENDOR VMware.*, GL_RENDERER Gallium.*, GLES 3.0 | disable_async_readpixels: no PBO-based readback
- chromium 137 (crbug 518889) | glReadPixels from an incomplete cube map texture fails | Android, NVIDIA (GL_VENDOR NVIDIA.*) | force_cube_complete: define all cube faces before use
- chromium 140 (crbug 550292) | glReadPixels fails on FBOs with SRGB_ALPHA textures (Nexus 5X, Adreno 418, Android 6.0) | Android, GL_VENDOR Qualcomm, Adreno 4xx (GL_RENDERER Adreno (TM) 4.*) | disable GL_EXT_sRGB
- chromium 216 (crbug 698926) | Pack parameters are applied wrongly while a pixel pack buffer is bound | Android, NVIDIA (GL_VENDOR NVIDIA.*) | pack_parameters_workaround_with_pack_buffer
- chromium 292 (crbug 830046) | glBlitFramebuffer misbehaves when src/dst regions reach outside the framebuffer | Android, NVIDIA (GL_VENDOR NVIDIA.*) | adjust_src_dst_region_for_blitframebuffer: clip the regions first
- chromium 484 (crbug 529867799) | A large pack row length into a pixel pack buffer hits a driver bug | Android, ARM Mali (GL_VENDOR ARM.*) | pack_large_row_length_separately_pack_buffer
- chromium 493 (crbug 546252753) | glBlitFramebuffer needs a prior glFinish when the draw FBO has several color attachments or an attachment with base_level > 0 (from the workaround name) | Android, GL_VENDOR ARM.*, GL_RENDERER Mali-G.* | finish_before_blit_framebuffer_multi_attachment
- chromium 499 (crbug 546630009) | glBlitFramebuffer misbehaves when src/dst regions reach outside the framebuffer | Android, ARM Mali (GL_VENDOR ARM.*) | adjust_src_dst_region_for_blitframebuffer: clip the regions first
- chromium 503 (crbug 550525698) | Readbacks (incl. PBO and buffer-map) cut by a GPU reset complete without error and deliver unwritten memory | Android, ARM Mali (GL_VENDOR ARM.*, GL_RENDERER Mali.*) | check_graphics_reset_status_after_readback
- angle adjustSrcDstRegionForBlitFramebuffer (crbug 830046) | glBlitFramebuffer misbehaves when the source/destination region parameters are large | Any GPU on Linux; Android NVIDIA or Mali; Windows NVIDIA; Apple OS on GLES (iOS) | Adjusts src/dst regions to sane bounds before blitting
- angle clipSrcRegionForBlitFramebuffer (crbug 830046) | glBlitFramebuffer misbehaves when the source region does not match the read framebuffer size | Any GPU on Apple OS (macOS GL, iOS GLES); Linux AMD (GL and GLES) | Clips the source region to the read framebuffer and adjusts the destination to match
- angle copyImageSubData3DFailsOnTailMipLevels (crbug 550607402) | glCopyImageSubDataEXT fails (GL_INVALID_VALUE) on 1x1xD tail mips of 3D textures deeper than wide/high | PowerVR ("Imagination" in vendor string), GL and GLES | Re-uploads from a CPU shadow copy, or copies per slice with glCopyTexSubImage3D
- angle emulateCopyTexImage2D (anglebug 40096747) | glCopyTexImage2D hits a driver bug | Apple GPU vendor (Apple silicon), GL and GLES | Replaces CopyTexImage2D with TexImage2D plus CopyTexSubImage2D
- angle emulateCopyTexImage2DFromRenderbuffers (anglebug 42263273) | CopyTexImage2D spuriously returns errors when the read source is a renderbuffer | Apple OS on GLES (iOS), any GPU (AMD on Windows excluded, redundantly) | Emulates CopyTexImage2D when copying from a renderbuffer
- angle emulatePackSkipRowsAndPackSkipPixels (anglebug 40096712) | GL_PACK_SKIP_ROWS and GL_PACK_SKIP_PIXELS are ignored by glReadPixels | Any GPU on Apple OS (macOS GL, iOS GLES) | Applies the skip rows/pixels offsets itself instead of relying on pack state
- angle finishBeforeBlitFramebufferMultiAttachment (crbug 546252753) | glBlitFramebuffer goes wrong without a glFinish when the draw framebuffer has several color attachments or a color attachment texture with base_level > 0 | Mali (vendor/renderer string contains "Mali"), GL and GLES | Calls glFinish before such blits
- angle finishBeforeSameTextureCopy (crbug 552672183) | Copying from a framebuffer whose read attachment is the destination texture goes wrong while the source still has pending rendering | PowerVR (vendor string "Imagination"), GL and GLES | Calls glFinish before such copies
- angle flushBeforeDeleteTextureIfCopiedTo (anglebug 40644715) | Driver tracks CopyTex(Sub)Image texture dependencies incorrectly, so deleting a copy-target texture goes wrong | Apple OS with Intel GPU | glFlush before glDeleteTextures on a texture that was a CopyTex target
- angle forceLumaWorkaroundForSameTextureCopyTexImage2D (anglebug 486067696) | Same-texture CopyTexImage2D into LUMA (luminance/alpha) formats gives incorrect channel results | Huawei Maleoon (vendor HUAWEI, renderer Maleoon), GLES 3.0+ | Forces ANGLE's LUMA copy workaround path for same-texture copies
- angle packLargeRowLengthSeparatelyPackBuffer (crbug 529867799) | readPixels into a PBO with row pitch >= 256 MiB: Mali computes the stride in bits as int32, which wraps and writes out of bounds; Imagination crashes | Mali or PowerVR, GL and GLES | Packs row by row so the driver only sees PACK_ROW_LENGTH=0
- angle packLastRowSeparatelyForPaddingInclusion (anglebug 42260492) | readPixels into a pixel pack buffer counts an extra row padding for the last row | Any GPU on Apple OS, or NVIDIA (any OS), GL and GLES | Reads the last row separately
- angle packLastRowSeparatelyWhenFillingPackBuffer (crbug 551017142) | readPixels reaching the PBO end loses prior contents of bytes it skips, and counts last-row padding against buffer size | Mali, GL and GLES | Reads the last row separately when the pack could reach the buffer end
- angle packOverlappingRowsSeparatelyPackBuffer | Packing into a pixel pack buffer with overlapping rows (row length smaller than width) gives wrong results | NVIDIA, GL and GLES | Packs row by row
- angle readPixelsUsingImplementationColorReadFormatForNorm16 (anglebug 40096661) | readPixels with RGBA/UNSIGNED_SHORT from EXT_texture_norm16 formats is not implemented correctly | Non-Intel GLES 3.1+ drivers that expose GL_EXT_texture_norm16 | Reads with the implementation color read format/type and converts
- angle readPixelsViaSingleAttachmentFramebuffer (crbug 549347571) | readPixels from a framebuffer with several color attachments returns incorrect data | PowerVR, GL and GLES | Reads through a temporary framebuffer with only the read attachment bound
- angle unsizedSRGBReadPixelsDoesntTransform (crbug 550292, crbug 565179) | glReadPixels on unsized sRGB textures returns raw sRGB values instead of linearized values | Qualcomm proprietary driver (GL_VERSION without "Mesa") | Uses sized sRGB formats; below ES 3.0 disables EXT_sRGB if they are missing
- angle useScratchTextureForConvertingCopySubImage (crbug 576651388) | glCopyTexSubImage2D/3D fail with GL_INVALID_OPERATION when converting between framebuffer and texture formats | PowerVR, GL and GLES | Copies into a temporary 2D texture with glCopyTexImage2D, then transfers with glCopyImageSubData
- skia pack_parameters_workaround_with_pack_buffer | Drivers copy the last row wrongly with GL_PACK_ROW_LENGTH and a pack buffer | GL and GLES; set by the Chromium driver bug list (flag of the same name) | Disables read-pixels row bytes; reads via a tight scratch buffer
- skia fAllowBGRA8CopyTexSubImage | glBlitFramebuffer gives incorrect results while glCopyTexSubImage2D works, also for BGRA8 | GLES; vendor Qualcomm (all Adreno); also Mali-4xx and Tegra pre-K1 (no ES3) | Allows CopyTexSubImage for BGRA8 to avoid glBlitFramebuffer
- skia fDisallowBGRA8ReadPixels | BGRA as a glReadPixels external format is not used (no reason in source) | ANGLE D3D9 backend | Removes GL_BGRA as a read format for BGRA8 and related formats
- skia fDisallowDirectRG8ReadPixels | Readback of a GL_RG8 texture as GL_RG/GL_UNSIGNED_BYTE fails (TransferPixelsFrom test) | macOS native GL; AMD Radeon R9 M3xx, Pro 5xxx, Pro Vega (seen M370X 10.13, Pro 5300M 10.15) | Does not read RG8 directly as GL_RG
- skia fPadRG88TransferAlignment | RG88 readback into a transfer buffer needs 2-byte alignment | macOS, vendor Apple (Apple Silicon) | Pads the RG88 transfer offset alignment to 2 bytes

### texture

- chromium 147 | Max texture size capped at 4096, reason not stated (from the workaround name) | Android older than 14.0, all GPUs | webgl_or_caps_max_texture_size_limit_4096
- chromium 165 (crbug 596774) | Unpacking overlapping rows from a pixel unpack buffer is unstable | Android, NVIDIA (GL_VENDOR NVIDIA.*) | unpack_overlapping_rows_separately_unpack_buffer: upload those rows separately
- chromium 180 (crbug 579060) | eglCreateImageKHR fails for one-component textures | Android, Imagination PowerVR (GL_VENDOR Imagination.*), GL_RENDERER PowerVR .* | avoid_one_component_egl_images
- chromium 217 (crbug 698926) | GL_UNPACK_ALIGNMENT is applied wrongly while a pixel unpack buffer is bound | Android, NVIDIA (GL_VENDOR NVIDIA.*) | unpack_alignment_workaround_with_unpack_buffer
- chromium 301 (crbug 927470) | Allocating large textures hangs the system (Mesa) | Linux with OS version < 5.0, Intel (0x8086) | webgl_or_caps_max_texture_size_limit_4096
- chromium 312 (crbug 991869) | Crash in glTexImage2D | Android, GL_RENDERER .*Mali-T8.* | prefer_draw_to_copy: copy textures by drawing
- chromium 334 (crbug 1052114) | Drivers need the original texture of an EGLImage kept alive (from the workaround name) | Android, all GPUs | dont_delete_source_texture_for_egl_image
- chromium 404 | EGL_EXT_image_dma_buf_import_modifiers unusable for KMS modifier discovery on kms_swrast/llvmpipe | ChromeOS, GL_RENDERER llvmpipe.* | disable_egl_ext_image_dma_buf_import_modifiers
- chromium 425 (crbug 1512211) | Single-component EGL images fail | Android, Intel (GL_RENDERER .*HD Graphics 500.*) | avoid_one_component_egl_images
- chromium 456 | Max texture size capped at 8192, reason not stated (from the workaround name) | Android, all GPUs | webgl_or_caps_max_texture_size_limit_8192
- chromium 482 (crbug 496807874) | Full-image level-0 glTexSubImage2D from a PBO hits a driver bug | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | split_level_0_pbo_full_sub_image_2d: split into two calls
- chromium 483 (crbug 551708184) | glCompressedTex(Sub)Image2D of ASTC hits a driver bug unless the base level is reset | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | reset_base_level_for_astc_image
- chromium 486 (crbug 534903095) | Changing GL_TEXTURE_BASE_LEVEL of an NPOT immutable texture hits a driver bug | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | dont_change_base_level_for_npot_immutable_textures
- chromium 489 (crbug 530045332) | Growing the depth of a 3D texture misbehaves (from the workaround name) | Android, Qualcomm Adreno (GL_VENDOR Qualcomm.*) | lose_gl_context_when_increase_texture_3d_depth: lose the context instead
- chromium 490 (crbug 551573368) | glTexStorage2D hits a driver bug unless the base level is reset first | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | reset_tex_storage_base_level
- chromium 491 (crbug 548085797) | More than 13 texture image units are unreliable (from the workaround name) | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | max_texture_image_units_13
- chromium 492 (crbug 548127218) | Uploading oversized nonzero mip levels misbehaves unless sent via an unpack buffer (from the workaround name) | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | upload_oversized_mip_levels_via_unpack_buffer
- chromium 494 (crbug 553172761) | glTexImage2D of client data into NPOT textures misbehaves | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | use_tex_sub_image_for_client_data_npot_uploads
- chromium 498 (crbug 551065272) | glGenerateMipmap misbehaves unless mip levels are recreated first (from the workaround name) | Android, ARM Mali (GL_VENDOR ARM.*) | recreate_mipmap_levels_before_generate
- chromium 501 (crbug 551673541) | glTexStorage3D mishandles large NPOT 3D sizes (older drivers) | Android, Imagination PowerVR (GL_VENDOR Imagination.*), GL_RENDERER PowerVR.* | round_up_3d_texture_size_to_pot_for_limit
- angle allowAstcFormats | Mesa exposes ASTC on hardware without native ASTC support | ASTC on desktop GL is exposed only for non-Mesa drivers or Mesa Intel Gen9, Gemini Lake, Coffee Lake, Gen11, Gen12; native GLES always | Hides ASTC on other desktop GL Mesa drivers
- angle allowETCFormats | ETC2/EAC through desktop GL (ARB_ES3_compatibility) is emulated on Sandy Bridge, Ivy Bridge and Haswell | ETC on desktop GL is exposed only for Intel newer than Haswell; native GLES always | Hides ETC on other desktop GL drivers and marks ETC1 as emulated
- angle avoid1BitAlphaTextureFormats | 1-bit alpha formats have issues as framebuffer formats | AMD, desktop GL only | Avoids 1-bit alpha formats
- angle bgraTexImageFormatsBroken (anglebug 40096376) | BGRA formats are not accepted despite the extension being exposed | Qualcomm proprietary driver (not Mesa) | Does not expose EXT_texture_format_BGRA8888
- angle decodeEncodeSRGBForGenerateMipmap (anglebug 40644730) | glGenerateMipmap on sRGB textures gives wrong results | Any GPU on Apple OS | Decodes and re-encodes sRGB around mipmap generation
- angle disableClearTexImageForRobustInit (crbug 465918231) | glClearTexImage crashes for packed types, LUMA and depth stencil formats | PowerVR, GL and GLES | Does not use glClearTexImage for robust init
- angle disableClearTexture (crbug 475877320) | EXT_clear_texture crashes the driver (speculative) | PowerVR, GL and GLES | Disables EXT_clear_texture
- angle disableRenderSnorm (anglebug 42266745) | Rendering to snorm formats (EXT_render_snorm) is broken | Mesa < 21.3.0 (GL and GLES), or Mesa < 23.3.0 on GLES | Disables EXT_render_snorm
- angle disableTextureMirrorClampToEdge (anglebug 42266748) | EXT_texture_mirror_clamp_to_edge is broken | Mesa < 23.1.7, GLES only | Disables EXT_texture_mirror_clamp_to_edge
- angle emulateImmutableCompressedTexture3D (crbug 1060012) | Immutable storage for compressed 3D textures hits a driver bug | Qualcomm (any driver) | Uses mutable allocation instead
- angle emulateRGB10 (crbug 1300575) | GL_RGB10 is not used directly on desktop GL (fault not stated) | All desktop GL | Emulates RGB10 with RGB10_A2
- angle flushBeforeGenerateMipmap (crbug 549350408, crbug 549995090) | glGenerateMipmap without a prior flush leaves the driver in a bad state | PowerVR, GL and GLES | Flushes before glGenerateMipmap
- angle limitMax3dArrayTextureSizeTo1024 (crbug 927470) | Large 3D textures and array layer counts hang the system | Intel on Linux kernel < 5.0 | Limits 3D texture size and array layers to 1024
- angle limitWebglMaxTextureSizeTo4096 (crbug 927470) | Large textures cause frequent out-of-memory errors | Any GPU on Android SDK < 34; Intel on Linux kernel < 5.0 | Limits max texture size to 4096 for WebGL contexts only
- angle limitWebglMaxTextureSizeTo8192 (anglebug 42267045) | Large textures cause frequent out-of-memory errors | Any GPU on Android SDK >= 34 | Limits max texture size to 8192 for WebGL contexts only
- angle promotePackedFormatsTo8BitPerChannel (anglebug 42264008) | Packed color formats are buggy | Apple OS on a system with an AMD GPU | Promotes packed formats to 8 bits per channel
- angle readback3DDepthStencilPbosDuringUpload (crbug 550607402) | glTexSubImage3D of depth/stencil, format-converted or subrect PBO data into an immutable 3D or 2D array texture has a race and an offset bug | PowerVR, GL and GLES | Reads the PBO back on the CPU before uploading
- angle recreateMipmapLevelsBeforeGenerate (crbug 491732188) | glGenerateMipmap on a mutable texture may silently fail unless the full mip chain is defined | Mesa (any GPU) or Mali, GL and GLES | Recreates the levels the driver should recreate before generating
- angle recreateTextureOnTexImage3dDepthIncrease (anglebug 530045332) | glTexImage3D on a mutable TEXTURE_3D with an increased depth hits a driver bug | Qualcomm on Android | Recreates the texture and copies the existing levels
- angle resetTexImage2DBaseLevel (crbug 705865) | glTexImage2D with a non-zero base level gives wrong pixels | macOS 10.12.4 or later with Intel GPU | Resets the base level before glTexImage2D
- angle RGBDXT1TexturesSampleZeroAlpha (anglebug 42262386) | Sampling black texels from RGB DXT1 textures returns transparent black | Any GPU on Apple OS | Works around it so RGB DXT1 samples keep opaque alpha
- angle roundUp3dTextureSizeToPOTForLimit (crbug 551673541) | Large 3D textures are mishandled | PowerVR driver version < 26.2 (GL_VERSION "x.y@build") | Rounds 3D dimensions up to a power of two before checking allocation limits
- angle setZeroLevelBeforeGenerateMipmap | glGenerateMipmap fails if texture level 0 is not set | Any GPU on Apple OS | Sets level 0 before glGenerateMipmap
- angle splitLevel0PboFullSubImage2D (crbug 496807874) | Full-image level 0 PBO uploads through TexSubImage2D are buggy | PowerVR driver version < 26.2 | Splits the upload into two calls
- angle unpackLastRowSeparatelyForPaddingInclusion (anglebug 42260492) | Uploads from a pixel unpack buffer count an extra row padding for the last row | Any GPU on Apple OS, or NVIDIA (any OS) | Uploads the last row separately
- angle unpackOverlappingRowsSeparatelyUnpackBuffer | Uploads from a pixel unpack buffer with overlapping rows give wrong results | NVIDIA, GL and GLES | Unpacks row by row
- angle uploadTextureDataInChunks (crbug 1181068) | Large texture uploads hang or crash the driver | Any GPU on Apple OS | Uploads texture data in chunks under 120 KB
- angle useIntermediateTextureForGenerateMipmap (crbug 40279678) | Repeated glGenerateMipmap on textures used as framebuffer attachments loses the context | Mali-G710 or Mali-G715 (Pixel 7 / Pixel 8) | Generates mipmaps through an intermediate texture
- angle useTexSubImageForClientDataNpotUploads (crbug 553172761) | NPOT TexImage2D uploads from client data are buggy | PowerVR, GL and GLES | Uploads through TexSubImage2D instead
- angle virtualizeBaseLevelAndMipchain (crbug 550607402) | Changing GL_TEXTURE_BASE_LEVEL and allocating oversized mip levels on a native mip chain is buggy | PowerVR, GL and GLES | Virtualizes base level and per-level storage with immutable companion textures
- skia fTransferPixelsToRowBytesSupport (anglebug 5542) | Pixel-buffer to texture upload with a row length fails (TransferPixelsToTexture test) | ANGLE D3D9 and D3D11 backends | Disables row length for transfers from buffer to texture
- skia fInternalFormatForRenderbuffer (BGRA8, ANGLE) | ANGLE needs GL_BGRA8_EXT as BGRA renderbuffer format, which EXT_texture_format_BGRA8888 does not define | ANGLE (not Mesa-driven), ES2 and ES3, with GL_EXT_texture_format_BGRA8888 | Uses GL_BGRA8 for BGRA renderbuffers on ANGLE
- skia BGRA8 msaaRenderFlags (Mesa) | glRenderbufferStorage with BGRA errors though EXT_texture_format_BGRA8888 is claimed; old drivers may lack renderable BGRA | GLES with that extension; Mesa errors; non-ANGLE drivers untrusted | BGRA MSAA only on ANGLE not over Mesa; else no MSAA
- skia BGRA8 GL_APPLE_texture_format_BGRA8888 (ES2) | Driver remembers the external format from TexImage; TexSubImage in the other swizzle fails | GLES 2.0 contexts with GL_APPLE_texture_format_BGRA8888 (on ES3 Skia uses TexStorage) | No BGRA on ES2 with this extension
- skia fClearTextureSupport | glClearTexImage is buggy | Desktop GL only; NVIDIA driver < 367.57 (fixed between 340.96 and 367.57) | Disables glClearTexImage
- skia fDisallowTexSubImageForUnormConfigTexturesEverBoundToFBO (skbug 40042902) | TexSubImage to textures once bound to an FBO fails | Adreno 430 and other 4xx, driver < 145.0 (seen Nexus 6P driver 129) | No TexSubImage to unorm textures ever bound to an FBO
- skia fMustSetAnyTexParameterToEnableMipmapping | With sampler objects the GPU sometimes samples the wrong mip level | Not iOS; PowerVR Rogue (seen GE8300, ES 3.2 build 1.10@51309121) | Sets a texture parameter (min filter) on every texture bind
- skia fWritePixelsRowBytesSupport (PowerVR Rogue) | Uploads with GL_UNPACK_ROW_LENGTH give wrong data for alpha8 and gray8 | Not iOS; PowerVR Rogue (seen GE8300, ES 3.2 build 1.10) | Disables UNPACK_ROW_LENGTH for uploads
- skia fTransferPixelsToRowBytesSupport (PowerVR Rogue) | Buffer-to-texture transfers with GL_UNPACK_ROW_LENGTH fail for all color types | Not iOS; PowerVR Rogue (seen GE8300, ES 3.2 build 1.10) | Disables UNPACK_ROW_LENGTH for buffer-to-texture transfers
- skia fDontSetBaseOrMaxLevelForExternalTextures | Setting GL_TEXTURE_BASE_LEVEL or GL_TEXTURE_MAX_LEVEL on GL_TEXTURE_EXTERNAL_OES textures causes problems | Android builds, all GPUs (older Android versions) | Never sets base or max level on external textures
- skia fMipmapLevelControlSupport (crbug 1123874) | Setting the texture base/max levels can crash, perhaps for any texture | Android builds; vendor Imagination (PowerVR), Android up to Q | Disables mip level control
- skia fDoManualMipmapping | glGenerateMipmap produces incorrect mipmaps for sRGB textures | Vendor Intel, vendor ATI/AMD, NVIDIA driver on macOS; needs mip level control; not over Chrome command buffer (crbug 1224110) | Generates mipmaps by iterative downsampling draws
- skia fRegenerateMipmapType (skbug 40045272) | Slow path while rendering mip levels unless MAX_LEVEL is set; ANGLE validation rejects that MAX_LEVEL | Native Intel (no ANGLE); ANGLE with Intel vendor | Native Intel: set BASE and MAX level per level; ANGLE Intel: sync between levels
- skia fAnisoSupport (skbug 40044367) | Setting GL_TEXTURE_MAX_ANISOTROPY as a sampler parameter gives a GL error (texture parameter works) | Mali-G with the ARM driver, driver version < 1.19 (seen G76 r16p0; not G72 r19p0, G77, G78) | Disables anisotropic filtering
- skia fWritePixelsRowBytesSupport (iOS) | TexSubImage copies UNPACK_ROW_LENGTH * height bytes through a staging buffer, which is unsafe | iOS builds, all devices (flutter 16718, Apple 39948888) | Disables UNPACK_ROW_LENGTH for uploads
- skia fTransferPixelsToRowBytesSupport (iOS) | Transfers from a pixel buffer with a row length are unsafe (the GL to Metal layer) | iOS builds, all devices | Disables row length for buffer-to-texture transfers
- skia fDisableBGRATextureStorageForIntelWindowsES | BGRA textures made with texture storage cause problems with cross-context images | Windows; Intel driver with a GLES context | Does not use TexStorage for BGRA8
- skia fDisableLuminance16F | LUMINANCE16F is claimed but does not work; on Iris 6100 it corrupts later reads/writes of other formats | Intel Broadwell or vendor Qualcomm (seen Adreno 330, 418, 530, 540, 630), not on ANGLE | Disables the LUMINANCE16F format
- skia fDisableTexStorage (crbug 945506) | glTexStorage2D raises memory usage (seen on Mali-T720 Android Go) | Android builds; vendor ARM, unless fAlwaysUseTexStorageWhenAvailable or protected content | Disables texture storage
- skia fDisallowUnorm16Transfers | Pixel transfers of 16-bit unorm formats are not used (no reason in source) | Freedreno (Mesa) driver | Removes the transfer flag from R16, RG16 and RGBA16 formats

### buffer

- chromium 290 (crbug 906743) | glBindBufferRange size for uniform buffers must be a multiple of 4 (from the workaround name) | Android, Adreno (GL_RENDERER Adreno (TM) .*) | round_down_uniform_bind_buffer_range_size
- angle bindTransformFeedbackBufferBeforeBindBufferRange (anglebug 42263702) | glBindBufferBase/Range on transform feedback buffers misbehaves unless the buffer was first bound to the generic binding point | Any GPU on Apple OS (macOS GL, iOS GLES) | Binds to the generic GL_TRANSFORM_FEEDBACK_BUFFER point first
- angle deferGlDeleteBuffers (crbug 558870371) | glDeleteBuffers on a buffer still used by in-flight GPU work goes wrong | Mali, GL and GLES | Defers glDeleteBuffers until previous GPU work has completed
- angle limitMaxBufferSizeTo1gb (crbug 475877320) | Allocating and initializing buffers larger than about 1 GB fails inside the driver | Comment says Mac Intel drivers, but the condition (Apple-GPU vendor AND Intel vendor) can never be true, so it is effectively never on | Limits the max buffer size to 1 GB (when enabled)
- skia fPreferClientSideDynamicBuffers | Slow: tiled GPUs handle frequently changing VBOs badly; client arrays measured faster | GL and GLES; vendor ARM, Imagination or Qualcomm; client arrays enabled, not core profile, not Chrome command buffer | Uses client-side arrays for dynamic vertex data
- skia fMapBufferType (Adreno 3xx) | glDrawArrays throws GL_INVALID_OPERATION when drawing from a buffer that was mapped | GLES; Adreno 3xx with driver version > 127.0 (seen on Adreno 306, Android 6.0.1) | Disables buffer mapping and all buffer/texture/surface transfers
- skia fMapBufferType (skbug 40040603) | Buffer mapping misbehaves (reported on skia-discuss) | Intel Sandy Bridge renderer with the Intel driver (reported: HD Graphics 2000, GL 3.1 Build 9.17.10.4459) | Disables buffer mapping
- skia fTransferFromBufferToBufferSupport (PowerVR Rogue) | Buffer-to-buffer copies corrupt draws issued earlier in the command stream (mesh_update GM) | PowerVR Rogue renderer, all driver versions (seen on GE8300, ES 3.2 build 1.13, Android 12) | Disables buffer-to-buffer transfers

### draw

- chromium 49 (crbug 309734) | The first draw from an idle state is slow (GPU wake-up latency) | Android, Qualcomm Adreno (GL_VENDOR Qualcomm.*) | wake_up_gpu_before_drawing: wake the GPU ahead of the real draw
- chromium 221 (crbug 701682) | Very large instanced draw calls crash | Android, Adreno 3xx (GL_RENDERER Adreno (TM) 3.*) | disallow_large_instanced_draw
- angle disableBaseInstanceVertex (anglebug 42266610) | glDraw*BaseVertex* / base instance implementations are buggy | Mali Valhall (Mali-G57, G77, G68, G78, G310, G510, G610, G710, G615, G715), or any GPU on Linux (speculative), GL and GLES | Does not expose ANGLE_base_vertex_base_instance or EXT_base_instance
- angle emulateMaxVertexAttribStride (anglebug 42260722) | Querying GL_MAX_VERTEX_ATTRIB_STRIDE returns 0 | Linux AMD, desktop GL only | Reports an emulated limit instead of the query result
- angle ensureNonEmptyBufferIsBoundForDraw (crbug 1456243) | Drawing with a zero-sized buffer bound to an attribute with a non-zero divisor crashes | Any GPU on Apple OS or Android (description names Apple drivers), GL and GLES | Binds a non-empty buffer in place of the zero-sized one for the draw
- angle initializeCurrentVertexAttributes | Current (generic) vertex attribute values are not set to the spec defaults at context creation | NVIDIA, GL and GLES | Sets the current vertex attributes to the spec defaults at init
- angle setPrimitiveRestartFixedIndexForDrawArrays (anglebug 40096648) | DrawArrays discards vertex data when the fixed primitive restart index falls within the vertex count | Apple OS Intel, when fixed-index restart is emulated (desktop GL 3.1 to 4.2) | Also sets the emulated restart index for DrawArrays calls
- angle shiftInstancedArrayDataWithOffset (crbug 1144207) | glDrawArraysInstanced with a non-zero first vertex is buggy | Apple OS with Intel GPU, except Haswell | Shifts instanced array data by the offset
- angle syncDefaultVertexArraysToDefault (anglebug 355034686) | NVIDIA's OpenGL ES (emulated profile) cannot handle client-side vertex arrays on the default VAO | Feature on for non-core-profile contexts except NVIDIA; the fault is NVIDIA GLES | On NVIDIA does not sync frontend VAOs to the driver default VAO
- angle vertexIDDoesNotIncludeBaseVertex | gl_VertexID does not include the base vertex value | Apple OS with AMD GPU | Compensates for the missing base vertex
- skia fUsePrimitiveRestart | Primitive restart can make draws about 3x slower | GLES only; slowdown seen on Adreno; Skia enables it only on vendor ARM with GLES 3.0+ | Uses primitive restart only on ARM (Mali) ES3
- skia fAvoidLargeIndexBufferDraws | Draws with large index buffers are slow and allocate O(N^2) client-side VBO memory up to the max index value | Vendor Qualcomm (all Adreno), GL or GLES | Avoids draws that use large index buffers
- skia fBaseVertexBaseInstanceSupport (skbug 40043081 / b/40045491) | Base vertex/base instance and indirect draws are very slow (earlier a correctness bug, anglebug 4536) | ANGLE D3D9 and D3D11 backends | Disables base vertex/base instance, native indirect and multi-draw
- skia fBaseVertexBaseInstanceSupport (anglebug 4538) | Base instance is reported where instanced draws are not usable | Any context with base vertex/base instance but without draw-instanced support (anglebug 4538) | Disables base vertex/base instance, indirect and multi-draw
- skia fDrawInstancedSupport (b/188410972) | Instanced draws are not usable | Contexts running over virgl (virtio-gpu) | Disables instanced draws
- skia fRequiresCullFaceEnableDisableWhenDrawingLinesAfterNonLines | Lines are dropped when line draws follow non-line draws | Adreno 3xx, driver > 53.0 (seen 306/320/330, drivers 104.0-140.0; not 53.0) | Toggles GL_CULL_FACE before drawing lines after non-lines
- skia fMaxInstancesPerDrawWithoutCrashing (PowerVR Rogue) | Large instanced draws crash; minimum crashing count between 2^14 and 2^15 instances | PowerVR Rogue renderer with driver version < 23.2 (ANGLE driver version when on ANGLE); seen on a Chromebook | Caps instances per draw at 999
- skia fMaxInstancesPerDrawWithoutCrashing (disallow_large_instanced_draw) | Very large instanced draws crash | Set by the Chromium driver bug list (flag disallow_large_instanced_draw) | Caps instances per draw at 0x4000000
- skia fDrawArraysBaseVertexIsBroken | glDrawArrays with a nonzero base vertex (first) is broken (no detail in source) | Vendor Qualcomm (all Adreno) | Avoids a base vertex in glDrawArrays
- skia fNativeDrawIndexedIndirectIsBroken | glDrawElementsIndirect fails (GrMeshTest on every Win10 Intel bot) | Windows; Intel driver, or ANGLE with Intel vendor on the ANGLE OpenGL backend | Marks indexed indirect draws broken; keeps indirect buffers client-side

### state

- chromium 117 (crbug 488485) | GL_KHR_blend_equation_advanced breaks blending | Android, Qualcomm (GL_VENDOR Qualcomm.*), GL_RENDERER .*4\d\d (Adreno 4xx) | disable_blend_equation_advanced
- angle disableBlendEquationAdvanced (anglebug 42267098) | KHR_blend_equation_advanced has driver issues (Intel desktop fails many Skia blend tests) | Windows Intel; Adreno 4xx; Adreno 5xx; Mali (any) | Disables KHR_blend_equation_advanced and its coherent variant
- angle disableClipControl (crbug 1434317) | Querying the clip control state generates errors | Mali-G72, G76, G51 | Does not expose EXT_clip_control
- angle disableDrawBuffersIndexed | OES_draw_buffers_indexed (per-draw-buffer blend and color mask) is not trusted (fault not stated) | Windows AMD | Does not expose OES/EXT_draw_buffers_indexed
- angle emulateClipDistanceState (anglebug 42266348) | GL_CLIP_DISTANCEi_EXT enable state is ignored | Qualcomm (any driver) | Emulates the clip distance enable state
- angle emulateClipOrigin (anglebug 42266817) | GL_CLIP_ORIGIN_EXT state is applied incorrectly | Qualcomm proprietary driver with "V@" number < 490 that exposes GL_EXT_clip_control | Emulates the clip origin
- angle resyncDepthRangeOnClipControl (anglebug 42266811) | Clip control updates do not take effect until the depth range is set again | Qualcomm proprietary driver (not Mesa) | Re-sends the depth range after a clip control change
- angle srgbBlendingBroken (crbug 40488750) | sRGB blending is broken: write to an sRGB FBO with GL_FRAMEBUFFER_SRGB on and read back the same value; turning it off converts the wrong way | Qualcomm (vendor string, any driver; seen on Nexus 5 and others) | Does not expose EXT_sRGB_write_control
- skia fAvoidDithering (b/195281495) | Slow: steep dithering performance cliff | Android framework builds only; PowerVR Rogue (seen TecnoSpark 3 Pro, GE8300) | Avoids dithering
- skia disable_dual_source_blending_support | Dual-source blending is broken | Set by the Chromium driver bug list (flag disable_dual_source_blending_support) | Disables dual-source blending
- skia fBlendEquationSupport (skbug 40043001) | Advanced blend equations have major known issues | Adreno 430, other 4xx, 530, other 5xx; Intel driver; ANGLE vendor Intel; vendor ARM; Chrome command buffer | Disables advanced blend equations
- skia fBlendEquationSupport (NVIDIA < 337) | Non-coherent advanced blend has an issue | NVIDIA driver < 337.00, native or under the ANGLE OpenGL backend | Disables non-coherent advanced blend
- skia disable_blend_equation_advanced | Advanced blend equations are broken | Set by the Chromium driver bug list (flag disable_blend_equation_advanced) | Disables advanced blend equations
- skia fAdvBlendEqDisableFlags (NVIDIA < 355) | Advanced blend color-dodge and color-burn are faulty | NVIDIA driver < 355.00, native or under the ANGLE OpenGL backend | Disables color-dodge and color-burn advanced equations
- skia fAdvBlendEqDisableFlags (ARM) | Advanced blend color-burn is faulty until a driver fix ships | Vendor ARM with advanced blend | Disables the color-burn advanced equation
- skia fMustResetBlendFuncBetweenDualSourceAndDisable | After a dual-source blend draw, a draw with blending off still reads the second color output | Adreno 530, other 5xx, 620, 640 | Resets the blend function to one without src2 when disabling blend
- skia fNeverDisableColorWrites | Changing the sample mask while color writes are off drops every pixel | PowerVR Rogue (seen GX6250) | Never disables color writes; shaders always write the frag color
- skia fSRGBWriteControl (skbug 40036502) | sRGB write control (GL_FRAMEBUFFER_SRGB toggle) is faulty | Adreno 430 and other Adreno 4xx | Disables sRGB write control

### shader

- chromium 126 (crbug 510637) | Program binaries contain wrong bound attribute locations | Android, Adreno 3xx (GL_RENDERER .*Adreno (TM) 3.*) | disable_program_cache
- chromium 212 (crbug 658074) | Program binaries omit transform feedback varyings | Android, Adreno (GL_RENDERER Adreno.*) | disable_program_caching_for_transform_feedback
- chromium 218 (crbug 699122) | glProgramBinary often crashes | Android, Adreno 4xx/5xx (GL_RENDERER Adreno (TM) [45].*), driver_version >= 103.0 | disable_program_disk_cache
- chromium 243 (crbug 778871) | Program binaries omit transform feedback varyings | ChromeOS, ARM Mali (GL_VENDOR ARM.*, GL_RENDERER Mali.*) | disable_program_caching_for_transform_feedback
- chromium 246 (crbug 784817) | Shader code that initializes variables with loops can crash the driver | Android, all Adreno (GL_RENDERER Adreno (TM) *) | dont_use_loops_to_initialize_variables (translator)
- chromium 262 (crbug 286468) | Program fails if the vertex shader does not set gl_Position | Android, Imagination PowerVR (GL_VENDOR Imagination.*), GL_RENDERER PowerVR.*GX6250 | init_gl_position_in_vertex_shader
- chromium 264 (crbug 286468) | Program fails if the vertex shader does not set gl_Position | Android, Imagination PowerVR (GL_VENDOR Imagination.*), GL_RENDERER PowerVR.*G6200.* | init_gl_position_in_vertex_shader
- chromium 306 (crbug 961950) | Program binaries omit transform feedback varyings | Android, ARM Mali (GL_VENDOR ARM.*, GL_RENDERER Mali.*) | disable_program_caching_for_transform_feedback
- chromium 314 (crbug 1010338) | Crash when using GL_EXT_shader_framebuffer_fetch | Android, GL_RENDERER .*Adreno.* | disable GL_EXT_shader_framebuffer_fetch
- chromium 320 (crbug 709351) | Dynamic indexing of swizzled vectors is broken (from the workaround name) | Android, all GPUs | remove_dynamic_indexing_of_swizzled_vector (translator rewrite)
- chromium 466 (crbug 442879525) | Program binaries omit transform feedback varyings | Android, Imagination PowerVR (GL_VENDOR Imagination.*), GL_RENDERER PowerVR.* | disable_program_caching_for_transform_feedback
- chromium 472 | Compiler bugs around GL_MAX_*_UNIFORM_BLOCKS limits when checked only at link time | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | validate_max_per_stage_uniform_blocks_at_compile_time
- chromium 478 (crbug 510589906) | Program binaries omit transform feedback varyings | Linux, ARM Mali (GL_VENDOR ARM.*, GL_RENDERER Mali.*) | disable_program_caching_for_transform_feedback
- chromium 485 (crbug 529991907) | Too many declared varying output components hit a driver bug | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | limit_output_varyings_at_compile_time
- chromium 496 (crbug 555781989) | Program binary cache entries can collide | Android, Imagination PowerVR (GL_VENDOR Imagination.*), ANGLE on GLES (angle_gles) | disable_program_cache
- chromium 505 (crbug 40259276) | GL_ANGLE_shader_pixel_local_storage kept off on the Imagination GLES driver, no fault stated | Android, Imagination PowerVR (GL_VENDOR Imagination.*), ANGLE on GLES (angle_gles) | disable GL_ANGLE_shader_pixel_local_storage
- angle addAndTrueToLoopCondition | Loop conditions in for and while loops are computed wrongly | Apple OS with Intel GPU | Appends "&& true" to loop conditions
- angle alwaysCallUseProgramAfterLink (crbug 110263) | A driver bug appears unless glUseProgram is called after a successful link | All GPUs (condition true; TODO to limit it to old macOS) | Always calls glUseProgram after a successful link
- angle avoidBindFragDataLocation (anglebug 8646) | Programs fail to link after glBindFragDataLocation | Qualcomm proprietary driver (not Mesa) | Does not call glBindFragDataLocation
- angle avoidComplexExpressionsInStructConstructor (crbug 529509587) | The GLSL frontend writes out of bounds when a struct constructor's array member gets a constant array constructor with a precision mismatch | PowerVR, GL and GLES | Translator moves all complex expressions out of struct constructors
- angle clampArrayAccess (anglebug 40096539) | Out-of-bounds uniform array access reads invalid memory | Any GPU on Android, AMD, or any driver without GL_KHR_robust_buffer_access_behavior | Translator clamps array indices
- angle clampFragDepth | gl_FragDepth is not clamped correctly when rendering to a floating-point depth buffer | NVIDIA (comment: drivers up to at least 390) | Translator clamps gl_FragDepth
- angle clampPointSize | The reported point size range does not match the actual behaviour | Any GPU on Android, or NVIDIA | Clamps gl_PointSize to the reported range
- angle disableNativeParallelCompile (crbug 1094869) | Native KHR_parallel_shader_compile misbehaves in the NVIDIA driver under ThreadSanitizer | Linux NVIDIA, only in TSAN builds | Does not use native parallel shader compile
- angle disablePassthroughShaders (crbug 451796659) | The driver cannot report enough varying/uniform info from a linked program for passthrough shaders, so linking fails | Condition is Adreno 4xx (the description and comment say ES2 Mali / Mali 400) | Disables passthrough shaders
- angle disablePixelLocalStorage (anglebug 40096838) | ANGLE_shader_pixel_local_storage (built on framebuffer fetch / shader images) is broken | Mali-T8xx or older; Mali-G31, G51, G52, G71, G72 | Does not expose pixel local storage
- angle dontInitializeUninitializedLocals (anglebug 40096454) | Translator initialization of uninitialized locals triggers odd driver behaviour | Qualcomm proprietary driver (comment: Marshmallow Adreno 4xx) | Does not zero-initialize locals
- angle dontUseLoopsToInitializeVariables (crbug 809422) | for loops used to initialize variables hit native GLSL compiler bugs | Qualcomm proprietary driver, or Apple OS Intel | Initializes variables without loops
- angle emulateAbsIntFunction (crbug 642227) | abs(i) on an integer returns an unexpected result | Apple OS with Intel GPU | Emulates abs(int) in the translator
- angle emulateAtan2Float (crbug 672380) | atan(y, x) may return a wrong answer | NVIDIA (comment: drivers after 364, at least up to 375) | Emulates atan(y, x)
- angle explicitFragmentLocations (anglebug 42266740) | Fragment outputs need explicit location layout qualifiers to work | Qualcomm (any driver) | Always writes explicit location qualifiers for fragment outputs
- angle forceMaxCombinedShaderOutputResources | GL_MAX_COMBINED_SHADER_OUTPUT_RESOURCES is reported incorrectly | NVIDIA, or PowerVR Rogue (GL_RENDERER starts "PowerVR Rogue") | Forces it to the sum of the image, SSBO and color attachment limits
- angle initFragmentOutputVariables (crbug 1171371) | A fragment shader that does not initialize gl_FragColor causes context loss | Adreno number < 430 (2xx, 3xx, 40x, 42x) | Translator initializes fragment outputs
- angle limitMaxTextureImageUnitsTo13 (crbug 577781532) | Programs with more than 13 active fragment samplers break the driver | PowerVR, GL and GLES | Clamps GL_MAX_TEXTURE_IMAGE_UNITS to 13 so such programs fail to link
- angle limitShaderVariableFieldsAtCompileTime (crbug 583945831) | The driver indexes global variable and struct member info with 16-bit ints, wrapping at 65536 entries | PowerVR, GL and GLES | Limits declared variables and expanded fields per shader at compile time
- angle limitVaryingsAtCompileTime (crbug 529991907) | Shaders that declare too many varyings fail to compile | PowerVR, GL and GLES | Limits declared varying components at compile time
- angle passHighpToPackUnormSnormBuiltins (anglebug 42265995) | packUnorm4x8 fails unless it gets a highp vec4 (seen on Pixel 4) | Qualcomm (any driver) | Translator passes highp arguments to pack unorm/snorm built-ins
- angle preAddTexelFetchOffsets (crbug 642605) | texelFetchOffset treats a negative position as invalid even when position plus offset is in range | Apple OS with Intel GPU | Rewrites to texelFetch(sampler, position + offset, lod)
- angle preTransformTextureCubeGradDerivatives (anglebug 42266857) | Explicit cube map derivatives need a vendor-specific transformation to sample correctly | Apple GPU vendor (Apple silicon) | Translator transforms the derivatives
- angle reapplyUBOBindingsAfterUsingBinaryProgram (anglebug 42260591) | UBO bindings are forgotten after loading a program binary | AMD, or any GPU on Android | Reapplies the UBO bindings after a binary load
- angle removeDynamicIndexingOfSwizzledVector (crbug 709351) | Dynamic indexing of swizzled l-values works incorrectly | Any GPU on Apple OS, Android or Windows | Translator removes the dynamic indexing
- angle removeInvariantAndCentroidForESSL3 | ESSL3 invariant and centroid usage is not accepted as-is by GLSL 4.1 or lower (spec difference) nor by AMD desktop drivers | Desktop GL 4.1 or lower, or AMD desktop GL | Translator removes invariant and centroid
- angle rewriteRepeatedAssignToSwizzled | Repeated assignment to swizzled values inside a user-defined function gives incorrect results | NVIDIA (comment: fixed since 397.31) | Translator rewrites the assignments
- angle scalarizeVecAndMatConstructorArgs (crbug 1420130) | vec/mat constructors with vector or matrix arguments hit driver bugs | Mali or NVIDIA, GL and GLES | Translator scalarizes constructor arguments
- angle unfoldShortCircuits (anglebug 42263407) | Both sides of && and || run when they should short-circuit | Any GPU on Apple OS | Translator unfolds them into if statements
- angle useUnusedBlocksWithStandardOrSharedLayout | Unused std140 or shared uniform blocks are treated as inactive | Apple OS desktop GL, or Linux AMD | Translator keeps such blocks referenced
- angle validateMaxPerStageUniformBlocksAtCompileTime (crbug 475877320) | The compiler crashes on shaders with more than the uniform block limit | PowerVR, GL and GLES | Validates GL_MAX_*_UNIFORM_BLOCKS at compile time instead of link time
- skia fPreferFlatInterpolation | Flat interpolation is slow; ANGLE D3D inserts a geometry shader, ANGLE Metal rewrites index buffers for the provoking vertex | Vendor Qualcomm (tested Adreno 405, 530); ANGLE D3D9, D3D11 and Metal backends | Does not prefer flat interpolation
- skia fInfinitySupport (ANGLE Metal Intel) | isinf/isnan disable fast-math and the Metal shader compiler service hangs on some shaders | ANGLE Metal backend with ANGLE vendor Intel (Intel Macs) | Reports no infinity support
- skia fNonsquareMatrixSupport (skbug 40040603) | Shader compile error converting mat3x2 * vec3 result to a vec2 varying | Intel Sandy Bridge renderer with the Intel driver (HD Graphics 2000, GL 3.1 Build 9.17.10.4459) | Disables non-square matrix support in generated GLSL
- skia fFBFetchSupport (Adreno 5xx) | Framebuffer fetch is advertised but broken | Adreno 530 and other Adreno 5xx (seen Galaxy S7), all driver versions | Disables framebuffer fetch
- skia fMustForceNegatedAtanParamToFloat | atan second argument written as "-x" is read as an int | Vendor Intel, all | Writes -1.0 * x instead
- skia fMustForceNegatedLdexpParamToMultiply (skbug 40043167) | The compiler mishandles ldexp(..., -x) | macOS; vendor ATI/AMD | Writes ldexp(..., x * -1)
- skia fMustDoOpBetweenFloorAndAbs | floor and abs on the same line give bogus values, even when split | Vendor Intel (some GPUs; condition covers all Intel) | Inserts an operation between floor and abs
- skia fRequiresLocalOutputColorForFBFetch | With framebuffer fetch, reading the output color returns the original dst color even after a write | Vendor Qualcomm with framebuffer fetch support | Uses a local output color variable
- skia fMustObfuscateUniformColor | Compiler deduces opaque output from an opaque uniform color and drops the injected blending, turning SrcOver into Src | Mali-T renderers (newer Mali per comment) | Adds math on the uniform color to defeat the analysis
- skia fPerlinNoiseRoundingFix | Double table lookups in Perlin noise give artifacts without snapping to multiples of 1/255 | Android builds, all GPUs (seen Tegra3, PowerVR GE8320, Mali-G76, Adreno 308) | Snaps lookup coordinates to multiples of 1/255
- skia fMustGuardDivisionEvenAfterExplicitZeroCheck (anglebug 2098) | D3D compiler errors "NaN and infinity literals not allowed" on a division guarded by a zero check | Windows; ANGLE D3D9/D3D11 backends or Chrome command buffer | Adds an epsilon to division denominators
- skia fRewriteMatrixComparisons | Comparing a pair of matrices gives incorrect results | Adreno 530, other 5xx, 615, 620, 621, 630, 640, other 6xx | Rewrites matrix comparisons
- skia fCanUseFragCoord (skbug 40035225) | gl_FragCoord sometimes flipped (mostly older drivers); old compilers crash on gl_FragCoord.zw | Adreno 3xx, all drivers | Never uses gl_FragCoord
- skia unfold_short_circuit_as_ternary_operation | Short-circuit && and || evaluate wrongly | Set by the Chromium driver bug list (flag unfold_short_circuit_as_ternary_operation) | Unfolds short-circuit operators into ternaries
- skia emulate_abs_int_function | abs() on int is broken | Set by the Chromium driver bug list (flag emulate_abs_int_function) | Emulates integer abs
- skia rewrite_do_while_loops | do-while loops are miscompiled | Set by the Chromium driver bug list (flag rewrite_do_while_loops) | Rewrites do-while loops
- skia remove_pow_with_constant_exponent | pow() with a constant exponent is miscompiled | Set by the Chromium driver bug list (flag remove_pow_with_constant_exponent) | Replaces pow with a constant exponent
- skia fMustWriteToFragColor (Adreno) | Shaders must always write the fragment color (no detail in source) | Adreno 3xx and other Adreno 4xx | Always writes to the fragment color
- skia fExternalTextureSupport (skbug 40038974) | Drivers advertise only ES2 image_external but need _essl3 for ESSL3; others need the ES2 one with ESSL3 | Drivers with GL_OES_EGL_image_external but not _essl3, ESSL3-level GLSL | Enables both extensions in shaders
- skia fRemoveConstFromFunctionParameters | Shader compilation fails with no message when a function parameter is const | PowerVR Rogue, driver version < 1.15 (ANGLE driver version when on ANGLE); seen GE8300 ES 3.2 build 1.13 | Strips const from function parameters
- skia fNoPerspectiveInterpolationSupport (crbug 986581) | GL_NV_shader_noperspective_interpolation is advertised but works only in ESSL 3.20 shaders | Vendor Qualcomm with GLSL generation other than 3.20 es | Disables noperspective interpolation
- skia fNoDefaultPrecisionForExternalSamplers (flutter 38700) | A default precision for samplerExternalOES is not accepted | Android emulator driver | Omits the default precision statement for external samplers
- skia fProgramBinarySupport (skbug 40041731) | Program binary caching is flaky | ANGLE D3D9 backend with ANGLE vendor ATI/AMD | Disables program binaries
- skia fShaderDerivativeSupport (crbug 1197152 / b/187364475) | Shader derivatives are broken (no detail in source) | PowerVR Rogue with the Imagination driver, driver version < 1.16 (ANGLE driver version when on ANGLE); seen on ChromeOS | Disables shader derivatives
- skia fNoPerspectiveInterpolationSupport (Mali) | Slow: noperspective has no hardware support and is emulated by patching shaders | Mali-G and Mali-T renderers | Uses smooth interpolation instead of noperspective

### sync

- chromium 112 (crbug 477514) | EXT_disjoint_timer_query fails after 2 queries (Android Lollipop) | Android, Qualcomm (GL_VENDOR Qualcomm.*) Adreno 3xx (GL_RENDERER Adreno (TM) 3.*) | disable_timestamp_queries; disable GL_EXT_disjoint_timer_query(_webgl2)
- chromium 113 (crbug 477514) | EXT_disjoint_timer_query fails after 256 queries | Android, Adreno 4xx (GL_RENDERER Adreno (TM) 4.*) | disable GL_EXT_disjoint_timer_query(_webgl2)
- chromium 115 (crbug 462553) | glGetIntegerv(GL_GPU_DISJOINT_EXT) raises GL_INVALID_ENUM | Android, NVIDIA (GL_VENDOR NVIDIA.*), GL_VERSION >= 3.0 | disable GL_EXT_disjoint_timer_query
- chromium 125 (crbug 509727) | glFinish does not flush (clear) driver caches | Android, Adreno (GL_RENDERER Adreno.*) | unbind_egl_context_to_flush_driver_caches: release the current EGL context to flush them
- chromium 247 (crbug 702980) | EXT_disjoint_timer_query is broken (described as Galaxy S6) | Android, machine model SM-G930F, GL_VENDOR ARM.*, GL_RENDERER Mali-T880 | disable GL_EXT_disjoint_timer_query
- chromium 265 (crbug 811661) | glQueryCounter() and glBeginQuery() crash | Linux, GL_VENDOR VMware.* | disable GL_ARB_timer_query, GL_EXT_timer_query, GL_EXT_disjoint_timer_query(_webgl2)
- chromium 272 (crbug 858879) | A deleted GL_TIMESTAMP query id cannot be reused for other query targets | Android, Qualcomm Adreno (GL_VENDOR Qualcomm.*) | disable_timestamp_queries
- chromium 381 (crbug 1246254) | Waiting on an EGL fence that has already signalled misbehaves on i965 (from the workaround name) | ChromeOS, Intel gen >= 4, Mesa driver, GL_VENDOR Intel.* | check_egl_fence_before_wait
- chromium 409 (crbug 1393646) | A fence can be submitted ahead of earlier work: the driver reorders per-buffer queues (upstream hypothesis) | ChromeOS, GL_VENDOR freedreno (Qualcomm Adreno, Mesa; described as QC7c) | flush_before_create_fence: glFlush before creating a fence
- angle bindCompleteFramebufferForTimerQueries (crbug 1356053) | beginQuery for TimeElapsed or Timestamp needs a complete framebuffer bound | Mali, GL and GLES | Binds a complete framebuffer around timer queries
- angle disableSemaphoreFd (crbug 1046462) | GL_EXT_semaphore_fd is broken | Linux AMD on Mesa < 19.3.5 | Disables GL_EXT_semaphore_fd
- angle disableTimestampQueries (crbug 811661) | GL_EXT_disjoint_timer_query is broken | Linux VMware; Android NVIDIA; Android SDK < 27 with Adreno 5xx or older; non-Mesa Mali-T8xx or older; non-Mesa Mali-G31 or older | Disables GL_EXT_disjoint_timer_query
- angle finishDoesNotCauseQueriesToBeAvailable | glFinish does not make all query results available | NVIDIA, desktop GL only | Does not rely on glFinish for query availability
- angle queryCounterBitsGeneratesErrors (anglebug 42261713) | Querying GL_QUERY_COUNTER_BITS for timer queries generates errors | Nexus 5X (vendor and device id) | Leaves the counter bits at 0 without querying
- skia flush_queries_before_deleting_or_unbinding_fbo | Pending queries must be flushed before an FBO is deleted or unbound (no detail in source) | Vendor ARM (Mali) | Sets the flush-queries workaround flag
- skia fPreferVRAMUseOverFlushes | Deferring flushes can starve the GPU | Windows builds; ANGLE D3D9/D3D11 backends or Chrome command buffer | Prefers flushes over extra VRAM use
- skia fTextureBarrierSupport (Intel CherryView) | After a draw with a texture barrier, the next draw blends with a dst as if the barrier draw did not happen | Intel Cherry View renderer (seen on Linux HD 405) | Disables texture barrier support
- skia fAvoidReorderingRenderTasks (skbug 40042245) | Reordering render tasks gives faults on old drivers (no detail in source) | Adreno 620 and 640 with driver version < 571.0 | Does not reorder render tasks

### threads

- chromium 22 (crbug 230896) | Context switching is buggy; the bound FBO has to be released before a switch | Android, Imagination PowerVR (GL_VENDOR Imagination.*) | unbind_fbo_on_context_switch: unbind the framebuffer before switching contexts
- chromium 31 (crbug 154715) | Multiple real GL contexts are unreliable on Android drivers (15 crbugs listed) | Android, all GPUs | use_virtualized_gl_contexts: one real GL context shared by virtual contexts
- angle unbindFBOBeforeSwitchingContext (crbug 1181193) | Context switching while a framebuffer is bound is buggy | PowerVR, or Windows NVIDIA | Unbinds the FBO before switching contexts

### other

- skia fDisableTessellationPathRenderer | Skia tessellation path renderer draws wrong (dropped draws, flat curves, garbage, failed conic strokes); GL feature not isolated | Vendor Intel (Iris 640); Mali-T (Galaxy S6); Adreno 3xx, 430, other 4xx (405); ANGLE D3D9 | Disables the tessellation path renderer
- skia fDisableTessellationPathRenderer (flutter 113596) | Tessellation path renderer puts vertices in wrong places; GL feature not isolated | PowerVR Rogue, driver < 1.15 (seen GE8320 1.13 Android 12; fine on 1.10 and 1.15) | Disables the tessellation path renderer

