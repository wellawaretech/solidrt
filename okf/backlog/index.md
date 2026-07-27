---
type: bundle-index
title: Backlog
description: Deferred features and ideas, one file per item, picked up here when someone has time.
timestamp: 2026-07-13T00:00:00Z
---

# Backlog

- [Press util; end the Pressable exception](components-press-util.md) [promoted] -
  Press semantics extracted from Pressable into a components-package util;
  widened to gesture recognizers and promoted to
  okf/plans/component-gestures.md, this file is a pointer.
- [App-wide zoom](app-wide-zoom.md) [deferred] - Browser-style whole-UI zoom
  (pinch, ctrl+wheel) as a root-level runtime affordance that re-lays out at
  scale instead of magnifying raster output, needing no app cooperation.
- [fontStretch / width axis](font-stretch-axis.md) [deferred] - The bundled Noto
  variables carry a wdth axis the text API cannot reach; whether to expose a
  CSS-style font-stretch, pending an Impeller ParagraphStyle capability check.
- [AVIF decoding in decodeImage](avif-decode.md) [open] - The one practical
  web image format decodeImage lacks; pure-Rust decode does not exist in the
  image crate, so it needs the dav1d C system dependency.
- [GPU pipeline extensions](gpu-pipeline-extensions.md) [deferred] - Typed
  (vec, mat4) uniforms, index buffers, float data textures, blending and
  multi-pass targets on top of the minimal createPipeline.
- [stdin/tty support in flux](stdin-tty-support.md) [deferred] - A flux:stdin
  (or flux:tty) module for cross-platform raw-mode keystroke reading, the
  missing piece for any interactive terminal UI under flux, not just the CLI
  repl.
- [Move the srt CLI fully into flux](cli-flux-migration.md) [deferred] -
  Collapse the repl/dev-server split into one flux process so there is exactly
  one rebuild-and-push path, leaving Bun only as a bundler subprocess.
- [Runtime-side sourcemap remapping](runtime-sourcemap-remap.md) [deferred] -
  Remap stack frames in the runtime itself so the local terminal and logcat
  show tsx positions too; explicitly not to be done unless server-only
  remapping proves insufficient.
- [Node snapshots need a frame to happen](snapshot-requires-next-render.md) [deferred] -
  captureSnapshot and get_snapshot latch a frame request but do not wake the
  render loop, so a truly idle client never services the capture and the query
  times out.
- [MCP input injection](mcp-input-injection.md) [deferred] - Synthetic key and
  pointer events to clients, plus a snapshot-diff helper, so an agent can
  navigate and verify visuals without a human ferrying the app around.
- [App-registered debug commands via MCP](mcp-debug-commands.md) [done] - The
  srt:dev registerDebug plus MCP list_debug/call_debug, replacing the
  debug-keys and get_logs pattern for poking a running app; async commands
  still unsupported.
- [GPU resource inspection via MCP](mcp-gpu-resource-inspection.md) [done] -
  MCP readback of textures as PNG, buffer ranges and pipeline state, because a
  one-pipeline app hides everything from the render tree; depth attachments
  still deferred.
- [onFrame tick reset on reload](onframe-tick-reset-on-reload.md) [deferred] -
  The tick timebase resets across hot reload after the new instance's first
  frame, handing apps one enormous negative delta; apps clamp dt as a
  workaround.
- [Dev-state KV across reloads](dev-state-across-reloads.md) [deferred] - A
  host-owned per-client store (flux:dev devState) so apps can restore pose and
  UI state after a hot reload instead of resetting to start.
- [Client build info in list_clients](client-build-info.md) [done] - Git hash,
  version and profile per connected client in list_clients, so "does this
  binary have my engine fix" is checkable; build timestamp and HEAD staleness
  still deferred.
- [Engine-side HTTP disk cache](engine-http-cache.md) [done] - Explicit opt-in
  disk cache in the forge fetch layer, needed by a production app doing many
  image fetches; designed and shipped as okf/plans/fetch-cache.md.
- [Home for cross-crate constants](shared-config-constants.md) [deferred] -
  One defined home for cross-crate constants that today live as per-site
  literals (.srt-data, http-cache.db, the SolidRT/go identity, size caps);
  collects sites until designed.
- [Dev/prod signal for validation](dev-prod-validation-policy.md) [deferred] -
  The missing runtime signal and shared helper behind the agreed convention of
  throwing in dev and warning in prod; today everything is dev, so validation
  sites throw.
