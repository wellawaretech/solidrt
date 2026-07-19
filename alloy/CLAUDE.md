Alloy is the platform/rendering layer: SDL3 + Impeller, with glow (GL/ANGLE)
as the single trusted GPU path. Target is cross-platform (Linux, Android,
Windows, macOS, iOS), OpenGL ES 3.0 minimum. Backend priority: OpenGL first,
Vulkan next, Metal last (Vulkan/Metal are stubs today).

# Threads and frame loop
Three threads: main pumps SDL events and does frame bookkeeping, srt-ui runs
JS/layout/paint and builds display lists (zero GL), srt-raster owns the
process's single GL context + Impeller context and executes every GL command
(see raster.rs). Non-negotiable decisions:
- All GL lives on srt-raster; that is Impeller's GLES contract (one context,
  used only on its creating thread). Context methods marshal into RasterCmds
  over one ordered channel: fire-and-forget for frames/uploads/param writes,
  blocking RPC for creates/compiles/readbacks. Never add GL or Impeller
  context use anywhere else.
- The main loop blocks on the SDL event queue plus a FrameReady push from the
  raster thread. The old 8ms poll cap was removed; do not reintroduce
  polling. Send-then-wake ordering matters.
- Rendering is demand-driven (frame requests), not free-running.
- Superseded frames drop in interactive mode (load shedding under GPU-bound
  presents). Capture/playback mode draws every frame: exactly one Captured
  per submit is playback's lockstep contract.
- Vsync (swap interval 1) is set on srt-raster; blocking that thread in the
  swap is the point - srt-ui stays free for input and the next frame.

# Coordinate spaces (SDL3)
Mouse events and SafeArea are in logical points, not pixels. Never divide by
display_scale (breaks HiDPI). Only size_in_pixels is physical.

# Rendertree
The rendertree must stay engine-independent: no JavaScript or scripting
engine references. Methods take and return native Rust types only.

# Tests
Unit tests live in the single `src/tests/` folder (`src/tests/<module>.rs`),
never inline in source files.
