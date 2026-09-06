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
  the window: drag to pan with inertia, wheel/pinch to zoom about the
  pointer, tap to glide there, F to follow a roaming sprite through a dead
  zone, R to spin the view, Space to fit the world. The camera's `handlers`
  spread onto the layer's leaf; `update(dt)` from `onFrame` is the only
  per-frame call. Debug commands `camera` and `mode` drive it headless.
- `pick.tsx` - sprite pointer events through the component face: exact
  rotated-rect hit testing topmost-first, pointer capture (drag a sprite and
  the events keep naming it), click-vs-drag slop, and removal through a
  signal so `<For>` unmounts the `<Sprite>` and the layer compacts its draw
  order.
