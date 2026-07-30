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
  uniforms and the additive blend/depthWrite toggles landed 2026-07-29; still
  deferred: index buffers (shape decided), draw range/instancing, float data
  textures, cull/depth-func raster state, alpha translucency, multi-pass
  targets (gated on the gpu-review purity decision).
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
- [Idle tick runs away when the raster thread falls behind](idle-tick-gpu-backlog-runaway.md) [done] -
  The idle-tick gate read `pending_presents == 0` as "GPU idle", but it was
  equally true when the raster thread was too far behind to have returned a
  frame; on a slow GPU that closed a positive feedback loop and frame time
  diverged without bound (measured 900 JS ticks per presented frame). Fixed
  via a raster queue-depth gate, per-shader params load-shedding and two-deep
  present fencing, all TV-verified (unbounded doubling -> 120 ms flat, ticks
  1:1 with presents); adjacent findings split into their own items below.
- [Time the GPU pass work](gpu-pass-timing.md) [open] - Shader and pipeline
  passes run in the raster command loop where nothing is timed, so a client at
  50 s per frame reported a healthy `draw 40.3ms`; per-pass duration and count
  is the last counter the GPU investigation lacked, and a pass count alone
  would have caught that bug (~900 renders per presented frame).
- [The documented perf model is desktop-shaped](device-perf-model-docs.md) [done] -
  "GPU work is nearly free" holds on desktop and on a mid-range 2020 tablet but
  is wrong by ~8x on TV-class hardware, where the compositor can set the frame
  budget outright; the scaffold AGENTS.md now carries the device spread,
  primitive count as a real budget, compositor-bound as a recognisable
  condition, and how to measure your own numbers.
- [Reload does not drain the raster queue](reload-drain-raster-queue.md) [deferred] -
  A backed-up raster channel survives `load`/`reload`, so a wedged client
  cannot be recovered from the dev loop and the natural instinct (edit, reload)
  is exactly what does not help; defensive now that the runaway is fixed.
- [Diagnostics queue behind the thing they diagnose](diagnostics-off-raster-queue.md) [deferred] -
  `get_gpu_resources` is a raster command, so it times out precisely when the
  client is wedged and blames the JS thread, which was running fine; serve the
  inventory off published state, or at least name the real timeout.
