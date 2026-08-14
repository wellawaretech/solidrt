---
title: ANGLE textures and teardown crash
description: "The two Windows client killers (a snapshot boundary's cross-context texture blacking the window under ANGLE, and the engine-restart GL teardown race) are both fixed by the single-context + raster-thread architecture; stage 2 non-blocking creates/readbacks and an unexplained two-client dev-query timeout remain."
created: 2026-07-27
---

# ANGLE textures and teardown crash

Source: Windows client debugging session 2026-07-19 (crushy, win32-x64-msvc
0.0.30 with the GL serialization fix 8dff06d included; NVIDIA RTX 3070,
ANGLE 2.1.26628 / D3D11). Two distinct bugs, both ANGLE-only; Linux GL is
unaffected.

## Bug 1: one snapshot boundary blacks the entire window frame

Symptom: a window that presents at 60fps but is fully black, while MCP
get_snapshot of the same tree returns pixel-perfect frames and the app is
healthy (tree, layout, stats all normal).

Bisect result (feature-per-position test app, screenshots of the real
desktop): plain rects, nested views, transforms, flex, radius, linear and
radial gradients, and text all render fine on the window path. Adding a
single `repaintBoundary="snapshot"` view kills the WHOLE frame - including
unrelated draws before it in the display list and the stats overlay. Crushy
has two snapshot boundaries (stage backdrop, enemy layer), hence always
black; recurse.tsx has none, hence fine.

Mechanism: `snapshot_node` (alloy/src/rendertree/composite.rs) rasterizes
the subtree via `Context::render_display_list_to_texture` on the UI thread's
ImpellerContext, and the main display list then does `draw_texture_rect`
with that texture on the render thread's *separate* ImpellerContext
(GlSurface). The texture crosses Impeller contexts (and EGL contexts:
UI pbuffer context vs window context, shared group). Desktop GL tolerates
this; under ANGLE the surface draw silently fails/aborts, leaving the
cleared (black) backbuffer to be presented. draw_display_list reports no
error.

The offscreen capture path draws on the UI context where the snapshot
textures were created - same context, which is why captures always look
correct while the window is black.

Fix directions (pick one):
- Rasterize snapshot boundaries on the render thread's context (move the
  cache there), so the texture and the surface draw share one context.
- Share a single ImpellerContext between UI and render threads now that all
  GL is serialized by `context::lock_gl` (check Impeller reactor/current-
  context assumptions under GLES first).
- Detect the broken configuration (ANGLE) and force the existing inline
  fallback in `snapshot_node` (the Err branch already replays the DL
  inline) - correct but loses the cache; snow/backdrop re-raster per frame.

Suspect for the same class: adopted GL textures in general
(`Context::adopt_texture` - image textures, shader/pipeline textures) are
also created on the UI context and sampled by the render context. Shader
textures were disabled during the bisect, so their status on ANGLE is
unverified.

## Bug 2: engine restart (reload) crashes the client in ANGLE

Symptom: on nearly every dev-server bundle push or reload, the client dies
with no stderr output. WER: 0xc0000005 in libGLESv2.dll, repeatedly at
offset 0x2df506 (0.0.30 build; also 0x304875 once, 0x4e88cd on the pre-fix
0.0.29 build, once in nvwgf2umx.dll). Content-independent: happened with
crushy, with a rects-only test app, and with the default app being replaced.
With larger bundles (crushy) it was 14/14 fatal; with tiny test bundles the
client often survived.

Mechanism (likely): the engine loop in lattice/src/lib.rs drops the old
engine (QuickJS runtime, GUI plugin, render tree with PaintCache::Snapshot
ImpellerTextures, TextureEntry finalizers) when a reload arrives. Those
drops issue GL deletes on the UI thread outside `context::lock_gl`, racing
the render thread's composite/present of the last frame - which may also
still reference the dropped textures (use-after-free). The GL serialization
commit covered live Context methods and the render loop, but not teardown
drops.

Fix directions:
- Hold `lock_gl` across engine drop in the lattice loop (and any other
  teardown that releases GL-backed resources).
- Make the render thread drop/stop consuming the old frame (drain + clear
  the channel and present nothing) before the engine teardown starts, so no
  DL referencing dropped textures is ever redrawn.
- To confirm the exact crash site: enable WER LocalDumps for solidrt-go.exe
  and symbolize the ANGLE offset against the pinned ANGLE build.

## Done (2026-07-19): single-context architecture

