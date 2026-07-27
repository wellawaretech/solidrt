---
type: backlog-item
title: Snapshot boundaries reallocate their whole offscreen rig per raster
description: A content change drops the retained texture outright, so every re-raster rebuilds texture, MSAA renderbuffers, two FBOs and a wrapped surface (~133 MB at 1440p); retain the storage and pool the rig instead.
status: done
timestamp: 2026-07-27T00:00:00Z
---

# Snapshot boundaries reallocate their whole offscreen rig per raster

Every time a `repaintBoundary: "snapshot"` subtree is rasterized, the
offscreen path builds its entire GL rig from nothing and tears it down again:

- `render_display_list_to_texture` (alloy/src/gl.rs) creates the resolve
  target texture, sized up to the next 64px multiple.
- `draw_offscreen` creates a multisampled DEPTH24_STENCIL8 renderbuffer, a
  multisampled RGBA8 color renderbuffer, a draw FBO, a resolve FBO, and a
  wrapped Impeller surface described in its own comment as "use-and-throw:
  draw once, then drop it".

At 2560x1440 with the standard 4x samples that is roughly 59 MB of MSAA
color, 59 MB of MSAA depth-stencil and 15 MB of resolve target - about
133 MB allocated and released per rasterization. On a 1080x2400 phone,
about 93 MB. Plus four GL object create/destroy pairs (two renderbuffers,
two FBOs; the resolve texture is adopted, not deleted by us) and a surface
object churned through the driver.

This is precisely the pattern the onscreen path already learned to avoid:
`window_surface` in alloy/src/raster.rs is retained across frames because
wrapping fresh per frame "churns a surface object per frame through the
driver, which is exactly the kind of sustained allocate/release cycle
ANGLE/D3D11 handles poorly". The offscreen path never got the same
treatment.

## Why nothing is reused today

Two separate reasons, and the first is the structural one.

**Invalidation drops the storage, not just the validity.**
`RenderTree::invalidate_paint` (alloy/src/rendertree/tree.rs) does
`element.paint_cache.borrow_mut().take()`, which drops the
`PaintCache::Snapshot { texture, .. }` and with it the ImpellerTexture and
its GL name. The next visit therefore has nothing to render into and starts
over. Note the walk runs from the changed node **up to the root**, so a
single leaf change drops the retained texture of every enclosing snapshot
boundary, not just the nearest one.

For the `Recording` variant dropping is right - a stale DisplayList is
worth nothing and costs nothing to rebuild. For `Snapshot` the pixels are
stale but the *allocation* is still exactly the right size and format.

**The transient rig is per-call by construction.** The renderbuffers, both
FBOs and the wrapped surface are locals inside `draw_offscreen`, created and
deleted on every invocation regardless of caching decisions above.

## Fix, in two parts

1. **Separate stale from freed.** Keep the texture as storage when a
   snapshot boundary is invalidated - a `valid: bool` on the Snapshot
   variant, or a `snapshot_storage` field that `invalidate_paint` does not
   touch - and have `snapshot_node` re-render into it whenever the retained
   size and display scale still match. Size or scale changes still
   reallocate, which is correct and rare.
2. **Pool the transient rig raster-side**, keyed by
   `(alloc_width, alloc_height, samples)`. The raster thread owns the GL
   context and already keeps `textures` and `shaders` maps, so it is the
   natural home. One-shot node captures share `draw_offscreen` and benefit
   from this half even though part 1 does not apply to them.

In-tree precedents for both halves: `window_surface` retains a wrapped
Impeller surface across frames; alloy/src/shader.rs re-attaches a different
target texture to a retained FBO with `framebuffer_texture_2d` plus a
completeness check and a revert on failure; and the raster-side `GpuTexture`
already pairs a GL name with its dimensions, which is what re-rendering into
an adopted texture needs.

Ordering is safe by the same argument gl.rs already makes for sampling:
one GL context in the process, so program order covers it - a re-render
issued after the previous frame's draws cannot execute before them, and no
fence or glFinish is needed.

## MSAA policy: stop paying 4x on every raster

`MSAA_SAMPLES` is a hardcoded 4 applied to every offscreen raster,
documented as matching the window surface's own 4x request. Two changes,
both in the same visit as the fix above.

**Provenance first, because it is the constraint.** The offscreen MSAA was
added for gradient emoji drawn through the `<svg>` primitive into a snapshot
boundary. That case genuinely needs coverage AA:
alloy/src/rendertree/kinds/svg.rs parses with usvg and emits
`builder.draw_path`, so Impeller fills it stencil-then-cover, and without a
multisampled target the result is hard-edged. Any sample-count work has to
keep that case at 4x - it is the regression test, not a corner case.

