---
title: Build and run the SDL move into ore on macOS and Windows
description: The move of SDL out of alloy into ore (the window, the GL surface and its loss recovery, the pump, the devices) was verified on Linux, the tablet and the TV only; macOS and Windows never built or ran after it, so the bundled GL library load, the recreated window after a context loss and the macOS swap pacer are unconfirmed there.
created: 2026-11-13
---

# Build and run the SDL move into ore on macOS and Windows

## Problem

okf/done/ore-pump-and-window.md moved every SDL call out of alloy into ore.
Steps 1 to 4 were verified on Linux (Wayland and X11 through the dev
server), the tablet and the TV. macOS and Windows never built or ran after
the move. The code that only those two platforms reach is therefore
unconfirmed:

- the GL library load from the bundled ANGLE libraries, through the
  `lattice/src/gl_libs.rs` hints (`ore::gl_surface::set_library_path`);
- the context-loss recovery that recreates the window
  (`RECOVERY_RECREATES_WINDOW` is true on both, `Window::recreate_gl` in
  ore does the rebuild);
- on macOS, the CVDisplayLink swap pacer, which moved into the GL surface
  (`ore/src/gl_surface/display_link.rs`).

## Done looks like

- A CI build on macOS and on Windows passes.
- One run on each machine:
  - the window opens through the bundled GL libraries;
  - a forced context loss recovers with a recreated window;
  - on macOS the swap pacer still paces;
  - the pacing counters (refresh misses, cadence) read as before the move.

## Involves

CI (`.github/workflows/ci.yml`), one macOS and one Windows machine, and the
Checks section of okf/done/ore-pump-and-window.md for the run list.
