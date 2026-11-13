---
title: An app cannot choose the size its window opens at
description: The start size is the `--size` flag (default 1280x720), which a packed app has no path to, so a start size in the project config, carried by the manifest and read before the window exists, is the missing piece, with no resize flash and no startup change.
created: 2026-11-13
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
- `Manifest` in manifest.rs (optional, `serde(default)`, so older manifests
  still load) and `FactoryPayload` in
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
