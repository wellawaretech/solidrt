# @solidrt/2d examples

Single-concept sprite-layer patterns. Each file is a complete, runnable app
(`bunx srt run <file>` from the repo root) demonstrating exactly one thing -
copy one and adapt it.

- `sprites.tsx` - the layer at its natural scale: 500 sprites bouncing at
  frame rate, moved imperatively with `setSprite` from `onFrame` while the
  tree holds one texture leaf. Atlas from PNG bytes (`createAtlas`) sliced
  2x2 with `grid()`.
- `pick.tsx` - sprite pointer events through the component face: exact
  rotated-rect hit testing topmost-first, pointer capture (drag a sprite and
  the events keep naming it), click-vs-drag slop, and removal through a
  signal so `<For>` unmounts the `<Sprite>` and the layer compacts its draw
  order.
