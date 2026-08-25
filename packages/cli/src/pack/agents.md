# App identity, fonts and distribution

Read this before adding a font or preparing a build for distribution. The
assets/ folder and inlined imports are in src/bundle/agents.md.

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
- `bunx srt pack` builds a single-file executable (the runner with the
  bytecode, manifest, assets and fonts appended as a trailer);
  `bunx srt pack --folder` writes the flat app folder
  (runner + manifest.json + bundle + assets/, plus the runner's GL
  libraries on Windows and macOS) to `dist/pack/`.
