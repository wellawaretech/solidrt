# @solidrt/3d demos

Demos to run, not snippets to copy - `examples/` next door is the one-feature
set. List them with `bun run sol demo` and start one by its number.

This folder is ONE project: the demos share this package.json, this
tsconfig.json and this `assets/` folder. It does not ship with the package:
the CLI carries it pre-bundled (`make -C packages/cli demos` writes
`packages/cli/dist/demos/<package>/`), and `sol demo` serves that copy with
the chosen demo's bundle as its entry, on a dev server with a client. To
work on a demo here, `sol run src/<name>.tsx --project` from this folder,
and rebuild the shipped bundles when done.

Two rules hold it together:

- One file per demo. A demo is a single `src/<name>.tsx` - shaders and all -
  so it can be read top to bottom and copied out in one piece.
- One package per demo. A demo here uses @solidrt/3d and @solidrt/core, and
  nothing else: it shows what this package is for, on its own.
