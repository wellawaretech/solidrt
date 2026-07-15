Alloy is the platform/rendering layer: SDL3 + Impeller, with glow (GL/ANGLE)
as the single trusted GPU path. Target is cross-platform (Linux, Android,
Windows, macOS, iOS), OpenGL ES 3.0 minimum. Backend priority: OpenGL first,
Vulkan next, Metal last (Vulkan/Metal are stubs today).

# Threads and frame loop
Two threads: the UI thread builds display lists, the main/render thread
presents them. Non-negotiable decisions in this loop:
- The main loop blocks on the SDL event queue plus a FrameReady push from
  Context::submit. The old 8ms poll cap was removed; do not reintroduce
  polling. Send-then-wake ordering matters.
- Rendering is demand-driven (frame requests), not free-running.
- The render `time` value is present-count * refresh period, not wall clock.
  Do not "fix" it to wall clock.
- UI-to-render texture sync is a per-frame GL fence in Context::submit
  (glWaitSync), not glFinish.

# Coordinate spaces (SDL3)
Mouse events and SafeArea are in logical points, not pixels. Never divide by
display_scale (breaks HiDPI). Only size_in_pixels is physical.

# Rendertree
The rendertree must stay engine-independent: no JavaScript or scripting
engine references. Methods take and return native Rust types only.

# Tests
Unit tests live in the single `src/tests/` folder (`src/tests/<module>.rs`),
never inline in source files.
