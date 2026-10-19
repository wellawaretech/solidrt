---
title: Widget-style desktop windows
description: An app cannot make its window transparent, undecorated or always on top, or mark a drag-to-move region, so a desktop widget (a character, a clock, a floating panel) runs in an ordinary framed window; `<window>` has only title, fullscreen and shader. Desktop-only.
created: 2026-10-19
---

# Widget-style desktop windows

A desktop widget is a small window with no frame, a see-through background
where the app draws nothing, kept above other windows, and moved by
dragging its content. `<window>` has `title`, `fullscreen` and `shader`
only ([types.d.ts](../../packages/core/src/types.d.ts) `WindowProps`), so
such an app runs in an ordinary framed window with a painted backdrop.

Done looks like: an app declares the four things below, and on Windows,
macOS and Linux (Wayland and X11) the window shows them. On Android and the
TV they do nothing, because there the app is the whole screen.

## The four asks, and what each costs

**Undecorated** and **always on top** can change while the window is open:
`SDL_SetWindowBordered` (the sdl3 crate's `Window::set_bordered`) and
`SDL_SetWindowAlwaysOnTop` (missing from the sdl3 crate, so a wrapper goes
in `alloy/src/sdl_utils.rs`). They are plain `<window>` props on the
`title`/`fullscreen` path: the element sends an `AlloyCommand`
([kinds/window.rs](../../alloy/src/rendertree/kinds/window.rs)) and the
platform loop applies it ([app.rs](../../alloy/src/app.rs), `SetFullscreen`).

**Drag-to-move region**: on Wayland only the compositor can move a
window, and only when it is asked during a pointer press. So the portable
route is SDL's hit test (`SDL_SetWindowHitTest`, the sdl3 crate's
`set_hit_test`) answering `DRAGGABLE` for points in the region. The
callback runs on the platform thread and must answer at once, so it cannot
call JS or walk the tree. Instead, layout publishes the region's window
rects and the callback reads them. How an app marks the region is open:
a prop on an element (like CSS `-webkit-app-region: drag`, with a
`no-drag` opt-out for buttons inside it) fits the tree model.

**Transparent** is the hard one, because the window must be created that
way:

- `SDL_WINDOW_TRANSPARENT` exists only as a creation flag (set it with
  `WindowBuilder::set_window_flags`), and the GL config needs alpha bits.
  Today only Android requests alpha
  ([gl/context.rs](../../alloy/src/gl/context.rs), `set_alpha_size`).
- The window is created in `setup_video` before any JS runs. In the dev
  player one window serves the launcher and every app it loads. So an app's
  declaration either goes in the package.json `solidrt` config, read
  before the window is made (a packed app), or the runtime recreates the
  window when an app asks for it. The context-loss recovery already
  rebuilds the window and its GL context (`RECOVERY_RECREATES_WINDOW`,
  app.rs) and could be the mechanism. Every recreation path (that one, and
  the single-sample retry in `setup_video`) must keep the flags.
- Every path to the screen must keep alpha. The frame draw clears to
  transparent black, but a window shader pass clears to opaque black
  first ([gl/pass.rs](../../alloy/src/gl/pass.rs)). The offscreen rig
  resolve must also keep alpha, and the pixels it writes must be
  premultiplied the way the compositor expects.
- Per platform: Wayland honors an alpha EGL config, and on X11 SDL picks
  an ARGB visual. Windows (ANGLE on D3D11) and macOS (ANGLE on Metal) are
  unproven. A DXGI swap chain may ignore the alpha unless it goes through
  DirectComposition, and a Metal layer must be non-opaque. Check both on a
  probe before designing the API.

A widget usually also wants a fixed size, and an app cannot declare its
window size today either. That is related but not part of this item. A
resize command to the platform loop would also serve the headless side,
where a test cannot choose its surface size
([test-window-size-and-scale](../done/test-window-size-and-scale.md)).
