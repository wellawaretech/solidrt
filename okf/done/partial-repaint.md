---
title: Partial repaint - damage rects for the raster pass
description: Every frame with any damage re-rasterizes the full window display list; a 1 px change is a full-screen raster + resolve. Track screen-space damage rects and scissor the raster to them (EGL buffer age / swap-with-damage), so fill cost scales with what changed.
created: 2026-09-02
completed: 2026-09-02
---

# Partial repaint - damage rects for the raster pass

## Symptom

There is no partial repaint and no retained layer tree. Repaint boundaries
cache display-list rebuilds (Recording) and subtree raster (Snapshot), so
the BUILD side already scales with damage - but the raster pass replays
the full window display list into the swapchain every frame that has any
damage at all. A 1 px cursor blink is a full-screen raster plus MSAA
resolve.

Flutter's layer tree (with partial repaint on some embedders) and
Chrome's cc tiling + damage rects both avoid exactly this. On the
fill-rate-bound targets we care about (phones, MediaTek TVs) this is the
ceiling on battery and fill rate: demand-driven rendering already keeps
idle frames free, but any animated pixel makes every frame cost the whole
window.

## What done looks like

A frame whose damage is one small rect rasters and resolves roughly that
rect, not the window. Legible in stats (e.g. a damage-area or
pixels-rastered counter next to `nodesPainted`), and measurable as
frameMs / power on a TV with a small animation running.

## Design

Two halves: knowing WHAT changed on screen, and telling the GPU to only
redo that. Everything is conservative by construction: any uncertainty
degrades to full-frame damage, never to a wrong skip.

**Screen-space damage accumulation (stage 1).** A per-frame damaged-id
set on the RenderTree, plus a retained per-element window-space extent
cell maintained by the paint walk itself:

- Every mutation path inserts the damaged node's id: `apply_damage` /
  `apply_damage_batch` (skipping None/Present), the structural sites
  that bypass them (`insert_node`, `detach_node_now`, `destroy_node` all
  insert the parent id; `set_root` goes full), `set_unrounded_layout`
  (a node whose computed layout changed - this is what catches siblings
  shifted by someone else's relayout), and `texture_content_changed`
  (every referencer of a changed texture id, not just snapshot
  consumers). A cap on the set degrades huge batches (resize-scale
  relayouts) to full damage instead of paying per-node rect math.
- Each element keeps a `Cell<Extent>`: its subtree's window-space
  painted extent as of the last paint walk. The walk already fetches
  the child's slot-frame envelope (cull.rs) for the viewport cull test;
  a forward window transform carried in BuildContext (the mirror of the
  cull rect's inverse steps: own matrix, scroll, fit, child translate)
  maps it to window space for one Cell write per considered child.
  Non-2D transforms map to Unbounded, so anything under a 3D chain
  degrades to full damage on its next change.
- Resolving a frame's damage is then two cheap reads sandwiching the
  paint walk: before it, union the damaged ids' cells (their OLD
  extents - movement's erase side); after it, union the same ids' cells
  again (their NEW extents). A damaged id whose cell is Empty falls
  back to the nearest ancestor with a non-Empty cell (covers spans,
  which the walk never visits as children; terminates at the root,
  whose cell is the window).
- Why stale cells are safe: a node's cell is only stale while it sits
  inside a valid boundary cache, and a cache is valid only while
  nothing inside changed. Whatever moved the subtree (Compose on an
  ancestor) was itself damaged that frame, and its old + new union
  covered the node's pixels at both locations then. So a stale cell can
  only over-damage, never under-damage. A damaged node is always
  re-visited by the walk (its damage dropped every enclosing cache) or
  culled off-window, so its fresh extent is always current.

**Damage-limited raster + present (stage 2).** The probe (see Findings)
killed the note's original "root clip over the back buffer" idea:
Impeller clears every wrapped target, so the aged back buffer's content
cannot be drawn over in place. The validated mechanism instead reuses
the offscreen rig:

- Per frame the raster thread queries `EGL_BUFFER_AGE_EXT` on the
  current EGL display/surface (khronos-egl on
  eglGetCurrentDisplay/Surface; the SDL swap path stays untouched -
  buffer age is valid with plain eglSwapBuffers) and keeps a short ring
  of past frame damages. The repaint union is this frame's damage plus
  the previous `age - 1` entries; age 0, age beyond the ring, no
  extension, resize, or a window shader means full frame.
- The frame's display list is drawn into the retained rig with a root
  clip on the repaint union (Impeller culls ops outside a clip cheaply;
  the rig's full clear stays and is cheap), and only the union rect is
  blitted into the aged back buffer - rig and backbuffer share
  orientation, so the blit rect is identical on both sides. The MSRTT
  resolve copy-draw gets the same scissor. Frames shed by the raster
  loop's load shedding union their damage into the surviving frame, and
  the dev overlay's rect joins the union while it is active (its blend
  from last frame must not double-composite).
- The Android multisampled-FBO0 fast path draws into the window
  directly and cannot preserve (Impeller clears it), so it keeps
  full-frame drawing until measured on device; partial repaint there
  would ride the MSRTT rig path.

## Stages

0. Probe (`alloy/examples/partial_repaint_probe.rs`): Impeller wrap_fbo
   load behavior, buffer-age availability and honesty, and the
   rig-blit mechanism end to end. Done, see Findings.
1. Damage accumulation and visibility, no rendering change: the id set,
   the extent cells, the resolve, and a damage-area stat in PaintStats
   surfaced next to `nodesPainted` in `get_stats`. Done, see Findings.
2. The win: repaint-union clip + rect blit + buffer-age present on the
   EGL/GLES rig path; full-frame fallback everywhere else. Done, see
   Findings; power/frameMs on fill-bound hardware (a TV) still to be
   measured.
3. Follow-ups extracted at completion: the Android fast-path decision
   is [partial-repaint-android](../backlog/partial-repaint-android.md)
   (symptom measured on the TV); the compositor damage hint and
   multiple damage rects are ideas.md lines awaiting a measured victim;
   layer promotion (the Chrome-style layer tree) stays a deliberate
   non-goal until an app demands it.

## Findings

The durable engine facts (Impeller clears every wrapped target; rig and
FBO 0 share orientation; buffer-age availability per stack) are cut to
[impeller-wrapped-targets-and-buffer-age](../notes/impeller-wrapped-targets-and-buffer-age.md);
what follows is this plan's own record.

Stage 0 probe, 2026-09-02, Linux desktop (Mesa Intel RPL-P, GLES 3.2,
SDL on EGL):

- **Impeller never loads a wrapped target.** With FBO 0 pre-filled and
  a display list whose root is a clip rect, every pixel outside the
  clip came back black after `wrap_fbo` + draw, and a blend inside the
  clip blended against black, not the previous content. Confinement via
  clip is real (the draw stayed inside), but the pass clears the whole
  target first - so partial repaint must compose in an offscreen target
  and blit, never draw over the preserved back buffer.
- **Buffer age is present and honest here.** Age settles at 3 after 4
  warm-up frames; over 12 frames the pixel read back before drawing
  always matched the frame from `age` swaps ago.
  `eglSwapBuffersWithDamageKHR` and `eglSetDamageRegionKHR` (with
  `EGL_KHR_partial_update` listed) both resolve.
- **The rig mechanism works end to end.** A moving-widget loop - damage
  union over age, root-clipped rig draw, rect blit into the aged back
  buffer - held pixel-correct for 22 of 24 frames (the other 2 were
  age-unknown warm-up, drawn full); a corner pixel never redrawn after
  frame 0 stayed intact across every swap. Steady-state repaint area
  was ~2% of the window.

Stage 1 landed 2026-09-02: `RenderTree::note_damage` plus the
per-element `last_extent` cells (written in `record_node`'s child loop
off the same envelope the cull test fetches, through the forward
`WindowMap` in BuildContext), resolved in `paint_phase` by the old and
fresh cell reads around the walk; `FrameDamage` retained on the tree,
`damage_px` in PaintStats, `damagePx` in the dev-server stats JSON and
the get_stats prose. Unit tests in `alloy/src/tests/damage.rs` cover
the paint, compose-move, relayout-shift, scroll and removal rects plus
the None and Full paths. Verified live with `probes/damage-probe.tsx`
(800 static d-rects behind a Recording boundary, one 20x20 mover
stepping 8 px): rebuild frames report damagePx 660 - exactly the
mover's old+new union with the 1 px AA outset, (28+2)x(20+2) - with
nodesPainted 2 and a clean jank read (0 missedPresents, p95 0.31 ms).
Carry-overs for stage 2:

- Reused (present-only) frames do not resolve damage; ids noted by
  `texture_content_changed` stay in the set until the next rebuild.
  Stage 2's reuse path must drain them and union their cells directly
  (geometry is unchanged there, so the cells are current).
- The envelope is now computed for every considered child (it used to
  be skipped where the cull rect is suspended, inside boundary
  re-records); it is cached and invalidated with paint, so the
  amortized cost is one extra pass per invalidated subtree.
- `Damage::Present` unions nothing by design; the window-shader writes
  it carries fall under stage 2's window-shader full-damage fallback.

Stage 2 landed 2026-09-02. The pieces:

- `PresentDamage`/`DamageRect` (raster/cmd.rs): the frame's content
  delta in physical pixels, converted from the logical FrameDamage at
  submit (1 px pad absorbs scale rounding); `Context::submit` and
  `submit_clean` now carry it, `Full` being the always-correct default
  for raw-alloy callers. The reuse path resolves its damage without a
  walk (`composite::resolve_reuse_damage`, the stage 1 carry-over):
  only GPU-content ids land there and their cells are current.
- `raster::buffer_age`: EGL_EXT_buffer_age queried against the raster
  thread's current display/surface via khronos-egl (the loader shared
  with egl_headless); SDL's swap path untouched. Probed once, logged
  ("partial repaint: EGL buffer age available" / "off: reason").
- `RasterState::repaint_patch`: the frame's own delta (plus load-shed
  frames' - the run loop unions every received frame's damage - plus
  the overlay rect while one composites, since its blend must not
  stack) unioned with the last `age - 1` ring entries. The ring keeps
  per-present content deltas (DAMAGE_RING = 8); resize, playback,
  window shader, age 0, age past the ring, or a failed present (ring
  cleared, delta carried forward) all mean full frame.
