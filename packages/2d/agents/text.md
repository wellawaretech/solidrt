# @solidrt/2d - text runs

Text runs drawn as sprites. Read ../AGENTS.md first: it carries the model
this file builds on.

## Text runs

Text IN the layer's world - a label under a sprite, a name on a map, a
damage number - is a text run: a group of glyph sprites drawn from a
SPRITE FONT's atlas (`createSpriteFont`, `addText`/`setText`/
`destroyText`, `<Text2d>`; `examples/text.tsx`). The font is the
runtime's own glyph engine (`flux:font`): it shapes the text (kerning is
the engine's, carets are one pass), makes glyph cells on a worker thread
and packs them into one atlas texture that grows as glyphs arrive. The
atlas is one more entry in the layer's list - `createSpriteLayer([art,
font.atlas])` - so labels draw in the one draw with the sprites they
belong to and pan, zoom, sort (`orderBy`), pick and transition as
sprites; a run's pose is its group's (`run.group`), where glyph pointer
events bubble. Two cell kinds: "msdf" (the default: one atlas sharp at
every zoom, the fragment stage decoding `Atlas.sdf`; the per-run
`outline` draws only here) and "mask" (coverage at the face's exact size,
for 1:1 drawing). Text
as elements is still the right tool up to hundreds of labels: `d-text`
under a `<view>` carrying the camera transform (what `<TileLayer>` does
for its chunks) costs no JS per label on a camera move and is rasterized
at the real scale; runs are for thousands, for in-layer draw order, and
for a zoom animation that would re-rasterize every label per frame.

Layout is `layoutText` (text-layout.ts, pure): `\n` breaks, `maxWidth`
wraps with core's greedy rule, `align` per line, `anchor`
(start/middle/end, as d-text) and `anchorY` (top/middle/baseline/bottom)
place the run's point, `lineHeight` as named; `letterSpacing` is shaped
into the units by the engine. The run reports `width`, `height`, `ascent`
and `lines`. A run's `fontSize` defaults to the face's; glyphs scale their
cells by `fontSize / font.size`.

## Traps

- A run draws nothing for a glyph whose cell is not in the atlas yet:
  the sprite is there, hidden, at its place, and the font re-frames the
  run when the cell lands a frame or two later (`font.ready()` awaits
  that; `settle` does in a test). Printable ASCII is made at font
  creation so a run of it never waits; `chars: false` skips that.
- The atlas grows by doubling and EVERY cell moves when it does:
  `font.atlas.width`/`height` change in place and every run over the
  font is re-framed. Never cache a glyph's frame outside the font.
- A sprite font over "msdf" cells (the default) is sharp at every zoom;
  one over "mask" cells is exact at the face's size, resamples under zoom
  like any texture, and silently ignores `outline`.
- A run is node-layer only (it is a group); `addText` on a layer that
  does not declare `font.atlas` throws. Dispose runs before the font
  (`font.dispose`, or the owner's cleanup).
- The style record is 16 floats and the raw record 20: a raw record
  writer (`records(layer)`) that assumed 16 is off by the outline field
  per sprite.
