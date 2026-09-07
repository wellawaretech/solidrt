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
  the window, driving the layer's one view from an input map the app
  binds (`camera2dBindings` over the view's pointer feed, bridged from
  the view's root with `feedPointer`, and any pad): drag empty space to
  pan with inertia, wheel/pinch to zoom about the pointer, tap empty
  space to glide there (the view's `onTap` with `e.sprite` null), tap a
  sprite to select it, drag a sprite to move it (the sprite stops its
  down, so the camera never pans under it), F to follow a roaming sprite
  through a dead zone, R to spin the view, Space to fit the world. The
  VIEW's `handlers` spread onto its leaf; `update(dt)` from `onFrame` is
  the only per-frame call. Debug commands `camera`, `mode`, `selected`
  and `first` drive it headless.
- `pick.tsx` - the event model through the component face: exact
  rotated-rect hit testing topmost-first, `onTap` (the dispatch's own
  click, no slop bookkeeping), a claimed press dragging a sprite under a
  `<Camera2d>` that pans empty space and zooms on wheel, the layer's own
  `onTap` seeing misses with `e.sprite` null (and `tapCount` for double
  taps), and removal through a signal so `<For>` unmounts the `<Sprite>`
  and the layer recycles the slot. A corner `<View2d>` inset shows the
  same sprites again under a `<Camera2d>` of its own (drag or wheel on
  the inset moves only the inset's camera; a sprite tapped or dragged
  there responds as in the main view). The `state` debug command reads
  both cameras and the sprite positions back.
- `views.tsx` - layer views (`layer.createView`, the only way a layer
  shows): the world rendered twice from ONE set of sprites - the
  window-filling main view under a `createCamera2d` and a corner minimap
  showing the whole world at the fit zoom, a stroked rect outlining the
  main view rect through `map.project`. A tap on the minimap glides the main camera there (the
  view's root listener, `e.x`/`e.y` in world pixels), and a tap on a
  sprite in the minimap selects it as in the main view (the same sprite
  handlers, the walk ending at the view). Debug commands `state`,
  `camera`, `selected` and `first` drive it headless.
- `split-screen.tsx` - one `<SpriteLayer output={false}>` (sprites, no
  leaf of its own) shown through two `<View2d>` panes filling the window
  side by side, each with a `<Camera2d>` of its own following one of two
  roaming sprites through a dead zone, so the two poses differ every
  frame; wheel zooms a pane alone while its follow keeps tracking, a tap
  on a sprite in either pane tints it in both (one sprite, two views).
  The `cameras` debug command reads both poses back.
