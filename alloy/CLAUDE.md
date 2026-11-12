Alloy is the window's scene and its rendering: Impeller for 2D paint, behind
the renderer seam in `src/renderer/` (the one module that names the
`impellers` crate, see okf/done/renderer-seam.md); glow as the single
trusted GL path; the rendertree - Alloy's own scene graph with taffy-based
layout (cached), hit testing and pointer routing, damage tracking, and
compositing; and frame timing. It still drives SDL3 for the
window, input and the audio and camera devices; that moves to ore, the OS
layer, as features touch it. Target is cross-platform (Linux, Android, Windows, macOS, iOS), OpenGL ES 3.0 minimum.
GLES is the single backend by design, translated per platform by ANGLE where
no native GL exists; native Vulkan/Metal backends are deliberately out (see
okf/research/graphics-backend-strategy.md before proposing one).

What belongs here is the scene, its rendering and frame timing
(`FrameRelease`, cadence, refresh counting, liveness). Alloy is
facts-based: it reports facts and carries out what the embedder chooses.
Policy is lattice's: the pacing mode, the cadence-hold policy and its
defaults, any choice an embedder could make differently. Anything that reaches
the OS or SDL belongs in ore, even when it serves the window: new SDL calls,
new `sdl_utils.rs` wrappers and new `cfg(target_os)` code go there, not here
(the root CLAUDE.md, Projects, has the rule; okf/design/platform-layering.md
the design). The SDL code alloy still holds moves to ore when a feature
touches it.

# Threads and frame loop
Three threads: main pumps SDL events and does frame bookkeeping, sol-ui runs
JS/layout/paint and builds display lists (zero GL), sol-raster owns the
process's single GL context + the renderer and executes every GL command
(see raster.rs). Non-negotiable decisions:
- All GL lives on sol-raster; that is the renderer's GLES contract (one
  context, used only on its creating thread). Context methods marshal into
  RasterCmds over one ordered channel: fire-and-forget for frames/uploads/
  param writes, blocking RPC for creates/compiles/readbacks. Never add GL or
  renderer use anywhere else.
- The main loop blocks on the SDL event queue plus a FrameReady push from the
  raster thread. The old 8ms poll cap was removed; do not reintroduce
  polling. Send-then-wake ordering matters.
- Rendering is demand-driven (frame requests), not free-running.
- Superseded frames drop in interactive mode (load shedding under GPU-bound
  presents). Capture/playback mode draws every frame: exactly one Captured
  per submit is playback's lockstep contract.
- Vsync (swap interval 1) is set on sol-raster; blocking that thread in the
  swap is the point - sol-ui stays free for input and the next frame.

# Coordinate spaces (SDL3)
Mouse events and SafeArea are in logical points, not pixels. Never divide by
display_scale (breaks HiDPI). Only size_in_pixels is physical.

# Rendertree
The rendertree must stay engine-independent: no JavaScript or scripting
engine references. Methods take and return native Rust types only.

# Tests
Unit tests live in the single `src/tests/` folder (`src/tests/<module>.rs`),
never inline in source files.

A lib test reads back what the paint walk drew: in a test build a display
list returns its ops through `DisplayList::ops()`, and
`renderer::oplog::placed` places each one in device pixels (test builds
only, no cost to a shipped build). A claim about a shape's ops (its box, its
stroke width, its paint) goes there; pixels and texture draws stay with the
GL examples.