- [Android client forgets its dev-server address](android-dev-server-persistence.md) [deferred] -
  The address only arrives as a launch-intent extra, so any relaunch outside
  the CLI (the device's own launcher, a crash, a reboot) starts into
  `apps/default` with no way back without adb - the common case on a TV.
- [Android surface swap blocks four vsyncs](android-surface-swap-latency.md) [done] -
  SOLVED: the ~80 ms "swap block" on the 2017 MediaTek TV was our own
  unconditional 4x MSAA draining off-tile resolve traffic every frame. Fixed
  via a multisampled Android window backbuffer (in-tile resolve at swap) plus
  the rig's EXT_multisampled_render_to_texture path, whose resolve-out must
  be a sampling draw, never a blit (Adreno rejects the blit
  content-dependently). TV at 50 fps / 4x MSAA / 0.1 percent drops; the file
  keeps the full investigation record, traps, and per-device measurement
  rules (SF --latency only; screenrecord and the engine fps stat lie).
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
  whether they become thin compositions of the raw layer, whether a
  mid-level program shorthand (wanted by the window effect) is added, and the
  gpu-review naming findings (createShader/createPipeline return textures;
  iTime declared but never filled).
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
- [captureSnapshot on detached (d-*) nodes](capture-detached-nodes.md) [done] -
  A d-* node is drawn but has no layout entry, so every capture of one rejected
  as zero-sized; captures are now sized from the node's painted box
  (local_bounds over the same ctx.size the paint path uses), verified by
  alloy/examples/capture_detached.rs.
- [Sampler filter and wrap state](gpu-sampler-state.md) [done] - filter/wrap
  options on every create path, per-texture-id state applied via GL sampler
  objects (shader passes) and per-draw Impeller sampling (display); wrap
  default unified to clamp everywhere, repeat now explicit.
- [Dependency propagation between GPU targets](gpu-target-dependency-propagation.md) [done] -
  Target rendering is now pull-based: writes mark dirty, and a flush at each
  observation point (frame, capture, readback) re-renders the affected
  subgraph in dependency order; sampling cycles are rejected at bind time.
- [Guard that every referenced example ships](release-example-parity-check.md) [open] -
  A committed examples README can name an example file that is untracked, so
  the doc ships and the file does not; a release-time parity check between the
  README and the packed output would catch it.
- [GPU example gaps](gpu-example-gaps.md) [open] -
  A multi-pass shader chain example; its blocker (target dependency
  propagation) landed 2026-07-29, so it is now simply unwritten. The
  points-topology particle field shipped 2026-07-29 (gpu-particles.tsx) once
  in-draw blending landed.
- [More pipeline blend modes](gpu-pipeline-blend-modes.md) [deferred] - The
  createPipeline blend vocabulary stops at "none"/"add"; multiply, screen,
  subtract, min/max are each a two-line addition, and alpha-over waits on
  sorting plus premultiplied-vs-straight semantics.
- [Split GPU pipeline state from the render target](gpu-pipeline-object-model.md) [done] -
  Landed 2026-07-30: RenderPipeline (program + typed draw state) with
  createRenderPipeline/createShaderTarget split, one owned spec instead of
  the 2x2, invalid depth states unrepresentable, vocabulary parsed at the JS
  boundary, and headless assertion examples covering blend, typed uniforms,
  and the full JS surface.
- [GPU target purity and an explicit render verb](gpu-purity-decision.md) [open] -
  The retained target model silently relies on every pass being pure, and
  the whole accumulation/feedback/multi-pass/transform-feedback class breaks
  that; decide pure-vs-escape-hatch (recommended: manual targets + one
  renderTarget verb) before building any of them.
- [Branded GPU id types](gpu-branded-ids.md) [done] - Every GPU handle is a
  plain number across five id spaces, so destroyBuffer(textureId)
  typechecks and usually hits a valid id in the wrong space; branded types
  in flux-types close the class with no runtime cost.
- [Call-site validation for uniforms and draw bounds](gpu-callsite-validation.md) [open] -
  Param typos drop silently at render and arity mismatches warn where no
  app can see; a draw count past the buffer end is undefined-behaviour
  vertex fetch; both checkable synchronously from state the UI thread
  already mirrors.
- [GPU object labels and device limits](gpu-labels-limits.md) [open] -
  label? on every create surfaced in get_gpu_resources and error strings,
  plus a queryable gpu.limits with named-limit errors at create instead of
  framebuffer-incomplete hex.
- [Mipmaps](gpu-mipmaps.md) [deferred] - Minified textures alias by axiom
  today; mipmap?: boolean on the sampler state, generateMipmap under it,
  auto-regeneration for render targets off the dirty flush.
- [Buffers held like programs](gpu-buffer-lifetime.md) [open] - The one id
  space with an ordered-destroy rule whose violation silently freezes
  geometry; Rc from targets deletes the rule and the failure mode together.
- [Document the GPU pixel contract](gpu-pixel-contract-docs.md) [done] -
  Clip-space y points down, targets are premultiplied, values are
  non-linear RGBA8: three facts previously discoverable only the hard way,
  now one named "pixel contract" in gui/gpu.d.ts, core gpu.ts, docs/core.md
  and scaffold AGENTS.md. Docs only, landed 2026-07-30.
- [Compressed texture uploads (ETC2)](gpu-compressed-textures.md) [deferred] -
  ES 3.0 mandates ETC2 in core (4-8x texture memory), uploadTexture is
  RGBA8-only; demand-gated with the ANGLE-may-software-expand caveat
  recorded.
- [GPU file reorganization](gpu-file-reorg.md) [done] - Split shader.rs
  (1466 lines, six concerns) into an alloy gpu/ folder, rename flux
  plugins/gui/texture.rs to gpu.rs, lift the RasterCmd enum, capture path
  and context DTOs.
