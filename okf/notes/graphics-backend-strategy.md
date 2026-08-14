---
title: Graphics backend strategy
description: Settled direction; one GLES contract over ANGLE on every platform, native Vulkan rejected, Metal-native kept only as a mapped contingency; includes the Impeller texture-interop analysis and the ANGLE-sunset risk ledger.
created: 2026-08-12
---

# Graphics backend strategy

The question that started this: only OpenGL (via glow) is supported today,
and the intention was to keep the door open for Vulkan and/or Metal. Working
it through ended somewhere better: the door does not need to be walked
through. One GLES contract, translated per platform by ANGLE, is the
strategy, not a stopgap. This note records the analysis and the decision
ledger so the reasoning is not re-litigated from scratch.

## Where GL actually lives today

An audit for hard GL/GLES/glow dependencies in the core layers found the
containment already good:

- glow is confined to alloy's backend modules (gpu/, raster/, texture.rs,
  gl.rs). The rendertree has zero texture/gl/gpu imports. The flux gui
  plugins marshal neutral vocab types and ids only. forge, lattice and
  packages/core have no glow constructs.
- The app-facing vocabulary is deliberately WebGPU-flavored and neutral:
  topology/index-format/cull names, WebGPU winding rule, y-down clip space
  documented in target-space terms, branded plain-number ids, queried limits.
  The ES 3.0 restrictions in the contract (no base vertex/instance, GLES
  limit floors) define the portable subset, which helps any future backend.
- The one hard contract dependency is the shader language: GLSL ES 3.00
  source strings, gl_* builtins, the injected `#version 300 es` preamble.
  This is fine: GLSL translates (naga alone does GLSL to SPIR-V and MSL, and
  provides the reflection a name-based params model needs). GLSL ES stays
  the authoring language permanently.
- Cosmetic cleanups available, no urgency: sweep explanatory GL references
  out of core docs (glBlendFunc, glDrawElements and similar), and soften the
  "compiles exactly as written" promise for own-#version sources to "as
  written, subject to translation".

## Why not wgpu (stays retired)

wgpu was tried and thrown out (verbosity, crashes). The deeper reason it
stays out: WebGPU's explicitness - and WGSL's verbosity - exist to let a
browser statically validate untrusted code and translate it three ways.
Explicit bind groups replace GL's name-based driver reflection; the plumbing
the GL driver does behind your back gets moved into source text. Those are
the web's goals. solidrt runs a single trusted app and validates at its own
boundary, which is GLSL's model exactly. The verbosity buys nothing here.

## The Impeller texture interop analysis

The one hard issue for any non-GL backend is texture interop with Impeller.
Findings (verified against the impellers 0.4.2 crate and flutter master
impeller.h, 2026-08-12):

- The interop texture API is GL-only in BOTH directions: adopt
  (CreateWithOpenGLTextureHandleNew) and extract (GetOpenGLHandle). No
  Metal/Vulkan texture handle APIs exist upstream. Metal/Vulkan interop
  stops at surfaces (wrapped drawables, swapchains).
- The interop PixelFormat enum has only RGBA8888. r8 works today only
  because adoption wraps a real R8 GL name and the descriptor is never
  checked against it.
- GL adoption fails on a non-GL Impeller context, so on a Metal context the
  current adoption flow cannot exist at all.
- Inverted flow (Impeller allocates, backend renders into it) works today on
  stock libimpeller for GL: CreateWithContentsNew + GetOpenGLHandle (lazy,
  call with context current) + FBO attach. A feature-flag rehearsal on GL
  would prove the alloy-side inversion without landing anything.
- If a native bridge is ever needed, extraction is a small self-patch to
  libimpeller (return the handle already in Impeller's texture struct);
  adoption is the significant-effort direction (foreign allocator, lifetime,
  format reconciliation) and is rejected.
- Vulkan extraction would additionally need a layout handshake against
  Impeller's internal image-layout bookkeeping (transition out and back to
  the expected layout within one submission; desktop GPUs forgive layout
  lies, tiled mobile GPUs do not). This risk evaporated when native Vulkan
  was dropped (below).

## Two GL semantics the other APIs do not share

Neither of these decided anything above, but both explain why parts of the
current design have no cleaner GL alternative.

**Vsync is fused to swap.** With OpenGL/EGL, synchronizing to the display
happens as a side effect of `eglSwapBuffers` (what `SDL_GL_SwapWindow` calls).
There is no standard GL way to block until vblank without also presenting a
frame. The portable workaround, if it is ever needed, is to swap a tiny 1x1
pbuffer surface with vsync enabled - that blocks on vblank without touching the
window - which is hacky but stays within EGL rather than per-platform code.

