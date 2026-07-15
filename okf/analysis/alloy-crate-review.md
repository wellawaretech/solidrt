---
type: analysis
title: Alloy crate review - completeness, quality, tests
timestamp: 2026-07-15T00:00:00Z
---

# Alloy crate review - completeness, quality, tests

Full-crate review of `alloy` (~9k lines, 40 files) as of 2026-07-15: every
module read, unit + integration tests run (26 tests, all pass).

## Summary

Alloy is in very good shape for what it is: a young, GL-only platform layer
with unusually mature driver-workaround knowledge and excellent internal
documentation. It is not yet "production level" in the sense of the stated
ambition (Unity/Unreal-scope, multi-backend): it has exactly one backend,
panics as its API contract at the tree boundary, three broad
`unsafe impl Send/Sync` assertions the compiler can no longer check, and test
coverage that touches maybe 15% of the interesting logic. None of these are
wrong for the current stage, but they are the gap.

## Completeness

Measured against `alloy/CLAUDE.md` (OpenGL first, Vulkan next, Metal last):
the GL path is complete and hardened; Vulkan and Metal are
`panic!`/`unimplemented!` stubs (`context.rs`, `backend.rs`, `gl.rs`). Per
plan, but the stubs are `panic!` rather than the `Err(...)` style used
elsewhere, so a future backend enum typo becomes a crash rather than an error.

The two-thread architecture (UI thread builds display lists, main thread
presents) is fully realized, including the subtle parts: per-frame GPU fences
replacing `glFinish` (`context.rs:200`, `gl.rs:445`), frame coalescing with
fence release for superseded frames (`app.rs:188`), demand-driven idle ticks,
and the wake-after-send ordering. The rendertree has layout (taffy
flex/block/grid), repaint boundaries with a sophisticated hoisting model
(transform/clip/scroll hoisted out of caches so transform and scroll writes
replay cached content), damage classification, hit testing with
pointer-events inheritance, captures, and 10 element kinds. Peripherals
(camera with QR scan and rotation handling, microphone, audio incl.
streaming, gamepad with unmapped-joystick fallback, playback capture) are all
functional.

Known incomplete spots, all documented in-code:

- `bounding_box` composes translations only; rotate/scale/perspective
  ancestors give wrong coordinates (`tree.rs:255`, acknowledged TODO).
