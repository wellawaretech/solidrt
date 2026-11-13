---
title: An app cannot choose the size its window opens at
description: The start size is the `--size` flag (default 1280x720), which a packed app has no path to, so a start size in the project config, carried by the manifest and read before the window exists, is the missing piece, with no resize flash and no startup change.
created: 2026-11-13
completed: 2026-11-13
---

# An app cannot choose the size its window opens at

## Symptom

The size a window opens at is a process-level setting, not an app one:
`--size <WxH>` ([entry.rs](../../lattice/src/entry.rs)), default
`DEFAULT_WINDOW_SIZE` 1280x720. The three packed launch paths in entry.rs
pass `DEFAULT_WINDOW_SIZE` and have no flag path, so a packed app always
opens at the default.

An app with a natural window shape (a tool, a fixed-aspect game, a small
panel) opens at 1280x720 and cannot fix that.

Resizing while the app runs is a different need and a separate item:
[window-size-prop](window-size-prop.md). This item fixes the symptom above
without it.

## Done looks like

```json
"solidrt": { "windowSize": { "width": 900, "height": 600 } }
```

A packed app opens at that size, centered, in its first shown frame: no
flash, no resize after start. `windowSize()` reports 900x600 on every desktop
platform and every display scale. `sol run` opens the local client at the
same size with no flag typed. An explicit `--size` still wins. On Android and
the TV the key does nothing, because the app is the whole screen.

## Why the manifest and not a prop

The SDL window is created before any JS runs, so a `<window>` prop declared
at mount arrives after the window is already shown at the default size: a
visible resize at start, and an off-center window on X11, Windows and macOS
(a resize after creation grows from the top-left corner). The alternatives
were to create the window hidden and show it on the first frame (changes
startup for every app) or to accept the flash.

The packed manifest ([manifest.rs](../../lattice/src/manifest.rs)) is read
before `crate::start` and already carries identity (`displayName`, `icon`,
`fonts`); the packed launch paths pass `DEFAULT_WINDOW_SIZE` right next to
`payload.display_name`. A size there is known before the window exists, so
the window is created at it and `position_centered()`
([ore/src/window/open.rs](../../ore/src/window/open.rs)) still centers it.

## Involves

- `ProjectConfig` in
  [project.ts](../../packages/cli/src/lib/project.ts): the new key,
  shape-checked like the others (finite, positive, whole numbers), written
  into the manifest next to `displayName`.
- `Manifest` in manifest.rs (optional: an app may declare none) and
  `FactoryPayload` in
  [payload.rs](../../lattice/src/payload.rs), next to `display_name`.
- entry.rs: the three packed paths use the payload's size and fall back to
  `DEFAULT_WINDOW_SIZE`. The folder boot (`Manifest::load`, where it already
  takes `display_name`) uses the manifest's size unless `--size` was given.
- `sol run`: [server/main.ts](../../packages/cli/src/server/main.ts)
  already forwards `--size` to the local client it launches; without the
  flag it forwards the project config's size. Otherwise a dev run and the
  packed app open at different sizes.
- The `solidrt` key's docs and the scaffold.

## To settle before building

- **Units.** The key must mean the size `windowSize()` reports, in logical
  points. That is not what the window is created with today on every
  platform. [event.rs](../../alloy/src/event.rs) (`resize_event`) computes
  the logical size as the pixel size over the display scale. SDL's
  README-highdpi says window coordinates are logical on macOS and Wayland
  but physical pixels on Windows and X11, where high DPI is a content scale
  only. So a 900x600 window at 200% on X11 or Windows reads back as
  450x300. This is from SDL's document and our code, not measured: check it
  on a scaled X11 or Windows display first. `--size` has the same gap
  today.

  The conversion belongs in ore: the window contract takes logical points.
  After creation the factor is the window's display scale over its pixel
  density (1 on macOS and Wayland, the content scale on X11 and Windows).
  Before creation there is no window to ask, only the display's content
  scale, and whether that gives the same factor is per driver. Settle
  whether ore computes the size before creating the window, or creates it
  hidden, corrects it and shows it.
