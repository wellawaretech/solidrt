---
title: An app cannot resize its window while running
description: `<window>` has only title, fullscreen and shader and no platform command sets a size, so a desktop app cannot change its window size after start; a `size` prop over a new `AlloyCommand::SetSize` is the sketch, a follow-up to the start size and waiting for an app that needs it.
created: 2026-11-11
---

# An app cannot resize its window while running

## Symptom

Nothing an app does changes its window size while it runs:

- `WindowProps` has `title`, `fullscreen` and `shader`
  ([types.d.ts](../../packages/core/src/types.d.ts)). `windowSize()` and
  `onResize` are read-only. The only way the size changes is the user
  dragging the edge, the OS, or the `fullscreen` prop.
- `AlloyCommand` has no size command. `SetHeadlessWindow` exists but the
  windowed loop ignores it ([app.rs](../../alloy/src/app.rs)); it is the
  test host's resize ([stepped.rs](../../alloy/src/stepped.rs)).

An app that changes shape while running (a compact mode, a panel that grows
when it opens a drawer) cannot ask for the window that fits.

The size a window opens at is a separate item,
[window-start-size](window-start-size.md), and it comes first: it fixes the
common case (an app with one natural shape) with none of the open questions
below. Build this one when an app needs to resize after start.

## Done looks like

```tsx
<window title="Tool" size={{ width: compact() ? 320 : 900, height: 600 }}>
```

On desktop the window follows the prop when its value changes. `windowSize()`
reports the result through the normal resize path. On Android and the TV the
prop does nothing, because the app is the whole screen (same rule as
[widget windows](desktop-widget-window.md)).

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
  ([app.rs](../../alloy/src/app.rs)) on ore's `Window`: a `Window::set_size`
  method in [ore/src/window.rs](../../ore/src/window.rs), in logical points
  (the conversion to SDL's window coordinates is settled in
  [window-start-size](window-start-size.md), "Units"), and one arm in
  alloy's loop. The resulting `AlloyEvent::Resize` feeds layout as it does
  for a user drag.
- `WindowProps` in types.d.ts and flux-types, the read-back in
  [properties/read.rs](../../flux/src/alloy_plugins/properties/read.rs), the
  docs and the core AGENTS.md.
- The same command serves the control API resize that
  [tiny.md](../tiny.md) asks for (`/window?width=&height=`).

## To settle before building

- **Prop or call.** A prop is declarative, and a window size is not a
  state the app owns: the user drags it. The consequences are the next
  four bullets. An imperative `setWindowSize(w, h)` has none of them (it
  fires when called, and "reset to my size" after a drag just works), at
  the cost of not sitting next to `title` and `fullscreen`. Decide this
  first.
- **Set, not enforce.** The command fires when the prop value changes. A
  user drag does not write back and is not fought. The prop is not a
  constraint.
- **Object props re-fire.** `size={{ width: w(), height: h() }}` builds a
  new object every time the expression re-runs. After a user drag, a re-run
  that yields the same numbers would snap the window back. So `set_size`
  compares with the last declared size and skips the command when it is
  equal; without that, "set, not enforce" does not hold. The other side of
  the same rule: an app cannot put a dragged window back to the declared
  size, because the value did not change.
- **The first value.** A prop declared at mount differs from "nothing
  declared", so it fires at start: the resize flash and the off-center
  window that the start size item exists to avoid, whenever the prop and
  the start size disagree. Seed the last declared size with the size the
  window opened at, so a mount-time prop equal to it sends nothing.
- **Reload.** Each engine builds a fresh root window
  (`Window::default()` in
  [tree.rs](../../flux/src/alloy_plugins/tree.rs)), so the last declared
  size is lost on a hot reload and the prop fires again: a window the user
  dragged snaps back on every save. Either accept that, or keep the last
  declared size where it outlives the engine (the platform loop, next to
  the window).
- **Removing the prop.** A null write. Suggested: it does nothing (the
  window keeps its size) and clears the last declared size.
- **Read-back.** Because the prop does not enforce, the declared and the
  actual size drift apart after a drag. The read-back reports the declared
  value; `windowSize()` reports the actual one. Say so, so nobody "fixes"
  one to match the other.
- **Centering.** A resize after creation grows from the top-left corner on
  X11, Windows and macOS. For a change while running that is what the user
  expects; do not re-center.
- **Where the OS refuses.** A tiling compositor and a fullscreen or maximized
  window ignore or defer a size request. Settle whether a size set while
  fullscreen is stored and applied on leaving it, or dropped. SDL may
  already answer this: `SDL_SetWindowSize` records the request as pending
  state (SDL_video.c in sdl3-src 3.4.18), and the backends keep a windowed
  size they restore on leaving fullscreen. Check per backend before building
  our own store.
- **Headless.** The test host's size wins. `setWindow` is only accepted
  before a test's first frame
  ([lattice stepped.rs](../../lattice/src/stepped.rs); see
  [test-window-size-and-scale](../done/test-window-size-and-scale.md)), and
  a prop applied at mount lands after it, so routing the prop to the
  `SetHeadlessWindow` path would override the size the test chose in every
  test of an app that declares one. The render host runs on the same
  stepped loop, and a size change there changes the frame size in the
  middle of a render. Suggested: the stepped loop ignores `SetSize`, and a
  test checks the prop through the read-back and through the command the
  element sent, not through a resized surface.
- **Min and max size.** Out of scope here; a natural follow-up prop pair if
  an app needs to bound the user's drag.