1. **One shared MSAA scratch rig instead of one per boundary.** The
   multisampled color and depth-stencil renderbuffers only live for the
   duration of a raster; only the resolve target persists. Allocate one rig
   at window size and render smaller boundaries into a subrect via the
   viewport, blitting that subrect out (the resolve blit already takes
   explicit rects). One MSAA color + depth allocation for the whole process
   regardless of boundary count or raster frequency. A small panel then
   pays a window-sized rig, but once, not per boundary per frame.
2. **An explicit opt-out on the boundary, not inferred from content.** Most
   UI boundaries hold nothing needing coverage AA - axis-aligned rects,
   textures and text (glyphs are atlas-sampled, so the AA is already in the
   atlas; worth confirming by rendering text into a single-sample offscreen
   and diffing against the 4x version). Those should be able to skip the
   MSAA rig entirely: no multisampled renderbuffers, no resolve blit,
   nothing to invalidate - roughly 133 MB down to 15 MB and one pass instead
   of two.

   The app author knows which it is, so say it at the call site:

       repaintBoundary?: boolean | "snapshot" | "snapshot-no-aa"

   `"snapshot"` keeps exactly today's behaviour, so no existing app changes
   appearance. The name of the opt-out is deliberately unattractive: it
   should read as "something was given up here" to whoever later finds a
   panel looking jaggy.

   Touches: the `repaintBoundary` type and doc in packages/core/src/types.d.ts,
   the prop parsing that feeds it, `BoundaryMode` in
   alloy/src/rendertree/mod.rs, and the samples argument threaded to
   `draw_offscreen`.

   Two alternatives considered and rejected:

   - *Infer it by recording what the subtree draws* (a `needs_coverage_aa`
     flag carried through `record_node`, set by svg/path/oval/line/rounded
     corners/rotations). Workable - alloy records the display list itself,
     so no Impeller introspection is needed - but it is invisible magic, it
     needs a conservative default plus vigilance every time a node kind is
     added, and it is more code than the explicit rule.
   - *Make no-AA the default and MSAA the opt-in.* Cheaper for the common
     panel, but it silently regresses the emoji case above: hard edges, no
     error, easy to miss on the developing machine. Perf costs announce
     themselves; visual regressions do not. Flipping it later is still an
     option, but only together with an audit of boundaries drawing vector
     content, never silently.

   Node captures share `draw_offscreen` and have no boundary prop to carry
   this, so they stay at 4x - one-shot, cost irrelevant. (The atlas bake
   supersamples at SS=4 and downsamples, so MSAA is nearly redundant there
   anyway; not worth an API to express.)

## Two adjacent wins while in that function

- `glInvalidateFramebuffer` on the MSAA attachments immediately after the
  resolve blit. Without it a tiler writes both multisampled buffers back to
  main memory for nothing. Likely the highest-value single line here on
  Android.
- `EXT_multisampled_render_to_texture` where available: the resolve becomes
  implicit and the separate MSAA color renderbuffer disappears entirely.

## Priority

High relative to the rest of the GPU backlog. It is the reason snapshot
boundaries are documented as being for static, raster-expensive content and
why AGENTS.md warns that creating several at once is a visible one-frame
hiccup - the guidance is really a workaround for the churn. It is also the
prerequisite for anything layer-shaped (a retained root layer an effect
shader can read), where the current behaviour would mean reallocating a
full-window rig on every frame that anything in the app changes.

## Resolution

Implemented 2026-07-27 via okf/plans/snapshot-offscreen-rig-churn.md, all
three stages (retain-and-re-render, shared rig, "snapshot-no-aa" opt-out);
runtime verification pending. Deviations from the sketch above:

- Part 2's rig pool keyed by `(alloc_width, alloc_height, samples)` became a
  single `gl::OffscreenRig` grown monotonically, with smaller rasters using
  a subrect viewport + blit - one rig per size class is strictly worse than
  one max-sized rig, and the subrect mechanics are the same work.
- The wrapped Impeller surface is still per-call: `wrap_fbo` takes a size
  that varies per boundary, so retention only helps consecutive same-size
  rasters. Revisit only if surface churn still shows in profiles.
- `EXT_multisampled_render_to_texture` was left out; worth its own backlog
  item once the shipped shape is measured.
- The `glInvalidateFramebuffer` line landed (version-guarded: ES 3.0 core,
  desktop GL 4.3+), and msaa-unavailable now latches once per process.

The AGENTS.md first-frame hiccup warning stays: creating many boundaries at
once still allocates one resolve texture per boundary; what this removed is
the per-re-raster rebuild.
