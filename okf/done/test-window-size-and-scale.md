---
title: An app test chooses its window size and display scale
description: Every app test ran at the one surface the dev client was started with (1280x720, scale 1), so a responsive layout, a breakpoint or a display-scale dependent paint could not be tested; test(name, fn, { size, scale }) now mounts at that window, built on a headless window that is a framebuffer the raster thread owns rather than the EGL surface, so the size is an allocation and not a platform question.
created: 2026-09-30
completed: 2026-10-19
---

# An app test chooses its window size and display scale

Symptom (2026-09-30): `sol test` started the dev client headless at one
size, and every test of every file ran there. A layout that switches at a
breakpoint, a phone-sized screen, a 2x display scale: none could be
asserted.

Built 2026-10-19: `test(name, fn, { size: [width, height], scale })` beside
`fps` (okf/done/test-harness.md, step 4.3, where this was split off). A
test mounted at `{ size: [390, 844], scale: 3 }` reads a 390 wide window
box from the tree, `windowSize()` and `displayScale()` read 390 x 844 and
3, its pixels and its failure snapshot are 1170 wide, and the test after
it is back at the run's window. Tests in `packages/test/tests/app.test.tsx`;
the option is documented in `sol test` (packages/cli/src/test/docs.md),
the testing guide and `@solidrt/test`'s notes.

## The decision: the headless window is a framebuffer the raster thread owns

The item as shaped offered two shapes, neither verified:

- Rebuild the EGL surface between engines. Read in full on 2026-10-19:
  SDL's offscreen video driver, the headless backend on Linux and in CI,
  cannot do it - its `SetWindowSize` only sends the event and the pbuffer
  keeps its creation size (SDL 3.4.14, `SDL_offscreenwindow.c`). The
  surface could only be rebuilt by owning it outright (alloy's own EGL
  display selection in place of the offscreen driver, a pbuffer recreated
  on the raster thread, the hidden-window fallback resized and verified):
  platform work on three backends, and a change of the CI display path.
- One surface at the largest size a run needs, a test's size as a
  sub-rectangle. The sizes are known only as each test runs, so "largest"
  is a fixed cap: a limit on tests and memory on every run. Rejected.

What decided it was the question the OpenXR item asks from the other side
(okf/backlog/vr-openxr.md: an XR frame goes to a swapchain image, never to
FBO 0): headless, nothing needs the EGL surface to carry the picture. A
headless frame's destination is a readback, never a display, and the
surface's one job is to keep a context current. So the window framebuffer
became a parameter of the raster thread - FBO 0 for a window, an owned
color target for the headless sink (`RasterState::window_framebuffer`,
alloy/src/raster/compose.rs) - and the size became an allocation:

- The target is allocated at the published surface size and reallocated
  when a frame or a readback finds the size changed. Color only, like the
  window shader's layer: every frame reaches it as a rig resolve or a pass,
  never as a direct Impeller draw, so the rig carries depth and stencil.
- Every reader of the window moved with it: the window draw's resolve, the
  window shader's final pass, the overlay composite, the window readback
  and the window taps that recording uses take the window framebuffer.
- The in-tile fast path is gated on the target, not only on FBO 0's
  sample count: an owned target always goes through the rig. On the
  hidden-window fallback that surface is multisampled, and without the
  gate a frame would have landed in the real window at the wrong size.
- Orientation is unchanged: Impeller treats every wrapped FBO as a
  bottom-up window target and the resolve blit is straight, so the owned
  target holds rows exactly as FBO 0 does and the readback's flip stands.
- The resize contract is the interactive one. Alloy's stepped loop owns
  the headless window (`stepped::HeadlessWindow`: logical size and scale);
  `AlloyCommand::SetHeadlessWindow` publishes `size * scale` pixels through
  the same atomic a window resize does and tells the app the resize with
  that scale, and `EmitInitEvents` restores the run's window first, so
  every engine starts at the default. No second channel, no fence beyond
  the ordered raster channel.
- The three headless backends (SDL's offscreen driver, alloy's EGL pbuffer,
  the hidden window) are untouched; their surface's size and content no
  longer matter after setup. The EGL surface is still created at the run's
  size and its pixels go unused headless: a few megabytes, noted where the
  surface is made (alloy/src/egl_headless.rs). A surfaceless context there
  is the OpenXR-side cleanup, since that work shares the context creation.

In lattice, `sol:test` gained `setWindow(width, height, scale)`, allowed
before the first frame like `setFrameRate` (the window is a fact of the
engine's start). It validates positive integer sides, a positive scale,
whole-pixel products and the GPU's largest render target, sends the
command, and marks the engine's window state as waiting for the resize of
exactly that size and scale (`stepped::WindowReady::expect`), so `mount`
builds its first frame at the right size whichever order the run's own
resize and the call land in. The render host is untouched.

Verified: renders of `examples/hello-world` (1280x720 and 480x640) and
`examples/spin` byte-identical before and after; `bun run sol test
--only app` at the repo root; alloy and lattice unit tests.
