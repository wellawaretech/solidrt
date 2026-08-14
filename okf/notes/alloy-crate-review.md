---
title: Alloy crate review
description: GL-through-ANGLE is now the single backend by design and the crate has doubled with the gpu/raster subsystems; the old top test asks (damage, hit/routing) are covered by a 168-test suite. Remaining gaps are the unenforced unsafe Send/Sync (now four types), hot-path expect(&format!), and panics at the tree boundary.
created: 2026-07-15
---

# Alloy crate review

Full-crate review of `alloy` as of 2026-07-15 (then ~9k lines, 40 files, 26
tests). Re-evaluated 2026-08-14: every claim below was checked against the
current tree and the test suite re-run, and the `gpu/` and `raster/`
subsystems added since July (~5.6k lines) were read line-by-line (findings in
their own section below), so the whole crate is again at one review depth.
The crate is now ~22k lines across 75 files.

## Summary

Alloy remains in very good shape, and the two loudest complaints of the July
review have been answered: the test suite grew from 26 to 169 (all passing),
with damage/invalidation and hit testing/routing - the two asks - now covered,
and the "one backend of a planned three" framing is obsolete because GL through
ANGLE became the single backend by design (see
okf/research/graphics-backend-strategy.md); the Vulkan/Metal panic stubs are
deleted. What carries over unchanged: three (now four) broad
`unsafe impl Send/Sync` assertions with no enforcement, the hot-path
`expect(&format!(...))` in the node accessors, and panics as the tree API
contract.

## Completeness

The backend question is settled rather than incomplete: `backend.rs` was
rewritten around a dedicated raster thread that owns the process's single GL
context and receives work as raster commands; the main thread only does frame
bookkeeping and playback encoding. Frame building went through the FrameDriver
refactor (rendertree/frame.rs) with a display-list cache so texture-content
frames (e.g. camera uploads) present without a rebuild. Since July the crate
also grew a `gpu/` subsystem (buffers, programs, pipelines, targets, passes,
vocab parsing, limits - the JS-facing GPU API), a 16-slot layout cache,
resize-liveness settle/rebind, layout-activity counters, and packaged-font
loading. The rendertree kinds were split into `kinds/` modules and tree
mutation moved to the `edit`/`try_edit` model where the closure returns its own
damage.

Known incomplete spots, updated:

- ~~`bounding_box` composes translations only~~ Fixed: `compute_bounding_box`
  now composes every transform on the ancestor chain (translate, rotate,
  scale, 3D), as the forward companion of the hit descent.
- ~~Oval hit-testing commented out~~ Fixed 2026-08-14: the `Oval` dispatch
  arm in hit.rs is wired to the ellipse test (with a stroke-hole case in
  `Oval::is_in_bounds`), pinned by `oval_hits_as_ellipse_not_box`.
  Rectangle hit-testing likewise now honors per-corner radius and the
  stroke/fill draw style - the whole July hit-shape gap is closed.
- Perspective hit-testing still inverts onto the local z=0 content plane
  (kinds/view.rs); still documented, still the intended model.
- Text is still single-style: spans concatenate into one string
  (`sync_span_parent`); no per-span styling yet.
- Open `TODO` on power-status polling policy (app.rs:604); unchanged.

## Code quality

Strengths hold: comment discipline stayed at the same level through the new
modules (the raster/backend and gpu headers carry their constraints the same
way the driver workarounds do). Raw `.unwrap()` outside tests is down to one
occurrence. Error handling at the JS-facing surface is still consistently
`Result<_, String>`, including the new gpu vocab parsers.

Issues, ranked:

1. **Unsafe `Send`/`Sync` assertions - still the main structural risk, and
   it grew.** `Context` (context.rs:219), `PlatformContext`
   (rendertree/platform.rs:53), `RenderTree` (rendertree/tree.rs:22) and now
   `InputState` (input.rs:21) are all asserted thread-safe while being full
   of `RefCell`/`Cell`. The safety argument ("only touched from the UI
   thread") is still enforced by nothing. The cheap mitigation from July
   stands: record the owning `ThreadId` at construction and
   `debug_assert_eq!` it in the entry points, or a `ThreadBound<T>` newtype.
2. **Hot-path `expect(&format!(...))` - unchanged.** `RenderTree::node`/
   `node_mut` (rendertree/tree.rs:267, tree.rs:442) still build the panic
   String eagerly on every call; these run for every node on every paint walk
   and hit test. `unwrap_or_else(|| panic!(...))` is the one-line fix.
3. **Panics as the tree API contract - unchanged.** `create_node` (duplicate
   id), `insert_node` (attached-under-detached), `Element::from_kind`
   (unknown kind) all still panic. Still defensible while flux is the only
   (validating) caller; must become `Result`s if anything less trusted ever
   drives the tree.
4. **Duplication.** `prev_texture`/`prev_framebuffer` restore helpers are now
   duplicated between gl.rs and gpu/mod.rs (the shader.rs copy moved rather
   than merged). `decode_qr` still clones the full luma frame per scan
   attempt (`gray.to_vec()`, barcode.rs:18) because rxing wants an owned
   Vec - the camera rotation path got a reusable scratch buffer, the QR path
   did not. The span-to-text aggregation duplication is resolved in spirit:
   it now carries a comment explaining why both sites exist (detached text
   never enters layout).
5. **Layering nit - unchanged.** logging.rs:6 still special-cases its
   consumers by name (now an alloy/flux/lattice whitelist).

## The gpu/ and raster/ subsystems (reviewed 2026-08-14)

Line-by-line read of all twelve files (gpu/: vocab, program, buffer, limits,
spec, resources, pass, target, mod; raster/: mod, cmd, capture). Verdict:
this code meets the crate's bar and in places exceeds it. No new findings of
the severity of the Send/Sync or expect(&format!) issues above.

What holds it together:

- **The single-writer GL invariant is real.** Every GL call sits behind the
  raster thread; the UI side works on plain-data mirrors (uniform tables,
  buffer sizes, draw bounds) seeded by the create RPCs. Validation logic
  lives once, in `vocab`, and is called from both sides - UI-side for
  call-site errors, raster-side as warn-and-skip backstops on the
  fire-and-forget path - so the two cannot drift apart in what they accept.
- **Draw-range bounds checking closes a real UB hole**: raw GLES 3.0 has no
  draw-time bounds check, so an out-of-range fetch is undefined behaviour.
  `resolve_draw_range`/`validate_draw_range` bound every vertex, index, and
  instance fetch against mirrored buffer sizes before a command crosses the
  channel, with the WebGL-style caveat (index VALUES are not checked)
  documented. The stride-0 division hazard is excluded by construction
  (attributeless entries carry no fetch bound).
- **The Impeller-shared-context state discipline is exhaustive and
  explained.** `run_pass` saves/neutralizes/restores every piece of
  fixed-function state a pass could inherit or leak (including the
  non-obvious ones: rasterizer discard, sample coverage, Impeller's 0.0
  depth-clear value), each with the failure mode it prevents.
- **The dirty-flush propagation is pure and tested.** `propagation_order`
  (dependency-ordered re-render of target chains, cycle members returned
  separately so a diverged mirror degrades to stale pixels instead of a
  hang) is GL-free and covered by 22 tests; the validators have 20 more.

Minor observations, none blocking:

- `std::process::exit(1)` on confirmed GPU context loss runs in the bottom
  crate - a process-policy decision below the layer that usually owns
  policy. Deliberate and documented (okf/backlog/gpu-context-loss.md), but
  worth remembering if an embedder ever needs to survive context loss.
- The raw SDL window pointer in `RasterState` is the one cross-thread raw
  pointer; unlike the old `window_opaque` it carries an explicit lifetime
  contract (the Window lives on the main thread for the whole run).
- `prev_texture`/`prev_framebuffer` duplication (issue 4 above) is between
  gl.rs and gpu/mod.rs.
- Pass execution and target lifecycle have no tests - they need a live GL
  context. The pure halves (graph, validators, vocab parsing) are exactly
  the tested halves, which is the right line given the harness available.

Fixed since July: the hand-rolled `glReadPixels` transmutes in playback.rs are
gone (the remaining transmutes in gl.rs are the EGL/multisample entry points
glow does not expose), and the `window_opaque` use-after-move trap was deleted
with the backend rewrite (the raster `DisplayContext` documents its raw window
pointer's lifetime instead).

## Tests

169 tests (166 unit across 18 files in src/tests/, 3 integration), all pass.
The July priorities were delivered:

- **Damage semantics**: tests/tree.rs (44 tests) covers `invalidate_paint`,
  cache survival across the Recording/Snapshot distinction, GPU content
  writes as snapshot damage, and the detach/destroy lifecycle.
- **Hit testing and routing**: tests/hit.rs (14 tests) covers local-point
  composition, overflow gating and scroll compensation under viewBox
  transforms; tests/router.rs covers hover-diff ordering (deepest-first
  leaves), capture freezing routing until pointer-up, touch leave synthesis,
  and wheel/up gating.
- The gpu subsystem shipped with validation-error and graph tests
  (tests/gpu_validate.rs, tests/gpu_graph.rs), plus suites for layout cache,
  frame driver, paint, present, liveness, fonts, audio, resample, yuv, and
  keymap.

Still zero-coverage and still cheap to test: `upright_into` (camera.rs:433,
four rotation cases, property-testable as rotate-90-four-times = identity) and
`ScriptPlayer::due` ordering.

## Prioritized improvement list

1. Thread-affinity debug assertions (or `ThreadBound`) behind the four
   `unsafe impl Send/Sync` (soundness; unchanged from July, one more type).
2. Fix `expect(&format!)` in `RenderTree::node`/`node_mut` (hot-path perf,
   one-line change; unchanged from July).
3. Reuse a luma buffer in the QR scan path; `upright_into` and
   `ScriptPlayer::due` tests.
4. Later, when the surface stabilizes: a real error enum at the crate
   boundary instead of `String`, and `Result`-ify the panicking tree
   constructors if a less-trusted caller ever appears.

Done from the July list: damage and hit-testing unit tests (delivered well
beyond the ask), playback readback ported off hand-rolled FFI, and the
`window_opaque` pointer deleted.