- [Production diagnostics surface](production-diagnostics-surface.md) [deferred] -
  Layout counters are latched into Stats but only dev-client queries read
  them; wanted a production consumer so field bug reports carry the numbers.
- [Portals cannot mount at initial render](portal-initial-mount.md) [done] - A
  portal visible at first mount throws "no mount target" because windowRoot is
  set only after the initial build; decided as by design, documented with a
  clearer error.
- [Release readiness and pre-publish checks](release-readiness-checks.md) [deferred] -
  A pre-build readiness gate (types and runtime in lockstep, srt check, tests,
  version placeholders) plus post-build artifact checks before the
  irreversible npm publish.
- [MCP improvements and expansion](mcp-agent-loop-improvements.md) [deferred] -
  Round-2 agent dev-loop feedback: readOnlyHint annotations, call_debug
  broadcast, form-factor fields in list_clients, interaction-performance
  visibility, leak diagnostics.
- [Node/memory leak on unmount](unmount-node-leak.md) [done] - Element-valued
  props built a native subtree on every read, so typeof probes orphaned
  unmounted builds forever; fixed by resolving once through children(), with
  orphan stats.
- [Cross-platform GPU usage attribution](gpu-usage-attribution.md) [deferred] -
  Answer "is the client burning GPU while idle, and on what" portably: engine
  self-measurement in get_stats plus a per-OS story for whole-system
  attribution.
- [APK packaging for flux:ffi libraries](ffi-android-apk-packaging.md) [deferred] -
  Ship an app's ffi libraries in an asset folder, packaged into the APK's
  native-lib dir and opened by path automatically, since byte-loading is
  blocked by Android W^X policy.
- [Shader effects on a subtree](subtree-effects.md) [open] - A snapshot
  boundary already rasterizes a subtree into a texture, so running a shader
  over it and compositing the result is one extra pass, region-sized rather
  than window-sized; it can only ever see the subtree's own pixels, which is
  what separates it from a backdrop effect.
- [Backdrop filters through Impeller (blur, glass)](impeller-backdrop-filters.md) [open] -
  `save_layer` already takes a backdrop `ImageFilter` and we already call it
  with `None`, so Impeller's built-in blur/dilate/erode/matrix filters give
  frosted panels with correct see-through semantics, needing neither GLSL,
  impellerc, nor the root layer.
- [Idle tick runs away when the raster thread falls behind](idle-tick-gpu-backlog-runaway.md) [partial] -
  The idle-tick gate read `pending_presents == 0` as "GPU idle", but it is
  equally true when the raster thread is too far behind to have returned a
  frame; on a slow GPU that closed a positive feedback loop and frame time
  diverged without bound (measured 900 JS ticks per presented frame). Fixed
  via a raster queue-depth gate plus per-shader params load-shedding, and
  `get_stats` now reports `rasterQueue`/`idleTicks`; verified on the TV
  (unbounded doubling -> 120 ms flat, ticks now 1:1 with presents), the
  adjacent findings (fence honesty, reload queue drain, pass timing) open.
- [Adaptive present-fence depth](adaptive-present-fence-depth.md) [deferred] -
  Fallback if unconditional two-deep present fencing (shipped 2026-07-27)
  ever shows up as desktop drag latency: grant the second in-flight frame
  only while observed fence waits show the GPU is over budget, with
  hysteresis; the gating signals (fenceTimeouts, per-fence wait) already
  exist.
- [GPU context loss](gpu-context-loss.md) [partial] - A lost GL context used
  to leave the app running against a dead swapchain; swap-result checking and
  exit after two failed presents shipped, real recreation still open.
- [ANGLE textures and teardown crash](angle-cross-context-impeller-textures.md) [partial] -
  The two Windows client killers, both fixed by the single-context +
  raster-thread architecture; stage 2 non-blocking creates/readbacks and an
  unexplained two-client dev-query timeout remain.
- [In-place GPU resize](gpu-in-place-resize.md) [done] - Resize data textures
  and shader targets at a stable id so texture references, sampler bindings
  and owner-scoped auto-free survive; shipped, no GL-level test coverage.
- [Frame-safe texture destruction](gpu-deferred-texture-destroy.md) [done] -
  destroyTexture used to land before the reactive texture swap flushed; the
  runtime now defers reclamation until the live render tree no longer
  references the id.
