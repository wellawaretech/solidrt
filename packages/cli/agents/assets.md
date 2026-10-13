# Assets, fonts, identity and distribution

Read this before adding an asset or a font, setting the app's identity, or
preparing a build for distribution.

## Assets and inlined imports

- Everything under `assets/` ships with the app: the folder is collected
  wholesale into each build's version manifest (no bundler analysis, no
  registration step). Reference assets by path - `file("assets/sounds/x.ogg")`
  from `flux:fs` - and treat them as read-only at runtime; writes belong in
  plain relative paths, which land in the app's private data dir.
- Small text-like assets (SVG documents, shaders) can instead be inlined via
  imports. An import attribute picks the form and works on any extension:
  `import src from "./effect.glsl" with { type: "text" }` yields the file's
  contents as a string, `with { type: "binary" }` yields a Uint8Array. `.svg`
  is text-loaded with no attribute needed. The types come from `@solidrt/cli`
  (its `assets.d.ts`, named in tsconfig `types` next to `@solidrt/flux-types`):
  the common extensions (`.svg`/`.glsl`/`.vert`/`.frag` as text, `.png`/`.jpg`/
  `.wav`/`.ogg`/`.glb`/`.wasm`/... as bytes) typecheck without setup, and any
  other extension gets a `declare module "*.ext"` of the same shape in a `.d.ts`
  of the project. Inlining trades update granularity for zero I/O - keep big or
  streamable files (audio, images) in `assets/`.
- `bun run sol bundle` writes `dist/bundle/` (or `--output <dir>`):
  `<name>.sol.js` plus the app's isolate modules as `isolates/<id>.js`; with
  `--compile`, bytecode (`.fluxbc`) instead. Move the dir, not the
  bare file - a bundle loaded without its isolates/ dir loses them
  (`--stdout` cannot carry them at all). Isolates (`"use isolate"` modules)
  exist in projects only, not for a file served on its own.
- `bun run sol bundle --json` is the dev server's rebuild contract: one JSON
  object (code, sourcemap, manifest, isolates) on stdout. Not for humans.

## Fonts, identity and distribution

- The default fonts are the runtime's: the stock runtime carries Noto Sans,
  Noto Serif and Noto Sans Mono under the role aliases `sans`/`serif`/`mono`,
  and a custom runtime sets its own in `runtime/src/lib.rs` (`fonts()`:
  drop a role, replace one with its file, add faces). Custom fonts of the
  app go in `assets/fonts/` and are declared in the `solidrt.fonts` map in
  package.json (alias -> file path replaces the runtime's font under that
  alias or adds one selectable via fontFamily; `false` drops the runtime's
  so that role resolves to `sans` like an unknown family; `true` is the
  runtime's). Only registered faces draw: there is no system font fallback.
  The dev client, `sol render` and a pack all apply the same map over the
  same runtime set, and a pack ships only the files the app binds: a
  default app packs no font. A `true`/`false` on an alias the runtime does
  not bind fails the command (the stock roles, or the `fonts.txt` a custom
  runtime's build lists); the client logs at startup which fonts it
  registered and where each came from. A newly added or changed font
  registers on the next reload.
- The `solidrt` key in package.json is the app's identity: set a stable
  reverse-DNS `appId` before distributing - it keys the app's storage
  folder, defaults from the package name in dev, and `sol pack` warns
  while defaulted. `org` and `displayName` are optional display metadata
  (future player/window naming) with no storage meaning.
- `bun run sol pack` builds a single-file executable (the runner with the
  bytecode, manifest, assets and fonts appended as a trailer);
  `bun run sol pack --folder` writes the flat app folder
  (runner + manifest.json + bundle + assets/, plus the runner's GL
  libraries on Windows and macOS) to `dist/pack/`.
- A packed app contains the SolidRT runtime, and with it third-party
  software whose licenses ask that their notices travel with every copy.
  They are in
  `node_modules/@solidrt/<platform>/THIRD-PARTY-NOTICES.txt` (one per
  platform package, generated for the binaries beside it). Ship that file
  with the app you distribute - next to the executable, or wherever the
  app shows its licenses - together with the notices of whatever npm
  packages the app itself bundles. `sol pack` does not add it for you.