- The draw (gl/draw.rs): the patch becomes a root clip_rect wrapped
  around the frame's display list raster-side (Impeller owns GL
  scissor state, so confinement must come from the list), the rig's
  full clear stays, and the resolve copies only the patch - the
  explicit blit uses the patch rect (same rect both sides, GL-flipped
  once), and the MSRTT copy-draw gets a scissor through a new optional
  scissor on `run_pass`/`render_program_to_fbo`. The Android
  multisampled-FBO0 fast path ignores the patch and stays full-frame.
- `partialPresents` (RasterStats -> get_stats): cumulative presents
  that drew only a patch.

Verified live (Linux desktop, Mesa Intel): damage-probe reports
partialPresents climbing in step with its 10 Hz animation;
examples/gallery under scroll + hover bursts ran 300 frames with 0
missedPresents, 0 slowFrames, p95 1.85 ms, no warnings or GL errors;
examples/spin (3d) holds 60 fps with its Scene-texture content damage
riding the reuse-path resolve. Full-suite tests green (377 alloy, 56
flux gui). Visual check on the gallery under scroll/hover confirmed
clean (no stale or torn regions).

TV measurement, 2026-09-02 (Philips TPM171E, MediaTek armv7, 1920x1080
at 50 Hz, release armv7 client): the window backbuffer is 4x
multisampled, so the Android in-tile fast path is active and
`repaint_patch` correctly answers full frame (a gate added this session
- without it partialPresents would have counted fast-path frames that
actually drew full). The cost of that, measured with two probes sharing
the same 800-rect boundary-wrapped field and a 10 Hz animation:

- damage-probe (20x20 mover): gpuFrameExecMsPerFrame 39.4-40.4 ms,
  frameMs ~28-30, cpuPct ~130, 0 missedPresents.
- damage-probe-full (window-covering animated rect): 44.4-45.4 ms.

A 20 px change costs ~88% of a full-window change - fill cost is
damage-size-independent on this device, exactly the item's symptom, at
~40 ms GPU per animated frame. That is the quantified opportunity for
the Android follow-up (partial-repaint-android in the backlog): a
patch-confined MSRTT rig frame should bring the mover frame down
toward the desktop behavior. SurfaceFlinger's context lists
EGL_KHR_partial_update (which requires EGL_EXT_buffer_age per the
Khronos spec), so buffer age is very likely available to the app there
too. The old (2026-08-18) installed client could not serve as an A/B
baseline against today's server (missing setPointerLock export), hence
the two-probe comparison.

## Related

- [content-damage-perf](../backlog/content-damage-perf.md) - CPU-side costs of the
  damage-tracking path (the O(nodes) texture walk, the unbatched
  invalidate_paint on layout change); orthogonal to the fill cost here,
  but stage 1's damage-rect accumulation rides exactly that
  invalidate_paint walk - the batched form with a shared visited set is
  where old + new bounds would be unioned per frame.
- [2d-baked-layers](../backlog/2d-baked-layers.md) - the app-level mitigation for
  static 2D bulk; this item is the engine-level answer for the general
  rendertree.
- MSAA interacts: the resolve is per-frame full-target today; with a
  damage scissor the resolve region should shrink with it (check what
  the GL resolve path allows).