Both bugs shared one root: the engine violated Impeller's GLES contract
(impellers 0.4.2 docs: "the OpenGL ES context can only be created, used,
and collected on the calling thread"; guidance is one context per app). We
ran TWO ImpellerContexts on two threads over two shared EGL contexts, with
textures crossing between them and teardown deletes racing composite.
Desktop GL tolerated it; ANGLE did not.

Fix: the process now has exactly one GL context and one ImpellerContext,
both owned by the UI thread. `setup_opengl_platform` creates the single
context and releases it from the main thread; the UI thread makes it
current (window surface, `SDL_GL_MakeCurrent` via raw handle) and keeps it
for the engine's lifetime. `Context::submit` wraps FBO 0 at the current
size, draws, and presents (`SDL_GL_SwapWindow`) right on the UI thread -
in playback mode it reads the backbuffer back and ships pixels instead.
The main thread is a pure SDL event/window loop: it receives Presented
notifications for fps/FrameRendered bookkeeping and publishes the physical
framebuffer size through an atomic on resize.

Deleted wholesale: `GL_LOCK`/`lock_gl` and every call site (including the
teardown `Drop` serializers), `GpuFence` + the per-frame fence/flush,
`GlSurface`/`RenderSurface`/`create_render_surface`, the UI pbuffer +
second EGL context, the ANGLE `cross_context_textures_usable` inline
fallback, and the parking_lot dependency. Snapshot caching now works
identically on every backend (the cache texture and the surface draw share
the one context), and adopted/shader textures (`d-texture`, `<image>`) are
covered by construction.

Bug 2 disappears structurally: teardown drops happen on the same thread as
all other GL, so there is nothing to race.

Perf note: present now blocks the UI thread at vsync when the app outpaces
the display - the same stall the GL lock already imposed, minus the lock;
the main thread's event pump no longer stalls behind present at all.

## Open: dev-query timeouts with two clients connected (unexplained)

During verification (2026-07-19, Hyprland/Wayland, dev server 0.0.30) dev
queries against a new-architecture client twice fell into permanent 10s
timeouts. Both episodes occurred while TWO clients were connected to the
dev server; the affected client's window was visible, its UI thread showed
normal animation-level CPU (~10-14%), and nothing was logged. A fresh
single-client session against the same binary was flawless: instant
queries, full-window snapshot, 61fps, 8 consecutive reloads survived.
Prime suspect is therefore dev-server-side query routing with multiple
clients (or across a server-restart generation change), not the client -
but it was not root-caused. Reproduce with two clients before trusting
multi-client query results.

## Done (2026-07-19, second pass): dedicated raster thread

The single-context architecture above fixed correctness but ran GL
submission and the vsync wait inline on the JS thread; on GPU-bound scenes
(recurse.tsx full-window redraw, 4x MSAA, iGPU) input dispatch throttled to
GPU pace. Implemented the Flutter-shaped split within the same contract:

- main thread: SDL event pump, window management, frame bookkeeping.
- srt-ui (JS) thread: QuickJS, layout, hit-testing, DisplayList building -
  zero GL.
- srt-raster thread (alloy/src/raster.rs): owns the process's single GL
  context + ImpellerContext; executes all GL as `RasterCmd`s from one
  ordered mpsc channel; draws + presents with vsync (blocking this thread
  is the point); drops superseded frames in interactive mode (load
  shedding), never in capture mode (playback's contract is exactly one
  Captured per submit).

Calling conventions: fire-and-forget for frames/uploads/param writes/
destroys; blocking RPC (send + wait on a reply channel) for creates,
shader/pipeline compiles (errors must reach JS), rasterize, and readbacks.
The `Context` API surface was preserved verbatim; flux/lattice/rendertree
consumers were untouched. UI-side `Context` keeps handle+dims texture
entries and small mirrors (shader kinds, buffer sizes) for validation;
raster side keeps the GL-name map, shader/buffer maps, window surface
cache, present-failure exit, and slow-frame log.

The one docs-trusted assumption - dropping an impellers::Texture on a
non-context thread defers the GL delete to the context's reactor - was
verified first by examples/xthread_release.rs: 200 cross-thread drop
cycles recycled to a single GL name with no GL errors, and a handle
outliving the ImpellerContext dropped without crash.

The occluded-window JS-starvation concern above is structurally resolved:
submit no longer blocks, so a stalled present parks the raster thread, not
JS. Remaining exposure: a blocking RPC issued while a present is stalled
(occluded window) blocks the JS thread until the swap returns - rare ops
only, same class as the old GL lock. On Hyprland/Mesa no stall was
observed (occluded client kept presenting and answering GPU queries);
other platforms unverified.

## Stage 2 (deferred): non-blocking creates and readbacks

The raster protocol barely changes (blocking recv becomes a reply mailbox
drained from the JS event loop); what changes is the JS-facing calling
convention per op:

- Readbacks/captures: easiest - captureSnapshot is already promise-based
  and capture_requests/capture_ready is already async plumbing.
- CreateTexture: JS knows the dims; a pending registry entry whose
  Impeller handle arrives a frame later, paint skips/defers that texture.
- Shader/pipeline creation: sync-throw becomes promise-rejection - a
  visible JS API semantics change (interacts with the dev/prod validation
  policy); decide deliberately.
- Snapshot repaint boundaries: not an async RPC - move boundary
  rasterization raster-side entirely (ship the boundary DL with the frame;
  raster rasterizes and caches - the Flutter model). Additive protocol
  change.
- GpuResources: dev tooling; stays blocking.

## Repro/tooling notes

- Real-window verification needs a desktop screenshot (PowerShell
  CopyFromScreen via ssh/WSL interop works); MCP get_snapshot renders
  offscreen on the UI context and cannot see either bug.
- WSL interop caveat: killing the WSL-side stub (pkill) does not kill the
  Windows exe; the client can survive as a "ghost" and reconnect state gets
  confusing. Kill via Windows (taskkill) when it matters.
