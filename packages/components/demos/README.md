# @solidrt/components demos

Demos to run, not snippets to copy - `examples/` next door is the one-feature
set. List them with `bunx srt demo` and start one by its number.

This folder is ONE project: the demos share this package.json, this
tsconfig.json and this `assets/` folder, and `srt demo` serves the project
with the chosen `src/*.tsx` as its entry.

Two rules hold it together:

- One file per demo. A demo is a single `src/<name>.tsx` so it can be read
  top to bottom and copied out in one piece.
- One package per demo. A demo here uses @solidrt/components and
  @solidrt/core, and nothing else: it shows what this package is for, on its
  own.

## Demos
- `gallery.tsx` - every component on one scrolling wall of cards, recoloring
  live on theme toggle, with the environment -> capabilities -> policies
  cascade exposed as overrides to watch each control adapt.
