---
type: bundle-index
title: Backlog
description: Deferred features and ideas, one file per item, picked up here when someone has time.
timestamp: 2026-07-13T00:00:00Z
---

# Backlog

Sorted by status: open first, then partial, deferred, and closed
(decided/promoted/done) at the bottom.

- [Frame pacing - motion on the TV is not fluent](frame-pacing-fluency.md)
  [resolved] - Fixed 2026-08-13 by the FramePacing policy switch (SwapPaced
  for touchless devices, VsyncLocked kept for touch) plus the InputDevices
  touch fact gated on Android's touchscreen feature; 0.00-0.04% drops vs
  ~1.4% baseline on the 50 Hz TV. One flagged oddity remains open and now
  has a visible victim: ~1.1 stray idle Ticks/s during continuous
  animation - see the frame-scheduling section of [Video playback](video-playback.md).
- [Video playback](video-playback.md) [open] - One decode-to-YUV pipeline
  on every platform: software decoders on desktop, MediaCodec buffer mode
  on Android (punch-through rejected), planar YUV textures + shader
  conversion in alloy, player core in forge, no video primitive
  (texture/d-texture display the player's texture id). Fluency target is
  the Philips MT5891 TV; probed 2026-08-12: buffer mode = honest NV12 at
  3x realtime, AImageReader tap unsupported on device (not needed).
  Frame scheduling (timeline clock + standing demand) implemented
  2026-08-13 behind `video-timeline-pacing`, default OFF: missed slots
  11% -> 0.13%, residual ~1/s hitch traced to stray idle ticks (next:
  alloy idle-tick gate).
- [Runtime policies - tracked, app-readable, app-overridable](runtime-policy-registry.md)
  [open] - The registry of behavior policies the runtime selects from
  device facts (frame pacing first: SwapPaced on touchless vs VsyncLocked
  on touch). Policies must be enumerable, readable, and app-overridable
  or they are implicit magic; the surface design is deliberately deferred
  to the backlog-rework session.
- [Content-damage perf watchpoints](content-damage-perf.md) [open] - The
  remaining perf pothole in the GPU-content-damage path, recorded
  symptom-first: the O(nodes) walk in texture_content_changed (matters
  only at ~50k+ nodes with per-frame GPU writes; symptom = jsMs growth on
  GPU-writing frames; fix = texture-referencing node index). The
  boundary-shader-INPUT full re-bake was fixed 2026-08-10 (shader_dirty +
  Compose keeps the bake, only the pass re-runs).
- [texture params prop - one write path](texture-params-prop-write-path.md)
  [done] - The `<texture params>` prop now writes through
  set_target_params at the properties layer (2026-08-10): no tree
  damage, prop-driven shader animation keeps the reuse path, prop and
  imperative writes validate identically, and the kind's
  pending_params/set_params machinery is deleted. A params write with no
  src throws ("set src before params").

- [GPU-only redraws never invalidate snapshot boundaries](snapshot-gpu-content-invalidation.md)
  [done] - A texture id whose pixels changed through GPU writes (draw/shader
  targets, uploadTexture, camera frames) froze inside
  repaintBoundary="snapshot": GPU writes produced no rendertree damage and
  the clean-path present never ran the reuse gate, so the cached bake
  composited forever. Fixed 2026-08-10 by making content changes
  first-class damage: Context notes every content-mutating write's texture
  id (expanded through the mirrored sampler graph, manual targets as
  barriers, buffer writes via retained buffer ids), the frame build drains
  the set before its clean check, and
  RenderTree::texture_content_changed damages exactly the referencing
  nodes under snapshot boundaries - pure-GPU frames without snapshot
  consumers keep the reuse path. Camera frames and id-stable resizes ride
  the same channel.
- [Several dev servers on one machine, each with its own clients and MCP
  route](parallel-dev-servers.md) [open] - Inventory of the pieces: `--port`
  already threads through server, local/Android client and MCP bridge,
  servers already hold many clients, numbered client data trees already
  exist, and the control API already reports `entry`/`projectDir`. Missing
  is identity - concurrent clients all default to `client0/` (sharing
  identity and app data, against two explicit storage decisions), ports are
  picked by hand, and MCP config is static per workspace so the agent cannot
  be pointed at the right server. Scoped 2026-08-08 to one server per project
  folder (a folder with two servers is out of scope), which makes the project
  root the routing key: leading candidate is a `.srt-data/server.json` marker
  the server writes and the bridge finds by walking up from its own cwd, so
  two editor windows on two projects need identical config and no `--port`
  anywhere. Data-folder half still open.
- [Padding makes paint and hit size against different boxes](padding-box-divergence.md)
  [partial] - Paint hands laid-out elements their content box as ctx.size,
  hit passes the border box. The View half (transform center, viewBox fit,
  boundary matrices) is settled and fixed 2026-08-08 - border box on every
  path; the kinds half (a padded rect paints its content box but hit-tests
  its border box) is open, likely a per-kind settlement since text wrap must
  stay content-box.
- [overflow + viewBox clips the wrong rectangle](overflow-viewbox-clip.md)
  [done] - The overflow clip took the box extent as a raw number applied in
  design (pre-viewBox) space, so a magnifying fit escaped the clip and a
  minifying one cropped early; reported three times over four months
  ([[marble-fox]] F1, [[paper-crane]] 1, the unimog postmortem that
  root-caused it) without a dedicated item. Fixed 2026-08-08: the clip is
  emitted in box space before the fit on both paint and hit paths, pinned at
  both scales; scroll settled the same day to box pixels on every path, and
  the record order linearized to matrix, clip, scroll, fit, children.
- [The MCP verification surface - input, clock, crop, props](mcp-verification-surface.md)
  [done] - Five independent external agent-built app reports named the same
  four gaps; 2026-08-06 landed crop+scale on get_snapshot/get_texture
  (runtime-side, shared with texture's crop), set_time_scale/step_frames
  (paused frames still run the draw path natively so captures stay alive),
  frame-stepped virtual time for the WHOLE JS clock surface (timers +
  performance.now; Date.now is the wall escape hatch), props+quad on
  get_render_tree (reader lives next to apply_jsx), and server app identity
  on list_clients. [[mcp-input-injection]] landed the day after; traps in
  the file's implementation note.
- [Generate the docs/core.md props reference from the types](core-docs-generated-props.md)
  [open] - Hand-copied prop lists are how core.md drifted
  (fill/background/imageWidth); jsx-runtime.d.ts + types.d.ts are clean
  enough to generate the per-element props reference from, in a marked
  block, killing that drift class. Prose and the imperative surface stay
  hand-written; TS-5-from-bun-store and JSDoc-comment prep noted in file.
- [3D roadmap - toward Three.js parity](3d-roadmap.md) [open] - The
  scoreboard for @solidrt/3d: v1 landed 2026-08-05 (unlit + shaderMaterial,
  four primitives, orbit camera, all engine prerequisites); the remaining
  work ranked by structural leverage - the uModel/uViewProj split and
  UI-as-texture first, then lights, transparency, glTF and mipmaps, out to
  shadows and PBR - each entry pointing at its engine backlog file.
- [A snapshot boundary's retained texture as a texture id](snapshot-boundary-texture-id.md)
  [open] - repaintBoundary="snapshot" already keeps its subtree's
  rasterization in an adopted texture, but only the boundary shader can
  sample it; vending it as an ordinary TextureId that updates as the
  subtree repaints makes any UI subtree live content for the GPU stack -
  the load-bearing piece of UI mapped onto 3D geometry.
- [Uniform arrays (vecN[], mat4[])](gpu-uniform-arrays.md) [done] - A
  declared array uniform takes one flat `count * components` param under
  its bare name, dispatched through the glUniform*v forms; reflection
  unified on one typed slot (kind + count) for validation and dispatch
  both. Sampler arrays stay unsupported by design.
- [Cube map textures](gpu-cube-maps.md) [open] - No TEXTURE_CUBE_MAP
  support anywhere (upload, sampling, or render target), so skyboxes,
  environment/reflection mapping and cube shadow maps have no path; ES
  3.0 core, demand-gated on the scene-graph environment tier.
- [flux:wasm memory views and named call errors](flux-wasm-memory-access.md) [done] -
  `instance.memory` ArrayBuffer aliases linear memory (detach-on-grow, web
  `Memory.buffer` contract), closing the copy-free gap with the pure-JS
  build; call errors name the target, signature, and argument; guest-internal
  indirect-call traps get a stale-function-pointer hint (wasmi hides the
  index).
- [srt render should be headless, unscaled and able to choose its output folder](render-headless-determinism.md) [done] -
  Playback now runs on SDL's offscreen video driver (hidden window on the
  real display only as fallback), lays out with display scale pinned to 1 so
  `--size` is physical pixels on every machine, and `-o/--output` picks
  where frames land (default: the invoking directory).
- [Apps cannot see their own arguments (lattice never sets ProcessArgs)](app-process-argv.md) [done] -
  resolved 2026-08-05: argv is app arguments only (no exe/script slots),
  owned per app start - a distribution owns its whole command line (fluxrt
  parity), the dev runner passes the tail after the source path, and dev
  pushes carry the session's args so remote clients match local ones; exit()
  now ends a playback run early.
- [Headless render loose ends - a shutdown abort and a blind windowSize()](headless-render-loose-ends.md) [open] -
  Playback aborted once with SIGABRT at shutdown and has not reproduced in 22
  runs (suspect the new exit()-during-playback path, unproven), and
  `windowSize()` reads 0x0 in every headless render because playback's
  synthesised resize never reaches JS - so an app that lays out from it
  captures silently wrong instead of failing.
- [FFI write batching: interned keys, batched creation, command buffer](ffi-write-batching.md) [open] -
  Every property write is one string-keyed FFI call (mount fans a props
  object into per-prop calls; update bursts pay per-call overhead N times);
  three stages - intern prop names to ids, createNode with a props object,
  and a command buffer whose props land in a shared buffer Rust reads
  directly, drained once per flush. Generic before the animation-specific
  finding-b work.
- [Zero-copy texture upload staging](texture-upload-staging.md) [in-progress] -
  The steady state of any texture-driven app is one full-frame copy per frame
  to cross onto the raster thread; begin/endTextureUpload over raster-owned
  staging buffers is the only honestly zero-copy shape - matters when
  video/camera frames arrive. Stages 1+2 (owned-frame YUV upload,
  double-buffered plane sets) done 2026-08-13; TV measurement decides the
  PBO-pool stage.
- [Owner-scoped registerDebug](owner-scoped-register-debug.md) [open] -
  Reload-reset registrations force any state a debug command touches up to
  module scope; an owner-scoped variant auto-cleaned like onFrame lets it
  live in the component.
- [Statically detect layout-in-detached nesting](detached-nesting-static-check.md) [open] -
  A view inside a d-view typechecks and fails at runtime, and the type
  system cannot catch it (one JSX.Element type for every tag); the bundler's
  JSX pass sees static tags and can error on direct nesting, with runtime as
  the backstop behind component boundaries.
- [A zoom debug command in the scaffold](scaffold-zoom-debug-command.md) [done] -
  Retired 2026-08-06: get_snapshot's crop+scale params made magnified
  inspection one tool call with no app-side scaffolding (see
  [[mcp-verification-surface]]); the viewBox trick stays documented as the
  re-rendered-zoom alternative.
- [Diagnostics queue behind the thing they diagnose](diagnostics-off-raster-queue.md) [open] -
  get_gpu_resources queues behind the raster backlog it exists to explain,
  and get_stats/get_snapshot need a JS-thread slice so they time out on a
  busy (healthy) app with a message that says "wedged"; serve inventory and
  stats off published state, and name the real timeout.
- [Fix mDNS discovery (Discover finds nothing)](mdns-discovery.md) [open] -
  The client's `_solidrt._tcp` browse is intact but the dev server stopped
  advertising when it moved into flux (deliberate: the p2p ticket is the
  connect story), so Discover searched forever and the launcher button is
  commented out; restoring it means a forge::mdns responder as a flux
  capability, and whether that works next to Avahi/Bonjour/Windows on port
  5353 needs a spike before anything else.
- [Focus navigation (spatial/D-pad, tab order)](focus-navigation.md) [partial] -
  createFocusNav landed in components (arrows/dpad spatial, Tab reading-order,
  gamepad edges, automatic Modal trapping; Button focusable + ring) and the
  launcher is folded onto it (nav.tsx deleted); remaining: TextInput
  focused-vs-editing states, TV device verification.
- [Per-node event-interest mask for pointer dispatch](pointer-event-interest-mask.md) [open] -
  Rust marshals the full hit path into JS per pointer event because only the
  JS handler registry knows interest; a per-element event-kind bitmask prunes
  delivery to listening nodes and (staged) skips empty emissions, making
  input over handler-free regions free. Stage 1 is dispatch counters; the
  documented end state stores the handlers themselves Rust-side.
- [Relative mouse input (mouse look)](relative-mouse-input.md) [open] - No
  pointer-lock / relative-motion path exists anywhere in the surface, so
  first-person control is impossible however good the GPU gets; SDL already
  has the capability and alloy already discards the deltas.
- [Frame-batched multi-pointer delivery (and frame-paced mouse)](frame-batched-pointer-input.md) [done] -
  all pointer moves (mouse/pen included; pending_moves gate deleted) now
  dispatch from the resampler's frame slots, one per pointer per frame,
  followed by a "pointerFrame" terminator; createTransform measures once per
  frame on it, so the multi-touch span scissor is gone by construction.
- [Move the image codec to forge behind a flux:image module](flux-image-module.md) [done] -
  decodeImage/encodeImage were inline in a lattice-registered global, so
  headless flux had no image codec; now a forge core marshalled by a thin
  flux:image module, core re-exporting like flux:gpu, lattice's image dep
  dropped entirely.
- [AVIF decoding in decodeImage](avif-decode.md) [open] - The one practical
  web image format decodeImage lacks; pure-Rust decode does not exist in the
  image crate, so it needs the dav1d C system dependency.
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
- [Stats overlay should draw after the window shader pass](stats-overlay-post-shader.md) [done] -
  The debug overlay was recorded into the app's display list, so a window
  shader warped the HUD and its refresh forced full rebuilds. Both halves
  fixed and verified 2026-08-09: the texture-driven freeze was a dead
  overlay_due demand source, and the overlay is now retained raster-side,
  rasterized to a small layer and blended over every frame post-pass.
- [parseSvg replaces the svg primitive](parse-svg.md) [done] -
  Removed the `<svg>`/`<d-svg>` element for `parseSvg` (forge core, flux:svg
  module) returning plain draws JS maps to d-path subtrees inside a
  `viewBox`-fitted view; usvg moved alloy -> forge, gradients ride the
  extended absolute-space wire format, per-path hit-testing/animation live.
- [Node captures round-trip through a texture nobody wants](capture-pixels-round-trip.md) [done] -
  captureSnapshot now resolves { width, height, data } directly (2026-08-06,
  breaking); no texture is created and nothing needs destroying. The
  padding-aware capture texture (variant 2) stays unbuilt until a caller
  wants a texture rather than bytes.
- [Refactor the fused creates over the raw shading layer](gpu-fused-create-refactor.md) [partial] -
  The gpu-review naming findings landed 2026-07-31 as a hard rename
  (createShaderTexture/createPipelineTexture/createShaderTextureMemo) and
  iTime was dropped from all preambles - the preamble now declares exactly
  what the runtime fills. Still open: whether the fused paths become thin
  compositions of the raw layer, a mid-level program shorthand (wanted by
  the window effect), and the two-dialect preamble story.
- [Anti-aliasing for GPU pipeline targets](gpu-target-antialiasing.md) [open] -
  createPipeline targets are single-sample, so any filled geometry has hard
  jaggies; wanted a sample count (MSAA + resolve) or a documented supersample
  path with known-good minification.
- [Guard that every referenced example ships](release-example-parity-check.md) [open] -
  A committed examples README can name an example file that is untracked, so
  the doc ships and the file does not; a release-time parity check between the
  README and the packed output would catch it.
- [GPU example gaps](gpu-example-gaps.md) [open] -
  A multi-pass shader chain example; its blocker (target dependency
  propagation) landed 2026-07-29, so it is now simply unwritten. The
  points-topology particle field shipped 2026-07-29 (gpu-particles.tsx) once
  in-draw blending landed.
- [Two-tier handling for declared-but-inactive uniforms](gpu-inactive-uniform-two-tier.md) [open] -
  Uniform validation throws on any name absent from the reflected table,
  but reflection only sees active uniforms, so a declared-but-optimized-out
  uniform counts as a typo; a compile-time declared-name scan would demote
  that sub-case to a warning.
- [Call-site validation for uniforms and draw bounds](gpu-callsite-validation.md) [done] -
  Creates validate in their blocking RPCs, updates against UI-side mirrors:
  unknown/mismatched uniform names, non-sampler rebinds, window-shader
  params, and out-of-bounds draw counts all fail at the call site now;
  strict on inactive uniforms (see the two-tier follow-up).
- [Shader compile errors on .tsx lines via #line injection](glsl-line-injection.md) [open] -
  Compile errors report string-relative lines offset by the injected
  preamble; a bundler pass injecting #line into glsl-tagged literals makes
  the driver report .tsx lines. Mesa calibration probe first (honored at
  all? off-by-one convention?); no platform gate, ignoring drivers degrade
  to today.
- [Buffers held like programs](gpu-buffer-lifetime.md) [done] - The one id
  space with an ordered-destroy rule whose violation silently freezes
  geometry; Rc from targets deletes the rule and the failure mode together.
- [GPU draw targets (multi-pass into one target)](gpu-draw-list.md) [partial] -
  The multi-pass bullet built as a retained ordered draw list:
  createDrawTarget + addDraw/removeDraw with stable DrawIds, per-entry
  setters and ordering verbs (before on addDraw, setDrawOrder); stages
  1+2 implemented 2026-08-04, cross-device verification pending.
- [GPU context loss](gpu-context-loss.md) [partial] - A lost GL context used
  to leave the app running against a dead swapchain; swap-result checking and
  exit after two failed presents shipped, real recreation still open.
- [ANGLE textures and teardown crash](angle-cross-context-impeller-textures.md) [partial] -
  The two Windows client killers, both fixed by the single-context +
  raster-thread architecture; stage 2 non-blocking creates/readbacks and an
  unexplained two-client dev-query timeout remain.
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
- [App-wide zoom](app-wide-zoom.md) [deferred] - Browser-style whole-UI zoom
  (pinch, ctrl+wheel) as a root-level runtime affordance that re-lays out at
  scale instead of magnifying raster output, needing no app cooperation.
- [fontStretch / width axis](font-stretch-axis.md) [deferred] - The bundled Noto
  variables carry a wdth axis the text API cannot reach; whether to expose a
  CSS-style font-stretch, pending an Impeller ParagraphStyle capability check.
- [GPU pipeline extensions](gpu-pipeline-extensions.md) [done] - The record
  of the landed createPipeline extensions: typed uniforms and the additive
  blend/depthWrite toggles 2026-07-29, draw range and instancing (setDraw)
  2026-07-30, multi-pass targets (spun off as gpu-draw-list), index buffers,
  cull mode and per-instance attributes 2026-08-04. Its four remaining opens
  were split into their own items 2026-08-11 (next four entries).
- [Float texture formats (R32F/RGBA32F)](gpu-float-texture-formats.md)
  [open] - Data textures are RGBA8-only, so float payloads sampled in a
  shader (heightfields via texelFetch, bone matrices at scale) need
  fixed-point encode/decode; also the overflow path past uniform-array
  limits.
- [Sampleable depth](gpu-sampleable-depth.md) [open] - A target's depth is a
  private renderbuffer, so shadow maps, depth-of-field and SSAO have no
  path; storage swap is small, the open question is giving depth an id of
  its own to bind in another target's textures.
- [Alpha translucency (sorted blending)](gpu-alpha-translucency.md) [open] -
  Blending within a draw is additive-only; the mode is trivial but needs a
  sorted-geometry story and the straight-vs-premultiplied answer against
  Impeller's compositing (first step: gpu-pixel-contract-docs).
- [Depth func option](gpu-depth-func.md) [deferred] - Depth comparison is
  fixed at LESS; a WebGPU-style depthCompare on createRenderPipeline is
  additive when demand arrives, likely alongside sampleable depth for
  shadow maps.
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
- [MCP input injection](mcp-input-injection.md) [done] - 2026-08-07 landed
  send_input: synthetic pointer/key/wheel/text sequences through the REAL
  input pipeline (the batch-loop channel real SDL input feeds; no
  frame-request latch by design), with per-event delayMs/holdMs so "walk
  forward 500ms" or a typing burst is one call. Composes with clock control
  for deterministic interaction tests; traps (mouse hover persists, tap
  fields before text) in the implementation note. The snapshot-diff
  companion split to [[snapshot-diff-helper]].
- [Snapshot diff helper](snapshot-diff-helper.md) [deferred] - A numeric
  pixel-delta mode on get_snapshot against the previous capture of the same
  node; needs runtime-side raw-RGBA retention (the CLI has no PNG codec),
  so it is its own design, split out of [[mcp-input-injection]].
- [onFrame tick reset on reload](onframe-tick-reset-on-reload.md) [deferred] -
  The tick timebase resets across hot reload after the new instance's first
  frame, handing apps one enormous negative delta; apps clamp dt as a
  workaround.
- [Dev-state KV across reloads](dev-state-across-reloads.md) [deferred] - A
  host-owned per-client store (flux:dev devState) so apps can restore pose and
  UI state after a hot reload instead of resetting to start.
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
- [Release readiness and pre-publish checks](release-readiness-checks.md) [deferred] -
  A pre-build readiness gate (types and runtime in lockstep, srt check, tests,
  version placeholders) plus post-build artifact checks before the
  irreversible npm publish.
- [MCP improvements and expansion](mcp-agent-loop-improvements.md) [deferred] -
  Round-2 agent dev-loop feedback: readOnlyHint annotations, call_debug
  broadcast, form-factor fields in list_clients, interaction-performance
  visibility, leak diagnostics; plus drawn bounds for detached nodes in
  get_render_tree (d-* nodes report the inherited box, useless for locating
  them; local_bounds already exists).
- [Cross-platform GPU usage attribution](gpu-usage-attribution.md) [deferred] -
  Answer "is the client burning GPU while idle, and on what" portably: engine
  self-measurement in get_stats plus a per-OS story for whole-system
  attribution.
- [APK packaging for flux:ffi libraries](ffi-android-apk-packaging.md) [deferred] -
  Ship an app's ffi libraries in an asset folder, packaged into the APK's
  native-lib dir and opened by path automatically, since byte-loading is
  blocked by Android W^X policy.
- [Reload does not drain the raster queue](reload-drain-raster-queue.md) [deferred] -
  A backed-up raster channel survives `load`/`reload`, so a wedged client
  cannot be recovered from the dev loop and the natural instinct (edit, reload)
  is exactly what does not help; defensive now that the runaway is fixed.
- [Android client forgets its dev-server address](android-dev-server-persistence.md) [deferred] -
  The address only arrives as a launch-intent extra, so any relaunch outside
  the CLI (the device's own launcher, a crash, a reboot) starts into
  `apps/default` with no way back without adb - the common case on a TV.
- [Adaptive present-fence depth](adaptive-present-fence-depth.md) [deferred] -
  Fallback if unconditional two-deep present fencing (shipped 2026-07-27)
  ever shows up as desktop drag latency: grant the second in-flight frame
  only while observed fence waits show the GPU is over budget, with
  hysteresis; the gating signals (fenceTimeouts, per-fence wait) already
  exist.
- [Present-fence pacing on ANGLE](angle-present-fence-pacing.md) [deferred] -
  ANGLE/D3D11's glClientWaitSync never blocks (measured 2026-08-04, probe
  example in alloy), so depth-capped pacing degrades to check-and-proceed
  there; the false fenceTimeouts counter is fixed (glFlush after fence
  creation), a GetSynciv-spin fallback waits for evidence of real Windows
  drag latency, and macOS (ANGLE-Metal) is unmeasured.
- [Move the fetch disk cache out of forge?](fetch-cache-out-of-forge.md) [deferred] -
  Lattice is now the only cache configurer, so should the mechanism follow the
  policy out of forge, and which of the three candidate shapes pays for
  itself?
- [Deep links](deep-links-url-open.md) [deferred] - Opening the app at a URL
  from outside: an OS registration half (scheme declaration in srt pack and
  the Android manifest) and an app half that is just onOpenUrl.
- [Scoped style defaults and variant selection](scoped-style-defaults.md) [deferred] -
  The two real gaps behind "something like stylesheets": no scoped text
  defaults and no state/variant selection, both constrained to stay
  per-element property writes.
- [More pipeline blend modes](gpu-pipeline-blend-modes.md) [deferred] - The
  createPipeline blend vocabulary stops at "none"/"add"; multiply, screen,
  subtract, min/max are each a two-line addition, and alpha-over waits on
  sorting plus premultiplied-vs-straight semantics.
- [Mipmaps](gpu-mipmaps.md) [deferred] - Minified textures alias by axiom
  today; mipmap?: boolean on the sampler state, generateMipmap under it,
  auto-regeneration for render targets off the dirty flush.
- [Compressed texture uploads (ETC2)](gpu-compressed-textures.md) [deferred] -
  ES 3.0 mandates ETC2 in core (4-8x texture memory), uploadTexture is
  RGBA8-only; demand-gated with the ANGLE-may-software-expand caveat
  recorded.
- [Per-binding sampler override](gpu-per-binding-sampler.md) [deferred] -
  filter/wrap fused into the texture id is the right default but leaves no
  escape hatch (a nearest atlas cannot be blurred, a clamped target cannot be
  tiled); an id-or-object binding value is cheap because the sampler cache is
  already keyed by state.
- [Async shader compile and readback](gpu-async-compile-readback.md) [deferred] -
  The two calls whose cost class differs from the rest of the surface, both
  blocking the frame loop; invisible while compiles happen at startup, real
  for live-coding, and the async precedent (captureSnapshot) already exists.
- [Line vs path - the segment primitive](line-layout-endpoints.md)
  [decided 2026-08-02] - Line stays as the primitive whose geometry is
  numbers (animatable endpoints, dash) against path's DSL string; a laid-out
  line is a rule (thin box), no mirror prop, and the recorded growth
  direction is caps/arrowhead markers when a design asks. Documented in
  LineProps and core AGENTS.md.
- [GPU target purity and an explicit render verb](gpu-purity-decision.md)
  [decided 2026-07-30] - Option 2: the purity invariant is documented and
  render: "manual" targets stepped by renderTarget(id) are the one
  imperative escape hatch; implemented in okf/plans/gpu-render-verb.md,
  which also carries the gated follow-ups (loadOp, copyTexture).
- [Press util; end the Pressable exception](components-press-util.md) [promoted] -
  Press semantics extracted from Pressable into a components-package util;
  widened to gesture recognizers and promoted to
  okf/plans/component-gestures.md, this file is a pointer.
- [flux:audio live voice control](flux-audio-voice-control.md) [done] - A
  playing voice was stop-only; shipped 2026-08-03 as Playback/Clip handles:
  per-voice setGain/setPan (equal-power, [-1,1], also play options), an
  ended() poll, and loadPcm where the typed array is the format (u8/s16/f32
  interleaved). Positional audio for game ports closed.
- [Detached-view transform origin pivots around the inherited box](detached-view-transform-origin.md) [done] -
  A d-view's unset transform origin pivoted around the centre of the
  inherited layout box, so the same code landed elsewhere on a differently
  sized window; fixed 2026-08-03 - the unset origin on a detached view now
  defaults to its local (0,0), the anchor every other detached construct
  uses (drawn-bounds-centre rejected: new machinery plus a pivot that
  drifts with animating content). Explicit origins unchanged.
- [R8 / indexed uploadTexture format](texture-upload-r8-format.md) [done] -
  Palette-indexed content had to pack four indices per RGBA texel, free
  only when the width divides by four; shipped 2026-08-03 as
  `format: "r8"` on createTexture/createMutableTexture - 1 byte/pixel,
  alignment-free at any width, format is id state sizing every later
  upload/resize. Measured 2.45x on a whole game tick in the port that
  asked.
- [App-registered debug commands via MCP](mcp-debug-commands.md) [done] - The
  srt:dev registerDebug plus MCP list_debug/call_debug, replacing the
  debug-keys and get_logs pattern for poking a running app; async commands
  still unsupported.
- [GPU resource inspection via MCP](mcp-gpu-resource-inspection.md) [done] -
  MCP readback of textures as PNG, buffer ranges and pipeline state, because a
  one-pipeline app hides everything from the render tree; depth attachments
  still deferred.
- [Client build info in list_clients](client-build-info.md) [done] - Git hash,
  version and profile per connected client in list_clients, so "does this
  binary have my engine fix" is checkable; build timestamp and HEAD staleness
  still deferred.
- [Engine-side HTTP disk cache](engine-http-cache.md) [done] - Explicit opt-in
  disk cache in the forge fetch layer, needed by a production app doing many
  image fetches; designed and shipped as okf/plans/fetch-cache.md.
- [Portals cannot mount at initial render](portal-initial-mount.md) [done] - A
  portal visible at first mount throws "no mount target" because windowRoot is
  set only after the initial build; decided as by design, documented with a
  clearer error.
- [Node/memory leak on unmount](unmount-node-leak.md) [done] - Element-valued
  props built a native subtree on every read, so typeof probes orphaned
  unmounted builds forever; fixed by resolving once through children(), with
  orphan stats.
- [Idle tick runs away when the raster thread falls behind](idle-tick-gpu-backlog-runaway.md) [done] -
  The idle-tick gate read `pending_presents == 0` as "GPU idle", but it was
  equally true when the raster thread was too far behind to have returned a
  frame; on a slow GPU that closed a positive feedback loop and frame time
  diverged without bound (measured 900 JS ticks per presented frame). Fixed
  via a raster queue-depth gate, per-shader params load-shedding and two-deep
  present fencing, all TV-verified (unbounded doubling -> 120 ms flat, ticks
  1:1 with presents); adjacent findings split into their own items below.
- [Time the GPU pass work](gpu-pass-timing.md) [done] - Shader and pipeline
  passes ran in the raster command loop where nothing was timed, so a client at
  50 s per frame reported a healthy `draw 40.3ms`; landed 2026-07-30 as
  `gpuPasses`/`gpuPassMs` and `rasterCmdMs` in get_stats plus per-target
  `passes`/`passMs` in get_gpu_resources; runtime-verified 2026-07-31 across
  five clients, exactly 1:1 with frames, and they measured the TV's per-pass
  cost at 0.5-0.7 ms against 0.10 ms on Linux.
- [The documented perf model is desktop-shaped](device-perf-model-docs.md) [done] -
  "GPU work is nearly free" holds on desktop and on a mid-range 2020 tablet but
  is wrong by ~8x on TV-class hardware, where the compositor can set the frame
  budget outright; the scaffold AGENTS.md now carries the device spread,
  primitive count as a real budget, compositor-bound as a recognisable
  condition, and how to measure your own numbers.
- [Android surface swap blocks four vsyncs](android-surface-swap-latency.md) [done] -
  SOLVED: the ~80 ms "swap block" on the 2017 MediaTek TV was our own
  unconditional 4x MSAA draining off-tile resolve traffic every frame. Fixed
  via a multisampled Android window backbuffer (in-tile resolve at swap) plus
  the rig's EXT_multisampled_render_to_texture path, whose resolve-out must
  be a sampling draw, never a blit (Adreno rejects the blit
  content-dependently). TV at 50 fps / 4x MSAA / 0.1 percent drops; the file
  keeps the full investigation record, traps, and per-device measurement
  rules (SF --latency only; screenrecord and the engine fps stat lie).
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
- [Snapshot boundaries reallocate their whole offscreen rig per raster](snapshot-offscreen-rig-churn.md) [done] -
  A content change dropped the retained texture outright, so every re-raster
  rebuilt texture, MSAA renderbuffers, two FBOs and a wrapped surface (~133 MB
  at 1440p); fixed 2026-07-27 via okf/plans/snapshot-offscreen-rig-churn.md
  (retain and re-render in place, shared grown rig, "snapshot-no-aa" opt-out).
- [Paint properties on the texture element](texture-element-compositing.md) [done] -
  texture/d-texture carried no PaintProps, so two GPU layers could not be
  composited additively in the tree; fixed by giving the texture kind the same
  PaintState every other kind has, after verifying what a paint actually does
  to a raster draw.
- [captureSnapshot on detached (d-*) nodes](capture-detached-nodes.md) [done] -
  A d-* node is drawn but has no layout entry, so every capture of one rejected
  as zero-sized; captures are now sized from the node's painted box
  (local_bounds over the same ctx.size the paint path uses), verified by
  alloy/examples/capture_detached.rs.
- [Sampler filter and wrap state](gpu-sampler-state.md) [done] - filter/wrap
  options on every create path, per-texture-id state applied via GL sampler
  objects (shader passes) and per-draw Impeller sampling (display); wrap
  default unified to clamp everywhere, repeat now explicit.
- [Target verb unification](gpu-target-verb-unification.md) [done] -
  Landed 2026-08-06, all three stages: setTargetParams/Textures/Size are
  the target-level verbs on every kind (setShader* retired), the
  `<texture params>` prop drives draw targets' shared params, the lifetime
  opt-out is `autoFree: false` (no more `manual` collision), and the
  program-state leak is documented publicly. Internal sentinel cleanups
  assessed and deliberately skipped - rationale in the file.
- [Dependency propagation between GPU targets](gpu-target-dependency-propagation.md) [done] -
  Target rendering is now pull-based: writes mark dirty, and a flush at each
  observation point (frame, capture, readback) re-renders the affected
  subgraph in dependency order; sampling cycles are rejected at bind time.
- [Shared (target-level) params for draw targets](gpu-shared-draw-params.md)
  [done] - Landed 2026-08-06: a draw target holds shared params every entry
  reads, applied before the entry's own (entry overrides shared), written
  once via setTargetParams or seeded by createDrawTarget's positional
  params; validation is coverage-based (at least one declaring pipeline,
  partial coverage tolerated). @solidrt/3d swapped uMVP for per-mesh uModel
  + shared uViewProj, making camera motion one write instead of O(meshes).
  Shared sampler bindings (setTargetTextures + opts.textures seed) landed
  the same day with identical rules, pixel-asserted in the draw_list
  example; program-sorted ordering stays deferred.
- [Split GPU pipeline state from the render target](gpu-pipeline-object-model.md) [done] -
  Landed 2026-07-30: RenderPipeline (program + typed draw state) with
  createRenderPipeline/createShaderTarget split, one owned spec instead of
  the 2x2, invalid depth states unrepresentable, vocabulary parsed at the JS
  boundary, and headless assertion examples covering blend, typed uniforms,
  and the full JS surface.
- [Branded GPU id types](gpu-branded-ids.md) [done] - Every GPU handle is a
  plain number across five id spaces, so destroyBuffer(textureId)
  typechecks and usually hits a valid id in the wrong space; branded types
  in flux-types close the class with no runtime cost.
- [Document the GPU pixel contract](gpu-pixel-contract-docs.md) [done] -
  Clip-space y points down, targets are premultiplied, values are
  non-linear RGBA8: three facts previously discoverable only the hard way,
  now one named "pixel contract" in gui/gpu.d.ts, core gpu.ts, docs/core.md
  and scaffold AGENTS.md. Docs only, landed 2026-07-30.
- [GPU file reorganization](gpu-file-reorg.md) [done] - Split shader.rs
  (1466 lines, six concerns) into an alloy gpu/ folder, rename flux
  plugins/gui/texture.rs to gpu.rs, lift the RasterCmd enum, capture path
  and context DTOs.
- [GPU object labels and device limits](gpu-labels-limits.md) [done] - Landed
  2026-07-31: a label on every create, surfaced in get_gpu_resources and in
  raster-side error strings, plus GpuLimits queried at raster startup and
  checked at every create/bind/resize site with the limit named, so an
  oversize target fails as "exceeds this device's limit" instead of
  framebuffer-incomplete hex. Runtime-verified 2026-07-31 on five clients,
  each reporting its own ceilings (one device caps at 16383, which is why
  the check queries rather than assumes).