- Oval hit-testing is commented out, so ovals hit as rectangles
  (`hit.rs:57`); rounded `clip_radius` corners are also not honored by the
  hit path (paint clips them, hits don't).
- Perspective hit-testing is a documented z=0 approximation (`view.rs:200`).
- Text is single-style: spans concatenate into one string; no per-span
  styling yet.
- Open `TODO` on power-status polling policy (`app.rs:317`); currently emits
  an event every 10s even when nothing changed.

## Code quality

Strengths: comment discipline is the best in the repo - nearly every
non-obvious decision carries its constraint (GL name ownership after Impeller
adoption to prevent double-free, tile-alignment on Android, MSAA fallbacks at
both window and FBO level, Impeller state leakage after captures with an
exhaustive save/restore list in `shader.rs:625`, SDL safe-area unit
differences per platform). Raw `.unwrap()` is essentially absent (5
occurrences, 4 of them EGL proc-address lookups). Error handling at the
JS-facing surface is consistently `Result<_, String>`.

Issues, ranked:

1. **Unsafe `Send`/`Sync` assertions are the main structural risk.**
   `Context` (`context.rs:126`), `PlatformContext` (`platform.rs:30`) and
   `RenderTree` (`tree.rs:20`) are asserted thread-safe while being full of
   `RefCell`/`Cell` and thread-bound GL state. The safety argument ("only
   touched from the UI thread") is currently true but enforced by nothing;
   `Context` sits behind an `Arc`, so any future code can move it across
   threads and the compiler will allow UB. Cheap mitigation: record the
   owning `ThreadId` at construction and `debug_assert_eq!` it in the
   handful of entry points (submit, texture ops, pump), or wrap the
   thread-bound interior in a `ThreadBound<T>` newtype.
2. **Hot-path `expect(&format!(...))`.** `RenderTree::node`/`node_mut`
   (`tree.rs:225`, `tree.rs:331`) build the panic-message String eagerly on
   every call, including successes; these run for every node on every paint
   walk and hit test. Should be `unwrap_or_else(|| panic!(...))` (clippy's
   `expect_fun_call`).
3. **Panics as the tree API contract.** `create_node` (duplicate id),
   `insert_node` (attached-under-detached), `Element::from_kind` (unknown
   kind) all panic. Defensible as asserts against the JS renderer, but one
   malformed FFI call takes down the whole runtime. Acceptable while flux is
   the only (validating) caller; must become `Result`s if the rendertree is
   ever driven by anything less trusted.
4. **Residual hand-rolled GL FFI.** The Cargo.toml comment says glow replaced
   the transmute FFI, but `playback.rs` still transmutes
   `glReadPixels`/`glBindFramebuffer` by hand even though
   `create_gl_context()` would give it glow for free. The EGL transmutes in
   `gl.rs:12` are harder to avoid (glow does not cover EGL), but the
   playback ones are a straightforward cleanup.
5. **Window raw pointer in `DisplayContext`.** `window_opaque: *const c_void`
   (`backend.rs:35`) points at a `Window` that is subsequently moved into
   and out of `App`. The single deref happens while still valid, and
   `GlSurface::create` ignores the argument anyway, so it works - but it is
   a use-after-move trap for the next refactor. The parameter is unused, so
   the pointer could just be dropped.
6. **Minor duplication:** span-to-text aggregation exists twice (eager in
   `tree.rs:347`, per-pass in `layout/context.rs:97`); the
   `prev_texture`/`prev_framebuffer` helper sets are duplicated between
   gl.rs and shader.rs. `decode_qr` (`barcode.rs`) clones the full luma
   frame every scan attempt because rxing wants an owned Vec - a per-scan
   width*height allocation on the UI thread that a reusable buffer or
   ownership handoff would avoid.
7. **Layering nit:** the logger in `logging.rs` special-cases `flux` and
   `lattice` targets, so the bottom crate knows its consumers' names.
   Harmless, slightly backwards.

## Tests

What exists passes and is well-chosen: tree structural invariants (the
detach/destroy lifecycle, anchor insertion, the attached-under-detached
panic), view transform origins, gradient fallback derivation, SVG parsing
including currentColor and gradients, a QR tolerance pin with
camera-realistic fixtures (explicitly there to prevent silent decoder
regression), and an SDL IOStream FFI round-trip.

But 26 tests for this crate is thin, and the most subtle logic in the crate
has zero coverage, despite being pure and GPU-free:

- **Damage semantics** (`tree.rs:179`): the Transform/Scroll/Paint/Layout
  matrix of what invalidates what, including the Recording-vs-Snapshot
  scroll distinction. This is the correctness core of the repaint-boundary
  system; its bugs manifest as "stale pixels sometimes", the worst kind to
  debug live.
- **Hit testing** (`hit.rs`): pointer-events inheritance/cascade, the
  `None`-with-hittable-descendant path-removal logic, overflow gating,
  scroll compensation, and `path_diff`. All pure functions over an in-memory
  RenderTree.
- **`upright_into`** (`camera.rs:336`): four rotation cases of raw index
  arithmetic, trivially property-testable (rotate 90 four times = identity).
- `compute_bounding_box` ascent (scroll/translate composition,
  positioning-context stop), `ScriptPlayer::due` ordering, wheel/touch event
  translation.

If one test file gets added, make it damage + invalidation; if two, add hit
testing.

## Prioritized improvement list

1. Thread-affinity debug assertions (or `ThreadBound`) behind the three
   `unsafe impl Send/Sync` (soundness).
2. Unit tests for `apply_damage`/`invalidate_paint` and
   `hit_recursive`/`path_diff` (correctness insurance for the trickiest
   code).
3. Fix `expect(&format!)` in `RenderTree::node`/`node_mut` (hot-path perf,
   one-line change).
4. Port playback.rs readback to glow; delete the unused `window_opaque`
   pointer.
5. `upright_into` and bounding-box tests; reuse a luma buffer in the QR scan
   path.
6. Later, when the surface stabilizes: a real error enum at the crate
   boundary instead of `String`, and `Result`-ify the Vulkan/Metal stubs so
   backend growth doesn't inherit `panic!` arms.
