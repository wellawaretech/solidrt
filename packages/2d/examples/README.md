# @solidrt/2d examples

Single-concept sprite-layer patterns. Each file is a complete, runnable app
(`bunx srt run <file>` from the repo root) demonstrating exactly one thing -
copy one and adapt it.

- `sprites.tsx` - the layer at its natural scale: 500 sprites bouncing at
  frame rate, moved imperatively with `setSprite` from `onFrame` while the
  tree holds one texture leaf. Atlas from PNG bytes (`createAtlas`) sliced
  2x2 with `grid()`.
- `tiles.tsx` - the baked tile layer: a 128x128 world (6144px - bigger than
  one texture may be) baked into lazily-allocated chunks, flown over by a
  ship-style camera (fixed screen pivot, the world panning and ROTATING
  under it via the `<TileLayer>` camera prop - transform writes, never a
  re-bake), and a timer editing tiles to show that a `setTile` batch
  re-bakes only the chunks it touches.
- `anim.tsx` - frame animation with `createAnimation`: three sprites share
  one looping clip (one clock, stepped by wall time independent of display
  rate) and a one-shot clip holds its last frame and fires `onEnd`.
  Self-asserting - watch the logs for ANIM-OK.
- `camera.tsx` - the 2d camera (`createCamera2d`) over a world larger than
  the window, attached at the layer's root (`cam.attach(layer)`): drag
  empty space to pan with inertia, wheel/pinch to zoom about the pointer,
  tap empty space to glide there (the layer's `onTap` with `e.sprite`
  null), tap a sprite to select it, drag a sprite to move it (the sprite
  stops its down, so the camera never pans under it), F to follow a
  roaming sprite through a dead zone, R to spin the view, Space to fit the
  world. The LAYER's `handlers` spread onto its leaf; `update(dt)` from
  `onFrame` is the only per-frame call. Debug commands `camera`, `mode`,
  `selected` and `first` drive it headless.
- `pick.tsx` - the event model through the component face: exact
  rotated-rect hit testing topmost-first, `onTap` (the dispatch's own
  click, no slop bookkeeping), a claimed press dragging a sprite under a
  `<Camera2d>` that pans empty space and zooms on wheel, the layer's own
  `onTap` seeing misses with `e.sprite` null (and `tapCount` for double
  taps), and removal through a signal so `<For>` unmounts the `<Sprite>`
  and the layer recycles the slot. The `state` debug command reads the
  camera and sprite positions back.