- **`--size` units.** Once ore takes logical points, `run`'s `--size` is
  logical too, and its usage text
  ([usage.ts](../../packages/cli/src/lib/usage.ts), "Window size") should
  say so. `render`'s `--size` is physical and says so; that stays.
- **Tests and render.** Suggested: the key does not apply there. The test
  host and the render host take their size from their own `--size`, and an
  app test keeps the size the test chose.
- **The go client.** An app installed in the player runs in the player's
  window, which exists before the app's manifest is read. Suggested: the
  key does nothing there; resizing an existing window is the other item.
- **The key's name and shape.** `windowSize: { width, height }` matches
  `windowSize()`. A `"900x600"` string would match the flag instead.
- **A window larger than the display.** Check what SDL and the compositors
  do with a start size that does not fit, and say so in the docs rather
  than clamp ourselves, unless the result is a window the user cannot
  reach.

## Decisions

Each open question from the shaping, settled:

- **Units: logical points, converted in ore at creation.** `WindowConfig`'s
  `width` and `height` are logical points, the unit `windowSize()` reports
  and `Window::set_size` takes; alloy and lattice pass them through
  unchanged. Before the window exists there is no window to ask, so ore
  estimates the factor from two facts it has at that moment: the video
  driver's name and the primary display's content scale
  (`SDL_GetDisplayContentScale`). On the drivers whose window coordinates
  are physical (`x11`, `windows`; SDL's README-highdpi) the factor is the
  content scale; everywhere else (macOS, Wayland, the headless drivers,
  Android) it is 1. A pure `start_units(logical, driver, content_scale)`
  next to `window_units`, unit tested. Once the window exists, the actual
  factor (display scale over pixel density, the `window_units` formula) is
  compared with the estimate; when they differ the window is resized
  through `set_size` and re-centered, with a warning naming the driver,
  because the driver table was wrong, not because a compositor chose
  another size (Hyprland reports every window maximized; a size compare
  would fire on every start there, a factor compare does not). The
  estimate is applied for the interactive window only: a headless window's
  size is the frame size the host asked for and its surface never matters
  after setup.
- **Not hidden-then-shown.** Creating the window hidden, correcting it and
  showing it would need a second centering pass on X11 and Windows (a
  resize grows from the top-left corner) and changes the open sequence on
  every driver for a correction only two drivers need. The estimate gives
  the right size in the first shown frame on the drivers where SDL's own
  contract says what the factor is; the post-creation check catches the
  rest.
- **`--size` is logical too.** The same `WindowConfig`, so `run`'s and
  `client`'s `--size` mean logical points from now on; the usage text says
  so. `render --size` is physical, headless, and unchanged (factor 1 on the
  offscreen driver).
- **The key: `windowSize: { width, height }`.** Whole numbers of at least 1,
  checked in `parseProjectConfig` like every other key; an unknown key
  inside the object fails like one at the top level; null unsets. No
  maximum and no clamp: a size larger than the display goes to the OS as
  asked, and the window manager decides (the docs say so).
- **The manifest carries it** as `"windowSize": {"width","height"}` next to
  `displayName`, omitted when the project declares none. lattice reads it
  as `Option<WindowSize>`; the CLI checked the shape, so lattice does not
  check it again.
- **Where the size applies.** The desktop packed paths (trailer, .solapp,
  adjacent folder) and the `--assets` folder boot of the interactive
  runner, when `--size` was not typed. Android's packed path keeps
  `DEFAULT_WINDOW_SIZE`: the app is the whole screen, and the key does
  nothing there by construction. The test host and the render host keep
  their own `--size`. The go player's window exists before an installed
  app's manifest is read; the key does nothing there, and the `<window
  size>` prop is the resize for a running window.
- **`sol run` and `sol client` default to the project's size.** Without
  `--size`, the local client gets `--size WxH` from the project's config
  (the server for `run`, the command itself for `client`), so a dev run and
  the packed app open at the same size. A file run has no project and keeps
  the default. The shape rules live in a pure module, `lib/window-size.ts`
  (the fonts.ts pattern), because project.ts is bun-only (node:fs, Bun,
  a spawn at import) and the dev server is a flux script with its own
  reader of the key (server/mode.ts, `declaredConfig`); the test runs on
  flux against the pure module.
- **Two manifest writers.** The dev manifest (`buildManifest`) and the pack
  folder's (pack/layout.ts) each spell out the identity block; the size
  entry is one shared helper, `manifestWindowSize`, so the two cannot
  drift. Found when the first packed run opened at the default.
- **No start flash.** The window is created at the size, centered by
  `position_centered()` as before. alloy's `declared_size` is seeded with
  the open size, so an app that also declares `<window size>` with the
  same numbers sends nothing.

## Work packages

Both run in parallel: they touch disjoint files and meet on the manifest
JSON (`"windowSize": { "width": <int>, "height": <int> }`).

### 1. Rust: ore's conversion, the manifest, the payload, the entry

- [ore/src/window.rs](../../ore/src/window.rs): `WindowConfig`'s doc says
  `width` and `height` are logical points. `pub(crate) fn
  start_units(logical: (u32, u32), driver: &str, content_scale: f32) ->
  (u32, u32)` next to `window_units`: a `PHYSICAL_WINDOW_COORDINATE_DRIVERS`
  constant lists `x11` and `windows`; on those the size is multiplied by
  the content scale and rounded to whole units, elsewhere returned as is;
  a non-positive content scale means a factor of 1. Tests in
  [ore/src/tests/window.rs](../../ore/src/tests/window.rs): x11 at 2.0
  doubles, wayland at 2.0 keeps, an unknown driver keeps, the guard.
- [ore/src/window/open.rs](../../ore/src/window/open.rs), `open_gl`: for
  the interactive window, the creation size is `start_units` of the config
  size with the driver name and the primary display's content scale
  (`get_primary_display()?.get_content_scale()`, a failure logged at debug
  and treated as 1). After `Window::new`, compare `window_units(config
  size, display_scale, pixel_density)` with the units used; when they
  differ, `set_size` to the config size, re-center through the sdl3
  window's `set_position(WindowPos::Centered, WindowPos::Centered)`, and
  warn with the driver name and both factors. One info line on every open:
  the logical size, the units and the factor (the verification hook for
  this plan and for future scale bugs).
- [lattice/src/manifest.rs](../../lattice/src/manifest.rs): `#[derive(Deserialize, Clone, Copy)] pub struct
  WindowSize { pub width: u32, pub height: u32 }`; `#[serde(default,
  rename = "windowSize")] pub window_size: Option<WindowSize>` on
  `Manifest`; `pub fn window_size(&self) -> Option<(u32, u32)>`. Tests in a
  new `lattice/src/tests/manifest.rs` (registered in tests/mod.rs): a
  manifest with the key parses to the pair, one without parses to None.
- [lattice/src/payload.rs](../../lattice/src/payload.rs): `pub window_size:
  Option<(u32, u32)>` on `FactoryPayload`, next to `display_name`, filled
  from `manifest.window_size()` in `load` and `load_adjacent_folder`.
- [lattice/src/entry.rs](../../lattice/src/entry.rs): the desktop packed
  path passes `payload.window_size.unwrap_or(DEFAULT_WINDOW_SIZE)`; the
  Android packed path keeps `DEFAULT_WINDOW_SIZE` with a comment. The flag
  parser's `size` becomes `Option<(u32, u32)>` (None until `--size`); the
  test and render hosts take `size.unwrap_or(DEFAULT_WINDOW_SIZE)`; the
  `--assets` manifest block takes the manifest's size into a local when no
  `--size` was typed, and the interactive `crate::start` gets
  `size.or(manifest_size).unwrap_or(DEFAULT_WINDOW_SIZE)`. The
  `DEFAULT_WINDOW_SIZE` doc names the manifest as the third source.
- Build check: `cargo test -p ore --features window`, `cargo test -p
  lattice --lib`, `cargo build -p alloy` (the `WindowConfig` fields are
  unchanged, so alloy needs no edit).

### 2. CLI, docs and the scaffold

- [project.ts](../../packages/cli/src/lib/project.ts): the header comment's
  key list gains `"windowSize": { "width": 900, "height": 600 }` (the size
  a desktop window opens at, logical points); `windowSize?: { width:
  number; height: number }` on `ProjectConfig`; `"windowSize"` in
  `PROJECT_KEYS`; `export function checkWindowSize(raw: unknown): string |
  null` (the fonts.ts pattern: an object with exactly `width` and `height`,
  each a whole number of at least 1; messages in the `"solidrt":
  "windowSize" ...` style naming the field and the received value), called
  from `parseProjectConfig`; `export function windowSizeFlag(config:
  ProjectConfig): string | undefined` returning `"900x600"`;
  `buildManifest` spreads `...(windowSize ? { windowSize } : {})` right
  after `displayName`.
- [server/main.ts](../../packages/cli/src/server/main.ts): the client
  args take `args.size ?? windowSizeFlag(loadProject(mode.projectDir).config)`
  when the mode has a project (loadProject takes a dir; the file mode
  passes nothing).
- [client/main.ts](../../packages/cli/src/client/main.ts): the same
  default from the project in the current directory when `--size` is not
  given.
- [usage.ts](../../packages/cli/src/lib/usage.ts): run/client `--size`:
  "Window size in logical points (default: the project's windowSize, else
  1280x720)". `render`'s line stays.
- [scaffold/package.json](../../packages/cli/src/init/scaffold/package.json):
  `"windowSize": null` after `"displayName"`.
- [pack/docs.md](../../packages/cli/src/pack/docs.md): the key in the
  JSON block and a paragraph: the size the desktop window opens at, in
  logical points (the unit `windowSize()` reports), the same at every
  display scale; `sol run` opens the local client at it and `--size`
  wins; Android and the TV ignore it; not clamped: a size larger than the
  display is passed to the OS as asked and the window manager decides.
- [server/docs.md](../../packages/cli/src/server/docs.md): one sentence
  under `run`: the local client opens at the project's `windowSize`, or
  `--size`.
- [packages/core/AGENTS.md](../../packages/core/AGENTS.md): one sentence
  after the `<window size>` paragraph: the size a window opens at is the
  project's `windowSize` key in package.json; the prop is for a change
  while running.
- A new `packages/cli/tests/project-window-size.test.ts` (flux:test, the
  project-fonts.test.ts shape): valid sizes pass; a missing side, a
  fraction, zero, a negative, a string, an array and an unknown key are
  refused with messages naming the field; `windowSizeFlag` renders
  "900x600" and is undefined without the key.
- Check: `bun run sol check packages/cli/tests/project-window-size.test.ts`
  and `bun run sol test packages/cli/tests/project-window-size.test.ts`.

## Findings

- Verified on the live client (Wayland, Hyprland 0.56, scale 1): `sol run`
  in a project with `"windowSize": { "width": 900, "height": 600 }` started
  the client with `--size 900x600`; ore logged `window: 900x600 logical
  points, 900x600 units (factor 1 on wayland)`; the tree root read
  900x600. `sol pack --folder` wrote `"windowSize":{"width":900,"height":600}`
  into the manifest and the packed runner opened at it (same log line).
- A floating toplevel on Hyprland keeps the size it was created with: the
  compositor flags it maximized (the upstream record) but maps it at the
  client's initial size, so the start size works there even though a
  later resize request does not.
- The X11 and Windows start factor (the content scale) is from SDL's
  README-highdpi, not measured: XWayland on this desktop cannot create a
  GLES window surface. The post-creation check in ore's `open_gl` warns and
  corrects if the table is wrong, so a scaled X11 or Windows run reports it
  in its log.