Both alternatives separate the two cleanly, which is the shape to keep in mind
if the contingency backends are ever revisited: Vulkan's
`vkAcquireNextImageKHR` on a `VK_PRESENT_MODE_FIFO_KHR` swapchain blocks until
the next vblank slot as a first-class part of the API, and Metal's
`CAMetalLayer.nextDrawable` blocks until a drawable is available at the next
vsync (with MTKView's draw delegate firing on the display-link cadence). GL is
the odd one out.

**Writes through a shared context are not visible until they complete.** When
two GL contexts share objects, the reader sees undefined contents until the
writer's commands have actually completed; `glFlush` only submits them. We paid
for this once, when the UI thread wrote textures (shader output, uploads, and
snapshot-boundary rasterization) that the render thread sampled: `glFlush`
alone produced intermittent corruption - nothing, shifted content, wrong
glyphs, colored rects in black, fully filled rects - on newer deeply-pipelined
Android GPUs only. Older devices, the emulator and Linux happened to finish the
write before the read. A blocking `glFinish` fixed the corruption by stalling
the JS thread; a `glFenceSync` on the writer plus a server-side `glWaitSync` on
the reader fixed it properly, ordering the GPU without stalling either CPU
thread.

None of that plumbing survives, because the raster thread now owns the only GL
context in the process (`alloy/src/raster/mod.rs`) and there is no second
context to be incoherent with - which is a large part of why single-context is
worth defending. The fences left in the tree are for present pacing, a
different job. Reintroducing a second context reintroduces this whole class.

## Decision ledger

1. ANGLE is a strategic dependency, not a layer to shed. It is
   Google-maintained infrastructure (Chrome's WebGL on Windows/macOS;
   Android is making ANGLE-on-Vulkan the platform GLES driver), CTS
   conformant, and encodes two decades of driver-quirk workarounds -
   exactly the maintenance we do not want to own. Dropping it would mean
   maintaining two native surfaces ourselves for negative gain.
2. Native Vulkan is off the roadmap entirely. Vulkan-land is where ANGLE's
   quirk moat is most valuable and where a first-party team with device
   farms should own the problem. Zink (Mesa's conformant GL-on-Vulkan,
   non-Google) exists as an independent second road should ANGLE ever rot.
3. Metal-native is a mapped contingency, not a roadmap item. Metal was
   assumed to become a necessity for Apple platforms; it is not. ANGLE on
   iOS/macOS sits on Metal and does not touch Apple's deprecated GLES, so
   the tower is the same architecture already shipped on Windows
   (Impeller-GLES + app GPU backend, ANGLE translating). If the contingency
   ever fires, the path is: GL inversion rehearsal, self-built libimpeller
   with the extraction getter, alloy Metal lowering, naga for GLSL to MSL.
   Its honest dividends are operability, not speed: Xcode GPU tooling,
   input-to-present latency tail (encode is on the critical path in this
   deliberately non-pipelined same-frame design; matters for cold-clock
   first-frame-after-idle, 120Hz, heavy vector frames), binary size (ANGLE
   out, naga in), memoryless MSAA on TBDR. Shader/GPU-bound work gets zero
   faster.
4. iOS requires owning builds either way: the impellers crate prebuilts
   cover windows/macos/linux/android only (build.rs panics on iOS), so iOS
   means a self-built libimpeller plus a patched/vendored crate, and on the
   tower path an ANGLE-for-iOS build as well.

## The product principle that anchors it

"If it runs on your Linux box, you don't have to worry whether it works on
any other platform." The Windows present-fence episode proved the value: the
approach was spec-wrong all along, Android/Linux GL drivers were merely
lenient, and ANGLE-on-D3D11 - a second independent implementation of the
same contract - surfaced it as a reproducible failure. One API across N
translation backends is continuous conformance testing of our rendering
code. Per-platform native code paths break that guarantee structurally:
they create bugs the Linux dev loop cannot surface. This is the strongest
argument for the single-contract strategy and the standing argument against
any native backend.

Open idea from the same principle: the Linux dev box currently runs native
GL, likely the most lenient backend in the fleet. Running the dev
configuration through ANGLE (ideally ANGLE-on-Vulkan with validation
layers) would make dev the strictest referee, turning the promise from
hoped-for into enforced.

## Risk: Google sunsets ANGLE

Sized and accepted. Likelihood very low: ANGLE is infrastructure bound to
Chrome and Android (not a product), Apple co-maintains upstream for
WebKit's WebGL, BSD-licensed. Impact is slow rot of the vendored build
against new OS/GPU generations - not a cliff - with years of runway and
visible signals (commit activity, Chrome/Android moves). Hedges standing:
the Metal extraction contingency above, and Zink for Vulkan-land.

## Follow-ups

- Dropped 2026-08-12: the dead Backend::Vulkan/Metal enum and DisplayContext
  stub variants in alloy (backend.rs and the backend field plumbed through
  GpuTexture/RasterState). The real future-proofing is the neutral vocab
  layer and the engine-independent rendertree, not empty enum arms.
- Optional doc sweep of GL vocabulary in core docs; soften "compiles
  exactly as written".
- Cheap and parallel: file the upstream Impeller feature request for
  backend-native texture extraction anyway; if it lands, the contingency
  patch evaporates.
- Consider ANGLE(+validation) as the Linux dev configuration.
