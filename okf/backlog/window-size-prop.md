---
title: A size prop on window
description: An app cannot choose its window size at start or change it while running; the start size is the `--size` flag (default 1280x720, fixed for a packed app) and `<window>` has only title, fullscreen and shader, so a `size` prop applied through a new platform command is the missing piece, with the start size best carried by the packed manifest to avoid a resize flash. Desktop-only.
created: 2026-11-11
---

# A size prop on window

## Symptom

The window size is a process-level setting, not an app one:

- At start: `--size <WxH>` ([entry.rs](../../lattice/src/entry.rs)),
  default `DEFAULT_WINDOW_SIZE` 1280x720. A packed app has no flag path, so
  it always opens at the default.
- While running: nothing. `WindowProps` has `title`, `fullscreen` and
  `shader` ([types.d.ts](../../packages/core/src/types.d.ts)).
  `windowSize()` and `onResize` are read-only. The only way the size changes
  is the user dragging the edge, the OS, or the `fullscreen` prop.
- `AlloyCommand` has no size command. `SetHeadlessWindow` exists but the
  windowed loop ignores it ([app.rs](../../alloy/src/app.rs)); it is the
  test host's resize ([stepped.rs](../../alloy/src/stepped.rs)).

An app with a natural window shape (a tool, a fixed-aspect game, a small
panel) opens at 1280x720 and cannot fix that.

## Done looks like

```tsx
<window title="Tool" size={{ width: 900, height: 600 }}>
```

On desktop the window follows the prop when it changes. `windowSize()`
reports the result through the normal resize path. On Android and the TV the
prop does nothing, because the app is the whole screen (same rule as
[widget windows](desktop-widget-window.md)).

The start size is a separate half (see "Initial size flash" below): the
suggestion is a size known before the window exists, not the prop.

## Involves

The `title`/`fullscreen` path, one more prop:

- `Window::set_size` in
  [kinds/window.rs](../../alloy/src/rendertree/kinds/window.rs): store the
  declared size, and send `AlloyCommand::SetSize { width, height }` only when
  it differs from the last declared one (see "Object props re-fire"). Return
  `Damage::None`, as title does: the size does not change until the
  `AlloyEvent::Resize` arrives, and that event already drives layout.
  (`set_fullscreen` returns `Damage::Layout` for the same reason and gains
  nothing by it; do not copy that.)
- The `"size"` arm in
  [properties/window.rs](../../flux/src/alloy_plugins/properties/window.rs):
  decode and validate (finite, positive, whole numbers; throw in dev).
- The platform loop applies it next to `SetFullscreen`
  ([app.rs](../../alloy/src/app.rs)) on ore's `Window`. The command is
  applied by alloy's loop, so the new size call is a `Window::set_size`
  method in [ore/src/window.rs](../../ore/src/window.rs) and one arm in
  alloy's loop. The resulting `AlloyEvent::Resize` feeds layout as it does
  for a user drag.
- `WindowProps` in types.d.ts and flux-types, the read-back in
  [properties/read.rs](../../flux/src/alloy_plugins/properties/read.rs), the
  docs and the core AGENTS.md.

## To settle before building

- **Initial size flash.** The SDL window is created before any JS runs, so a
  prop declared at mount arrives after the window is already shown at the
  default size: a visible resize at start. Options:
  1. Create the window hidden and show it on the first frame. This changes
     startup for every app.
  2. Accept the flash and say so.
  3. Know the size before the window exists. Suggested. The packed manifest
     ([manifest.rs](../../lattice/src/manifest.rs)) is read before
     `crate::start` and already carries identity (`display_name`, `icon`,
     `fonts`); the packed launch paths in
     [entry.rs](../../lattice/src/entry.rs) pass `DEFAULT_WINDOW_SIZE` right
     next to `payload.display_name`. A start size in the project config
     (the `"solidrt"` key that `loadAppIdentity` in
     [project.ts](../../packages/cli/src/lib/project.ts) reads), carried in
     the manifest, removes the flash for a packed app with no startup
     change; `--size` already covers a dev run. The start size then comes
     from the manifest or the flag, and the prop only changes it while
     running.
- **Centering.** The window is created with `position_centered()`
  ([ore/src/window/open.rs](../../ore/src/window/open.rs)). A resize applied
  after creation grows from the top-left corner on X11, Windows and macOS,
  so a prop at mount leaves the window off-center. Either re-center when
  the first size is applied, or let option 3 above make the question moot.
  Wayland does not apply: clients cannot position their windows there.
- **Prop versus `--size`.** Which wins when both are given. With option 3,
  `--size` and the manifest set the start and the prop only changes it
  later, so no precedence rule is needed. Without it: the prop wins once it
  is applied; `--size` stays the fallback and the dev override for tests
  and render.
- **Set, not enforce.** The command fires when the prop value changes. A
  user drag does not write back and is not fought. The prop is not a
  constraint.
- **Object props re-fire.** `size={{ width: w(), height: h() }}` builds a
  new object every time the expression re-runs. After a user drag, an
  unrelated re-run of that expression would snap the window back. So
  `set_size` compares with the last declared size and skips the command when
  it is equal; without that, "set, not enforce" does not hold.
- **Read-back.** Because the prop does not enforce, the declared and the
  actual size drift apart after a drag. The read-back reports the declared
  value; `windowSize()` reports the actual one. Say so, so nobody "fixes"
  one to match the other.
- **Units.** Logical window units (what `video.window(w, h)` takes on the
  high-pixel-density window), not physical pixels. The prop matches `run`'s
  `--size`, not `render`'s. The texts disagree because the semantics do:
  [usage.ts](../../packages/cli/src/lib/usage.ts) says "Window size" for
  `run` and "Frame size in physical pixels" for `render`, and
  [render/docs.md](../../packages/cli/src/render/docs.md) says physical.
  `render` really is physical; `run`'s usage text should say "logical".
- **Where the OS refuses.** A tiling compositor and a fullscreen or maximized
  window ignore or defer a size request. Settle whether a size set while
  fullscreen is stored and applied on leaving it, or dropped. SDL may
  already answer this: `SDL_SetWindowSize` records the request as pending
  state (SDL_video.c in sdl3-src 3.4.18), and the backends keep a windowed
  size they restore on leaving fullscreen. Check per backend before building
  our own store.
- **Headless.** Suggested: route the prop to the same resize path as
  `SetHeadlessWindow`, so an app test can check it, with a later
  `setWindow` winning (see
  [test-window-size-and-scale](../done/test-window-size-and-scale.md)).
  Ignoring it there leaves the feature untestable on the test host.
- **Min and max size.** Out of scope here; a natural follow-up prop pair if
  an app needs to bound the user's drag.
