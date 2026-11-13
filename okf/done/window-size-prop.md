---
title: An app cannot resize its window while running
description: `<window>` has only title, fullscreen and shader and no platform command sets a size, so a desktop app cannot change its window size after start; a `size` prop over a new alloy command, deduplicated in the platform loop, plus the control API resize that shares the ore call.
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
[window-start-size](../backlog/window-start-size.md). This item does not
wait for it: the dedupe below makes a mount-time declaration equal to the
open size a no-op, and a different one is the resize the app asked for.

## Done looks like

```tsx
<window title="Tool" size={{ width: compact() ? 320 : 900, height: 600 }}>
```

On desktop the window follows the prop when its value changes. `windowSize()`
reports the result through the normal resize path. On Android and the TV the
prop does nothing, because the app is the whole screen (same rule as
[widget windows](../backlog/desktop-widget-window.md)).

A dev tool can resize the window too: `POST /window?width=&height=` on the
control API and the `set_window_size` MCP tool, which [tiny.md](../tiny.md)
asked for so the resize and display-scale paths can be probed.

## Decisions

Each open question from the shaping, settled:

- **Prop, not a call.** It sits next to `title` and `fullscreen` and is
  read back like them. The consequences below are handled by one rule: the
  platform loop remembers the last declared size and applies a declaration
  only when it differs.
- **Set, not enforce.** A user drag does not write back and is not fought.
  The prop is not a constraint; min and max size stay out of scope.
- **Where the memory lives.** The platform loop in alloy's `app.rs`, next
  to the window, not the element: it outlives the engine. One place settles
  three questions at once. Object props re-fire (a re-run yielding the same
  numbers sends the same declaration, the loop skips it). The first value
  (the memory is seeded with the size the window opened at, so a mount-time
  declaration equal to it sends nothing and there is no start flash). Reload
  (a fresh engine re-declares the same size, the loop skips it, a window the
  user dragged stays where it is). The other side of the rule stands: an
  app cannot put a dragged window back to the declared size, because the
  value did not change.
- **Removing the prop.** A null write sends an empty declaration. The
  window keeps its size and the loop forgets the declared one, so the next
  declaration always applies: the value did change.
