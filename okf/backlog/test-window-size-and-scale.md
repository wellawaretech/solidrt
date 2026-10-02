---
title: An app test cannot choose its window size or display scale
description: Every app test runs at the one surface the dev client was started with (1280x720, scale 1), so a responsive layout, a breakpoint or a display-scale dependent paint cannot be tested; let a test name its size and scale (test(name, fn, { size, scale })), which needs the headless surface rebuilt or sub-sized between engines on all three headless backends.
created: 2026-09-30
---

# An app test cannot choose its window size or display scale

Symptom: `sol test` starts the dev client headless at one size, and every
test of every file runs there. A layout that switches at a breakpoint, a
phone-sized screen, a 2x display scale: none can be asserted.

Wanted: `test(name, fn, { size: [width, height], scale })`, beside `fps`
(okf/done/test-harness.md, step 4.3, where this was split off). With an
engine per test (D30) the option is natural: the size is a fact of the
engine's start, like the frame rate.

Why it is not a small change (read 2026-09-30):

- The headless surface is created once, in `alloy::setup`, at the size the
  process was started with. There are three backends behind it: SDL's
  offscreen driver, the EGL pbuffer built without SDL's video subsystem
  (`alloy/src/egl_headless.rs`, where the GL stack has no device
  enumeration), and a hidden window as the last fallback. The size lives
  in `DisplayContext`'s `surface_size`, which the raster thread reads for
  every frame.
- The init events a headless engine gets (`playback_init_events` in
  `alloy/src/event.rs`) are built from the window: its pixel size as the
  logical size, and a display scale of 1.

Two shapes, neither verified:

- Rebuild the surface between engines: a command to the stepped loop that
  resizes the offscreen window or recreates the pbuffer, fenced against
  the raster thread, then the resize event. Correct for every size; three
  backends to get right.
- One surface at the largest size a run needs, and a test's size as a
  sub-rectangle of it: `surface_size` set per engine, nothing recreated.
  Cheaper; whether every consumer of the surface size (the window fast
  path, the offscreen rigs, node captures) holds under a size smaller than
  the real surface is unchecked.

A test's options are known only once its engine runs the file, which is
after the engine was built; the frame rate solves the same ordering with
a call that is only allowed before the first frame (`setFrameRate`).

Done looks like: a test mounted at `{ size: [390, 844], scale: 3 }` reads
a 390 wide window box from the tree, its snapshot is 1170 pixels wide, and
the test after it is back at the default.
