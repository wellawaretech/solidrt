# Assets and inlined imports

Read this before adding an asset to an app. Fonts, the app identity and
distribution builds are in src/pack/agents.md.

- Everything under `assets/` ships with the app: the folder is collected
  wholesale into each build's version manifest (no bundler analysis, no
  registration step). Reference assets by path - `file("assets/sounds/x.ogg")`
  from `flux:fs` - and treat them as read-only at runtime; writes belong in
  plain relative paths, which land in the app's private data dir.
- Small text-like assets (SVG documents, shaders) can instead be inlined via
  imports. An import attribute picks the form and works on any extension:
  `import src from "./effect.glsl" with { type: "text" }` yields the file's
  contents as a string, `with { type: "binary" }` yields a Uint8Array. `.svg`
  is text-loaded with no attribute needed. Shader sources (`.glsl`/`.vert`/
  `.frag`) are declared as text modules out of the box, so they typecheck
  without setup. Inlining trades update granularity for zero I/O - keep big or
  streamable files (audio, images) in `assets/`.
- `bunx srt bundle` writes `dist/bundle/` (or `--output <dir>`):
  `<name>.srt.js` plus the app's isolate modules as `isolates/<id>.js`; with
  `--compile`, bytecode (`.srt.bin`/`.bin`) instead. Move the dir, not the
  bare file - a bundle loaded without its isolates/ dir loses them
  (`--stdout` cannot carry them at all). Isolates (`"use isolate"` modules)
  exist in projects only, not for a file served on its own.
- `bunx srt bundle --json` is the dev server's rebuild contract: one JSON
  object (code, sourcemap, manifest, isolates) on stdout. Not for humans.
