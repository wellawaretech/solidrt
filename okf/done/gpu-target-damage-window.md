---
title: Damage rectangles reach the window - a damaged target damages only what the node showing it shows of them
description: Landed 2026-11-12 - a content change carries its dirt (full or rectangles, the raster thread's model) to the tree, and a texture node maps the rectangles through its fit, source rectangle and window map into window damage; a press on a full-window view damages 5,214 px of 921,600 on the desktop client and every frame of it is a partial present; content under Recording boundaries damages where it shows now, composed up the boundaries' frames; the damage area reported for reused frames and in the stats window; a headless patch switch and window-pixel tests that compare a patched window with a whole redraw.
created: 2026-11-05
completed: 2026-11-12
---

# Damage rectangles reach the window

The window side of
[gpu-target-damage-draw-lists](gpu-target-damage-draw-lists.md). What is
left of the item, damage declared by an app, is
[gpu-target-damage](../backlog/gpu-target-damage.md).

## The symptom it answered

A culled view where one sprite animates rendered only that sprite's
rectangle into its target, and then damaged the whole box of every node
showing the target: a full-window view repainted the whole window every
frame of the animation, and no frame of it was a partial present.

## Where it pays

The window is patched from damage on every platform where the driver
lets it. A single-sample backbuffer, every desktop platform by design
(`configure_opengl`), draws frames into the offscreen rig and blits a
patch from it (`WindowRoute::Rig`). Android asks for a multisampled
backbuffer, the only MSAA configuration a tiled GPU runs at full rate.
A multisampled window whose display offers `EGL_KHR_partial_update`
declares the frame's patch as its damage region, draws the patch into
the corner of the patch layer (one layer kept between frames, at least
the stepped patch's size, allocated again at the patch's size after a
run of frames over four times the patch's area, its old pixels
invalidated before each patch) and copies that into the window with a
copy scissored to the patch (`WindowRoute::PatchLayer`); both Android
devices measured offer it. A frame that declares and then does not
present (a failed draw or swap) makes the frames after it draw whole
until every buffer of the swapchain is redrawn whole, and a declaration
the driver refuses turns the window's patches off for the context. A
patch over half the window, a window without the extension, a window
read back by a window tap and `SOLIDRT_WINDOW_PATCH=off` take the fast
path instead, which draws the whole frame. The gains and the variants
that were not shipped are in
[target-damage-measurements](../notes/target-damage-measurements.md),
the last section.

## What was built

- **One dirt model on both threads.** `DirtySet` moved from the raster
  thread to `alloy/src/gpu/dirt.rs`. The Context's content set
  (`content_changes`) is one: each texture id with full dirt or
  rectangles. A draw list's write notes the rectangles it sends the
  raster thread (`note_content_dirt`); every other change is full, and
  full wins. A tile's rectangles move by its origin into its parent's
  pixels. What samples the texture downstream changes in full, a sampler
  noted inside rectangles included, and so does the depth texture.
  `take_content_changes` widens two cases to full: every change while
  the force-full switch is on, so the switch diagnoses a wrong window
  rectangle as it does a wrong target rectangle, and a texture with a
  mip chain, whose levels spread a texel past what the rectangle maps
  to.
- **How the node was drawn.** A texture node keeps what its last build
  placed (`rendertree::Shown`): the source rectangle after the fit's
  crop, the destination rectangle in its box frame, and that frame's map
  to the window. Inside a Recording boundary's record walk the map is to
  that boundary's frame, and `Shown` names the boundary (see below).
- **The mapping.** `RenderTree::texture_content_changed` takes the dirt.
  For a change inside rectangles each one grows by a texel (linear
  filtering), is cut to the source rectangle, scales onto the
  destination, goes through the window map (its bounding box under a
  rotation), grows by a pixel (the box offset and the rasterization
  edge) and is cut to the node's extent. The ledger takes the result as
  a window area beside its node set (`DamageLedger::note_area`), and
  both resolves fold it in. A change the node does not show (outside a
  crop) damages nothing.
- **Under Recording boundaries.** A Recording boundary's cached content
  is recorded in its scrolled box frame: the composite applies its
  matrix and scroll around the cached display list, and the rest is the
  recording's own. Every walk that reaches the boundary, replay or
  record, notes that frame's map to the window, and its map to the frame
  of the boundary above it (`cull::RecordingFrame`, kept by the tree for
  boundaries only, `recording_frames`). The record walk and the note use
  one step for the matrix and the scroll (`cull::map_past_box`), so they
  cannot differ. `content_reach` composes, in its one walk up, the
  texture's map to its boundary's frame, each boundary's map to the next,
  and the outermost one's map to the window. The result cuts to the
  outermost boundary's extent, which its parent's loop writes on every
  walk. A full change there damages the node's drawn box where it shows,
  not the boundary. Each link names the boundary it is relative to; a
  link that names another breaks the chain (a capture's walk built the
  node outside the recording that replays), and the boundary is damaged
  whole.
- **The whole node** still, for: a full change outside a boundary, a map
  that is not 2d or has no inverse, a node no walk has built, and a node
  under a snapshot boundary or a view with a blur filter (the outermost
  boundary or blurring view, whole).
- **The stats.** A reused frame reports its damage area
  (`Commit::Reused { damage_px }`): `damagePx` read 0 for every frame
  that presented GPU content through the retained display list. Each
  frame record carries it, and `window.damagePx` sums it, so
  `/stats?frames=1` reads the last frame that changed the picture.

## A fix beside it

A replayed recording is not walked, so the nodes under it keep the
extents of the walk that recorded it. A content change behind a texture
under a repaint boundary that had moved or scrolled since damaged the
texture's old place, and a partial present left the new place stale.
The first fix took the outermost boundary's whole extent. Under
Recording boundaries the precise rule above replaces it; the whole
outermost node stays for a chain with a snapshot boundary or a blurring
view, and for a chain whose map is not known.
`alloy/src/tests/damage.rs` holds the cases.

The same kind of hole held for tree damage under such a boundary. A
node inside a replayed recording keeps the cell of the walk that
recorded it, so a later move, hide or removal of it damaged that old
place, not the place the replay showed it at. A Recording boundary now
keeps, beside its frame as the last walk saw it, the frame it was
recorded at (`RecordingFrame::recorded`). Where a boundary on a damaged
node's chain moved since it recorded, or has no frame noted, the
outermost repaint boundary's cell stands in for the node's
(`RenderTree::boundary_stand_in`, read by `composite::damaged_extent`
before the walk, after it and on a reuse frame). Under a boundary that
has not moved, the node's own cell stays, and the damage is the same as
before.

It is the outermost boundary of any kind, not the outermost Recording
boundary: only a boundary outside every cache has a parent whose loop
writes its cell on every walk. A reused snapshot is not walked either
and keeps no frame, and a Recording boundary inside a reused snapshot is
not visited, so its frame does not say that the snapshot moved. So a
snapshot boundary on the chain counts as a boundary that has always
moved, as `content_reach` already treats it for a content change. A
capture's walk under a replayed recording writes the cells inside at
the boundary's place now, and a later move back to where it recorded
made the frame match the recorded one again while the cells stayed
where the capture left them. The walk now clears the recorded frame
(`RenderTree::note_recording_walked`), and the boundary counts as moved
until it records again.

The stats overlay and the window shader had holes of this kind on the
raster thread. A frame's own damage holds the overlay's rectangle only
while it is installed, so the frames after a removal, a move or a resize
left it in the aged back buffers. Every frame under a window shader
redraws whole, but the ring records only the tree's delta, so the
buffers kept shaded pixels after the shader went. The headless target
hid both behind `window_target_held`. Now an overlay that leaves a
rectangle folds it into the tracker, and a new window shader program or
its removal folds full damage; the ring carries either to every aged
buffer. `window_target_held` no longer guards them.

A boundary frame with a tiny scale inverts with a large f32 error, and a
scale back up replays the recording without a new record walk. The
error is per axis, so the bound is on the smaller axis scale, not on the
determinant: a scale of 1e-6 on x and 100 on y has a determinant of
1e-4 and maps several px off. A frame whose determinant is under
`MIN_FRAME_SCALE` times the Frobenius norm of its linear part
(`cull::map_relative`) counts as having no inverse, like a zero scale,
and the boundary is damaged whole.

A view with a blur filter spreads each changed pixel of its subtree by
the blur's reach. Only the filtered view's own extent holds that reach,
so a changed rectangle under it damaged too little, and a partial present
left a ring of stale blurred pixels around it. Such a view now counts
like a repaint boundary: the change damages the outermost of the two,
whole. A filter of only color operations spreads nothing and keeps the
rectangle. `alloy/src/tests/damage.rs` holds the cases.

## The window's pixels in a test

A headless frame redraws the whole window, so a test could check the
damage area but not its place. The patch switch
(`Context::set_patch_headless_window`, `RasterCmd::SetPatchHeadlessWindow`,
`patchWindow` in `flux:test/gui`, `app.patchWindow` in `@solidrt/test`)
routes a headless frame like a windowed one. The headless window target
holds the last frame, so it is a back buffer of age 1: a frame redraws
its own damage and keeps the rest. A resize, a fresh target, a full
damage, a window shader and a frame that did not draw redraw it whole
(`window_target_held` is false only for a fresh target and after a
frame that did not draw). An overlay change and a window shader change
fold their repair into the damage, as for a window. A patched headless frame counts
in `partialPresents`. The switch is off by default, and the test host
turns it off ahead of every test. `app.windowPixels()` (`windowPixels`
in `sol:test`) reads the window target as the last frame left it.

## Verified

- `alloy/src/tests/damage.rs`: the mapping through a fill, a crop, a
  letterbox, a moved and scaled ancestor and a rotated one; rectangles
  that union and stop at the node; each whole-node fallback; a change
  and a move in one frame; under a Recording boundary that moved (a
  rectangle and a full change), that scrolled under a clip, and under
  two nested ones that moved in turn; the whole outermost node for a
  chain with a snapshot boundary or a blur, inside a recording or
  outside it; a boundary recorded at a zero or a tiny scale, or at a
  tiny scale on one axis and a large one on the other, and scaled back
  up; a full change under two nested moved boundaries; display
  scale 2; a rounded texture; a boundary mode that comes and goes. A
  child moved, hidden or removed under a moved Recording boundary
  damages its true old place (the removed child overhangs its parent, so
  the parent's extent does not hold it), and under an unmoved one the
  same damage as with no boundary. A child hidden after a capture's walk
  under a recording that moved and moved back damages its true place.
  Under a snapshot boundary, and for a Recording boundary inside one or
  around one, the outermost boundary stands in; that case is read from
  `boundary_stand_in` directly, since a headless snapshot fails to
  rasterize, paints inline on every walk and keeps its cells fresh.
  `alloy/src/tests/repaint.rs`: the overlay's repair decision (a
  removal, a move and a resize fold the old rectangle; a content-only
  change and a first install fold nothing), a moved overlay's repair, a
  folded rectangle and a folded full damage reach both buffers of an
  age-2 chain, and a known buffer age routes like an EGL one.
  `alloy/src/tests/gpu_graph.rs`: a sampler noted inside rectangles
  widens to full.
- `alloy/examples/target_damage.rs` (real GL): a write on a tile notes
  the parent's id with the tile's two rectangles in the parent's pixels,
  and in full under the switch.
- `packages/2d/tests/target-damage.test.tsx`: a sprite and a record
  write on a view shown at twice its size damage about the two boxes
  through the scale, under 1% of the node; under the switch and after a
  camera move, the whole node. Under the patch switch, the window after
  every step is byte-identical to a whole redraw of an identical scene,
  for a view shown at twice its size, at a quarter of it (the one-texel
  filter reach holds without a mip chain), inside a repaint boundary
  moved after its first frame, under a blur, for a write with a camera
  move in one frame, and with a window shader turned on and off. A mapped rectangle moved by 6 px fails the
  first three.
- On the desktop release client (2026-11-12, an RTX 3070 on Wayland,
  1280 x 720 window, `probes/target-damage-probe.tsx` PLAIN, the view
  filling the window): figures in
  [target-damage-measurements](../notes/target-damage-measurements.md).
  With the clock frozen and the same stepped press, a screenshot of the
  window after partial presents is byte-identical to one after forced
  full renders, the client's own fps badge aside.
