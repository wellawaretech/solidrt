# Assets and app identity

Read this before adding an asset, a font, or preparing a build for
distribution.

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
- Custom fonts go in `assets/fonts/` and are declared in the `solidrt.fonts`
  map in package.json (alias -> file path; role aliases `sans`/`serif`/`mono`
  replace the built-in defaults, `false` drops one, other keys add fonts
  selectable via fontFamily). A newly added font shows after restarting the
  client.
- The `solidrt` key in package.json is the app's identity: set a stable
  reverse-DNS `appId` before distributing - it keys the app's storage
  folder, defaults from the package name in dev, and `srt pack` warns
  while defaulted. `org` and `displayName` are optional display metadata
  (future launcher/window naming) with no storage meaning.
- `bunx srt pack src/index.tsx` builds a single-file executable;
  `bunx srt pack --folder src/index.tsx` writes the flat app folder
  (runner + manifest.json + bundle + assets/, plus the runner's GL
  libraries on Windows and macOS) to `dist/`.
