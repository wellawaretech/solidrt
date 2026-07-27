---
type: backlog-item
title: Stats overlay should draw after the window shader pass
description: The debug overlay is recorded into the same display list as the app, so a window shader warps the HUD too; draw it post-pass so diagnostics stay legible, which also stops the overlay forcing full rebuilds during effect-only frames.
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