---
type: backlog-item
title: Stats overlay should draw after the window shader pass
description: The debug overlay is recorded into the same display list as the app, so a window shader warps the HUD too, and its once-per-second refresh forces full rebuilds that defeat clean-tree fast paths; draw it post-pass, outside the gate. The texture-driven freeze half was a dead overlay_due demand source, fixed 2026-08-09.
status: open
timestamp: 2026-07-27T00:00:00Z
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