- [Reactive GPU resource lifetimes](gpu-reactive-resource-helpers.md) [done] -
  Core's gpu helpers only freed on owner disposal, wrong for resources rebuilt
  on signal changes; shipped as a manual option plus createShaderMemo at a
  stable id.
- [setShaderTextures - rebind sampler inputs](gpu-sampler-rebinding.md) [done] -
  setShaderTextures rebinds sampler2D inputs on a live shader, enabling
  retargeting and ping-pong without recompiling; shipped ahead of a real use
  case.
- [flux:net socket gaps](flux-net-socket-gaps.md) [done] - Three flux:net gaps
  surfaced by the linux VM's NAT gateway: Udp.close, TCP half-close, and raw
  ICMP; closed by one cancellation token per socket.
- [Move the fetch disk cache out of forge?](fetch-cache-out-of-forge.md) [deferred] -
  Lattice is now the only cache configurer, so should the mechanism follow the
  policy out of forge, and which of the three candidate shapes pays for
  itself?
- [Deep links](deep-links-url-open.md) [deferred] - Opening the app at a URL
  from outside: an OS registration half (scheme declaration in srt pack and
  the Android manifest) and an app half that is just onOpenUrl.
- [App icons](app-icons.md) [partial] - Stages 1+2 done (SVG icon from
  package.json/convention through the manifest to the launcher, monogram
  fallback; dev-client window icon via go-gated resvg + SDL_SetWindowIcon);
  stage 3 packed executables remain and own packed-app icons on all platforms.
- [Root layer - render the app into a texture effects can read](root-layer-effects.md) [partial] -
  Invert the frame so the app draws into the offscreen MSAA rig and resolves
  into a sampleable layer texture composited to a single-sample window, giving
  whole-app effects for about the cost of one quad; stages 1-3 implemented
  and verified 2026-07-27 (the inversion, the raw shading layer, the `shader`
  prop on `<window>`, and `previous`/uPrevious frame history; the hold flag
  was built, verified and dropped as premature), stage 4 (clean-tree raster
  skip) pending, plan okf/plans/root-layer-effects.md.
- [Stats overlay should draw after the window shader pass](stats-overlay-post-shader.md) [open] -
  The debug overlay is recorded into the app's display list, so a window
  shader warps the HUD too, and its once-per-second refresh forces full
  rebuilds that defeat clean-tree fast paths; draw it post-pass into FBO 0
  instead (mind the stage-1 orientation rules).
- [Snapshot boundaries reallocate their whole offscreen rig per raster](snapshot-offscreen-rig-churn.md) [done 2026-07-27] -
  A content change drops the retained texture outright, so every re-raster
  rebuilt texture, MSAA renderbuffers, two FBOs and a wrapped surface (~133 MB
  at 1440p); fixed via okf/plans/snapshot-offscreen-rig-churn.md (retain and
  re-render in place, shared grown rig, "snapshot-no-aa" opt-out).
- [Node captures round-trip through a texture nobody wants](capture-pixels-round-trip.md) [open] -
  Every capture rasterizes, reads back, uploads a texture and is then read back
  again, because both consumers only ever wanted the pixels; a pixels-returning
  variant halves the sync points.
- [Refactor createShader/createPipeline over the raw shading layer](gpu-fused-create-refactor.md) [open] -
  The fused conveniences predate compileShader/linkProgram/createShaderTarget
  and still compile+link internally with conditional preamble sniffing; decide
  whether they become thin compositions of the raw layer, and whether a
  mid-level program shorthand (wanted by the window effect) is added.
- [Anti-aliasing for GPU pipeline targets](gpu-target-antialiasing.md) [open] -
  createPipeline targets are single-sample, so any filled geometry has hard
  jaggies; wanted a sample count (MSAA + resolve) or a documented supersample
  path with known-good minification.
- [Paint properties on the texture element](texture-element-compositing.md) [done] -
  texture/d-texture carried no PaintProps, so two GPU layers could not be
  composited additively in the tree; fixed by giving the texture kind the same
  PaintState every other kind has, after verifying what a paint actually does
  to a raster draw.
- [Scoped style defaults and variant selection](scoped-style-defaults.md) [deferred] -
  The two real gaps behind "something like stylesheets": no scoped text
  defaults and no state/variant selection, both constrained to stay
  per-element property writes.
