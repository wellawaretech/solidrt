---
title: Stats overlay should draw after the window shader pass
description: The debug overlay was recorded into the app's display list, so a window shader warped the HUD and its once-per-second refresh forced full rebuilds. Both halves fixed and verified 2026-08-09 - freeze was a dead overlay_due demand source; overlay now retained raster-side, rasterized to a small layer and blended over every frame post-pass.
created: 2026-07-27
completed: 2026-08-10
---

# Stats overlay should draw after the window shader pass

Observed with the stage-2 window shader (okf/plans/root-layer-effects.md):
with the stats overlay enabled, the HUD is warped/dissolved along with the
app, because `lattice/src/plugins/draw.rs` records the overlay into the same
DisplayListBuilder as the tree (`stats.draw(&mut builder, ...)` right before
`builder.build()`). The overlay is diagnostics, not app content; it should
stay legible no matter what program the app runs over its frame.

Two costs today:

- Legibility: a heavy effect (dissolve, strong warp) makes the HUD unreadable
  exactly when you want to watch frame times.
- Rebuild pressure: `overlay_due` forces a full display-list rebuild once per
  second while enabled (the overlay's figures are baked into the DL), which
  also means those frames can never take a clean-tree fast path (stage 4 of
  root-layer-effects): the submit is a rebuild, so the raster skip does not
  apply that frame.

Direction: record the overlay into its own small display list and hand it to
the raster thread separately (a second field on the Frame command, or a
retained overlay DL updated once per second), drawn after the shader pass
straight into FBO 0. Note the orientation trap from the frame-inversion work:
a post-pass draw into FBO 0 goes through Impeller's bottom-up window-target
convention, so the overlay DL must render the same way the unshaded frame
path does (no Y-reversed blits; see the plan's stage 1 notes). Keeping the
overlay out of the app DL also shrinks the once-per-second rebuild to an
overlay-only update.

Not urgent: the overlay is a dev tool, and toggling stats off restores the
clean picture. Worth doing together with (or right after) stage 4, which is
what makes the rebuild-pressure half visible.

## The overlay freezes on texture-driven apps (2026-08-02)

From the wasm game-port demo feedback, and it raises this item's priority
above "not urgent": with the game running, `stats on` shows an overlay whose
every field - fps included - is frozen while the game animates underneath.
get_stats over MCP returns live values the entire time; it is the PAINT that
is stale. The app writes zero properties per frame (its pixels arrive only
through uploadTexture, which re-renders the shader target and re-composites),
so the render tree is structurally never dirty, the demand gate reuses the
painted output, and the overlay - painted inside that gated pass - is reused
along with it.

Measured: adding a single per-frame property write (a 1 px d-rect whose x
alternates) dropped reusedPerSec from 70 to 25 and brought the overlay back
to life. The runtime presents ~58 fps against a 35 Hz game tick, so ~23
frames/s write nothing - reusedPerSec counts exactly the frames where no
property write dirtied the tree.

The gate is behaving correctly; the overlay's dependency on it is the bug.
It hits ANY app whose pixels come from outside the render tree - game ports,
video playback, camera feeds, render:"manual" simulation targets,
shader-only scenes - which makes it a diagnostic tool failing silently at
exactly the moment something is being diagnosed. The fix direction is the
same as the warp problem above: composite the overlay outside the app's
gated pass (or have it mark itself dirty while enabled). Worth an AGENTS.md
line on what reusedPerSec means regardless, since it is currently the only
visible signal of the gate.

## Freeze half resolved: dead overlay_due (2026-08-09)

The draw loop already had the "mark itself dirty while enabled" mechanism -
overlay_due forces a frame through the demand gate and bypasses the
present-only reuse path once per second - but it had regressed to dead code.
The "Introduce MCP" commit (cdff25c) moved the once-per-second refresh()
into Stats::record_js so cpu/mem stay fresh for get_stats with the overlay
off. record_js runs at the top of every render() call, a few lines before
the gate reads overlay_due(), so whenever the sample came due record_js
consumed it and reset the timer in the same call; overlay_due() then read
~0 elapsed. Always false, so the overlay froze on texture-driven AND fully
idle apps (normally-animating apps rebuild every frame anyway, which is why
it went unnoticed).

Fixed by latching overlay_due before record_js in render() (see the comment
there); the HUD now refreshes at 1 Hz on reuse-path and idle apps. fps
counting itself was never wrong - the raster thread reports every present -
only the painted HUD was stale, which is why get_stats stayed live.

Still open: the post-shader-pass half above (warp legibility + the
once-per-second full rebuild defeating the clean-tree fast path).

## Post-pass half implemented and verified (2026-08-09)

The overlay is now retained raster-thread state, mirroring the window
shader's own pattern: `RasterCmd::SetStatsOverlay` installs/clears a small
overlay declaration (display list drawn at origin + the window rectangle it
belongs in, physical pixels).

First attempt drew the overlay DL straight over FBO 0 via a wrapped
Impeller surface and blacked out the frame: an Impeller surface draw ALWAYS
clears its target (documented on `Surface::draw_display_list`, impellers
0.4.2) - the composite can never be an Impeller draw onto the finished
frame. Actual mechanism: the declaration is rasterized once into a small
retained layer texture (`create_layer_target` + the offscreen rig, same
machinery as snapshot boundaries), deliberately UNFLIPPED so the layer
shares FBO 0's bottom-up convention, then blended over the finished frame
each frame with the shared copy program (`composite_program_over_window`:
viewport-positioned, premultiplied ONE/ONE_MINUS_SRC_ALPHA, no flip
anywhere). Composited before the capture readback, so playback frames carry
it. Per-frame cost is one ~200x150 blended quad; the Impeller rasterize
happens only when the declaration changes.

Lattice rebuilds the declaration once per second (`Stats::build_overlay`,
figures sampled in the same frame that shows them) - and immediately on
window-size/scale/safe-area changes, since placement is window-space
raster-side - and pushes it ahead of that frame's submit on the ordered
channel. Consequences:

- A window shader can no longer warp the HUD: the pass runs first, the
  overlay draws after it.
- The once-per-second full tree rebuild is gone: the reuse path no longer
  bypasses on a due overlay, and DlCache dropped its stats_enabled key.
  SetStatsOverlay is exempt from content_dirty, so the shaded clean-tree
  fast path (pass-only frames) survives overlay refreshes too.
- PaintStats are latched in Stats (like the layout counters) so an overlay
  built on a reuse frame has boundary/snapshot figures.
- The overlay clears with its engine (RenderInner::drop), so an app switch
  cannot leave a stale HUD.

The reusedPerSec AGENTS.md line landed in the scaffold's get_stats bullet.
Verified live on the window-shader example: HUD crisp and undistorted over
the warp, updating once per second, app content intact (the first-attempt
Impeller-over-FBO0 draw blacked it out; the layer+blend mechanism does
not). Note for anyone reading the HUD there: fps stays ~60 with the warp
amount at 0 because the example writes uTime into the shader params every
frame - a standing frame request presenting identical pixels - not because
the overlay forces frames.