- **Two commands, one ore call.** `AlloyCommand::DeclareSize(Option<(u32,
  u32)>)` is the app's declaration, deduplicated. `AlloyCommand::ResizeWindow
  { width, height }` is the dev tool's resize: it applies unconditionally
  and leaves the declared size alone, because it stands in for the user's
  drag. Both reach `ore::window::Window::set_size`.
- **Units.** `Window::set_size` takes logical points, the unit `windowSize()`
  reports. SDL's window coordinates are logical on macOS and Wayland and
  physical on X11 and Windows (README-highdpi), so ore converts: window
  units = logical * display scale / pixel density, which is 1 where the
  coordinates are already logical and the content scale on X11 and
  Windows. From SDL's document, not measured on a scaled X11 display: say
  so in the method's doc.
- **Where the OS refuses.** Nothing of our own. `SDL_SetWindowSize`
  (SDL_video.c, sdl3-src 3.4.18) records the request as the window's
  pending size and the backends apply it when the window leaves fullscreen
  or maximized; a backend without `SetWindowSize` (Android) returns
  unsupported and sends no event; a tiling compositor may ignore or defer
  it. ore returns SDL's error. The sdl3 crate's `set_size` drops the
  result, so ore calls `SDL_SetWindowSize` through `sdl3::sys` and reads
  `sdl_error()` on false.
- **Android and the TV.** No `cfg`: SDL's unsupported answer is the fact.
  alloy logs a declaration the platform refused at debug (an app that
  declares a size runs there too), a dev-tool resize that failed at warn.
- **Centering.** Not touched: a resize while running grows from the
  top-left corner, which is what the user expects.
- **Read-back.** `size` reads back the declared value; `windowSize()`
  reports the actual one. They drift apart after a drag by design.
- **Headless.** The stepped loop ignores both commands (its catch-all arm):
  the test host's size wins, and a size change in the middle of a render
  would change the frame size. A test checks the prop through the read-back
  and the command the element sent.
- **Damage.** `Damage::None`: the size changes when `AlloyEvent::Resize`
  arrives, and that event already drives layout.

## Work packages

Package 1 lands first; 2 and 3 run in parallel after it and touch disjoint
files.

### 1. ore and alloy: the call, the commands, the loop, the element

- [ore/src/window.rs](../../ore/src/window.rs): `pub fn set_size(&mut self,
  width: u32, height: u32) -> Result<(), String>` next to `set_fullscreen`,
  converting through a pure `pub(crate) fn window_units(logical: (u32, u32),
  display_scale: f32, pixel_density: f32) -> (u32, u32)` (a non-positive
  scale or density means a factor of 1; round to whole units). The pixel
  density is the sdl3 window's `pixel_density()`. Unit tests in
  [ore/src/tests/window.rs](../../ore/src/tests/window.rs): factor 1
  (2.0 over 2.0), factor 2 (2.0 over 1.0), the guard. The module doc's
  command list gains "size".
- [alloy/src/event.rs](../../alloy/src/event.rs): the two variants with
  their doc comments (above). The logical size at open comes from the same
  formula `resize_event` uses, in one helper so the two cannot disagree.
- `alloy/src/window_size.rs` (new, registered in lib.rs): `pub(crate) fn
  size_to_apply(declared: &mut Option<(u32, u32)>, request: Option<(u32,
  u32)>) -> Option<(u32, u32)>`, the dedupe rule, with its test in
  `alloy/src/tests/window_size.rs`: same size skipped, a different size
  applied and remembered, None forgets and the next declaration applies,
  the seed makes a mount-time declaration equal to the open size a no-op.
- [alloy/src/app.rs](../../alloy/src/app.rs): `declared_size` seeded with
  the window's logical size at open; the `DeclareSize` arm through
  `size_to_apply` (refusal at debug), the `ResizeWindow` arm unconditional
  (refusal at warn). The stepped loop's catch-all already ignores both; a
  comment there names them.
- [kinds/window.rs](../../alloy/src/rendertree/kinds/window.rs): `pub size:
  Option<(u32, u32)>` (None in `Default`), `set_size(&mut self, size:
  Option<(u32, u32)>, cmd_tx) -> Damage` that stores, sends `DeclareSize`
  and returns `Damage::None`; the comment above `set_title` gains the size.

### 2. flux and core: the prop, the read-back, the types, the docs

- [properties/window.rs](../../flux/src/alloy_plugins/properties/window.rs):
  the `"size"` arm. Null clears. Otherwise a map whose `width` and `height`
  are finite, positive, whole numbers; the error names the field and the
  received value like the shader fields do.
- [properties/read.rs](../../flux/src/alloy_plugins/properties/read.rs):
  `("size", ReadValue::Nums(vec![w, h]))` when declared.
- [flux/src/tests/properties.rs](../../flux/src/tests/properties.rs): a
  valid size applies with `Damage::None` and `DeclareSize(Some(..))` arrives
  on the command channel; null sends `DeclareSize(None)` and clears the
  read-back; a fractional, zero, negative, non-finite or non-numeric field
  is an Err naming the field; the read-back carries the declared pair.
- [types.d.ts](../../packages/core/src/types.d.ts): `size?: { width:
  number; height: number } | null` on `WindowProps`, with the contract in
  its doc comment: logical points; desktop only; applied when the value
  changes; not a constraint (a drag is not fought, the same value declared
  again does nothing); the read-back is the declared size and `windowSize()`
  the actual one; ignored by the test host.
- [packages/core/AGENTS.md](../../packages/core/AGENTS.md): a short
  paragraph next to the `windowSize()` guidance with the same contract.

### 3. lattice and the CLI: the control API resize

- [lattice/src/go/connection.rs](../../lattice/src/go/connection.rs): a
  `"window"` query (in `QUERY_KINDS` too): `width` and `height` as positive
  whole numbers, else an error reply naming the field; sends
  `AlloyCommand::ResizeWindow` over `flags.alloy_cmd_tx`; replies with the
  requested pair. The resize event drives the frame, so no latch.
- [control.ts](../../packages/cli/src/server/control.ts): `POST
  /__control__/window?width=&height=` (in `CONTROL_ENDPOINTS`), shape
  checks with a 400 that teaches the args, forwarded as the `window` query.
- [mcp/main.ts](../../packages/cli/src/mcp/main.ts): `set_window_size`
  (width, height, client), posting to `/window`. The description says the
  request is in logical points, applies on desktop only, that a tiling
  compositor or a fullscreen window may ignore or defer it, and that the
  result is read back from the tree root's size after `settle`, never
  assumed.
- [debugging.md](../../packages/cli/agents/debugging.md): the endpoint in
  the control API list and the tool in the MCP list.
- [tiny.md](../tiny.md): delete the `/window?width=&height=` line.

## Findings

- Hyprland reports every window as maximized, so SDL never applies a
  client-side size there: [hyprland-maximized-state-on-every-toplevel](../upstream/hyprland-maximized-state-on-every-toplevel.md).
- Verified on the live client through a temporary log in ore's `set_size`:
  the mount-time declaration (different from the open size) reached SDL
  once in the right units; a re-fired object with the same numbers sent
  nothing; a null write forgot the size and the next declaration applied;
  the first reload after a change applied, the second sent nothing; the
  control API resize reached SDL unconditionally. The resize itself could
  not be observed on this desktop (the note above), and the X11 and
  Windows unit factor stays unmeasured.
- The opt-in `SOLIDRT_RESIZE_VIA_LIMITS=1` (ore's `set_size`) pins the
  window's min and max size to the request so Hyprland applies it, and
  lifts them once the resize lands. The upstream record has the details.
