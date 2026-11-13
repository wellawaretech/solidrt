---
title: Hyprland tells every toplevel it is maximized, so a window can never resize itself
description: Hyprland sends the xdg maximized state (and all four tiled states) to every toplevel at first map to keep clients from drawing their own decorations, with no option or rule to turn it off; SDL then holds SDL_SetWindowSize as a pending size that never applies, so the `<window size>` prop and the control API resize do nothing on Hyprland unless the client runs with SOLIDRT_RESIZE_VIA_LIMITS=1, which pins the window's min and max size to the request. Known upstream since 2023 and kept on purpose.
project: Hyprland (github.com/hyprwm/Hyprland)
versions: Hyprland 0.56.2 (verified here 2026-11-13; src/protocols/XDGShell.cpp, the surfaceCommit listener); introduced by commit ae46fba "xdg: set state maximized for all tiled windows" (2023-11-11, for #3830)
status: unfiled
link:
created: 2026-11-13
---

# Hyprland tells every toplevel it is maximized, so a window can never resize itself

Found while verifying the `<window size>` prop
([window-size-prop](../done/window-size-prop.md)): the command reached SDL
with the right units, `SDL_SetWindowSize` returned true, and nothing
happened. The flags SDL reported for a floating, non-fullscreen window were
`OPENGL | RESIZABLE | MAXIMIZED | HIGH_PIXEL_DENSITY`.

## What Hyprland does

In `src/protocols/XDGShell.cpp` (v0.56.2) the surface commit listener runs
this on the commit that maps a toplevel, floating or tiled:

```
// this forces apps to not draw CSD.
if (m_toplevel)
    m_toplevel->setMaximized(true);
```

No configuration variable, window rule or xdg-decoration check guards it.
The toplevel constructor also pushes all four `XDG_TOPLEVEL_STATE_TILED_*`
states for protocol version 2 and up. A client's `unset_maximized` request
only raises an event; the state stays.

The protocol reads the other way: `maximized` means "you are maximized,
take the configured size", and the tool for suppressing client-side
decorations is xdg-decoration's server-side mode, which Hyprland already
implements.

## What SDL does with that

`SDL_SetWindowSize` (SDL_video.c, sdl3-src 3.4.18) records the request as
the window's pending size, and the Wayland backend applies it only when the
window is neither fullscreen nor maximized (`Wayland_SetWindowSize`:
"Maximized and fullscreen windows don't get resized"). On Hyprland the
request is held for good and the call still reports success.
`SDL_RestoreWindow` sends `xdg_toplevel_unset_maximized` and waits a
roundtrip; Hyprland re-sends maximized, so the flag never clears. A
fullscreen request is a state request, not a size, and works.

## What it costs us

- `<window size>` and the control API's `/window` (the `set_window_size`
  tool) are correct per SDL and do nothing on Hyprland by default, for
  every SolidRT app and every Hyprland user, not just this desktop. An app
  that changes shape while running (a compact mode, a growing panel) stays
  at the size the compositor gave it, unless the client runs with the
  opt-in below.
- Verifying the resize path of an app on a Hyprland desktop means checking
  that the command reached SDL (a temporary log in ore's `set_size`) and
  resizing from the compositor side: `hl.dsp.window.resize({x=..., y=...,
  relative=false, window=w})` in `hyprctl eval`.
- XWayland is no workaround on this machine: `SDL_VIDEO_DRIVER=x11` fails
  to create a GLES window surface.

## Our workaround

Opt-in, through the environment variable `SOLIDRT_RESIZE_VIA_LIMITS`: set
to anything but an empty string or "0" in the client's environment (a `sol
run` passes its environment to the client it launches), ore's
`Window::set_size` resizes through the window's size limits. This is the one
way a client can resize itself on Hyprland, per the maintainer in
hyprwm/Hyprland discussion #11723 (mpv confirmed it works).

- `set_size` pins the minimum and the maximum size
  (`SDL_SetWindowMinimumSize` / `SDL_SetWindowMaximumSize`) to the target
  in SDL window units before the `SDL_SetWindowSize`, and answers
  `SizeRequest::Forced`.
- SDL's Wayland backend sends the xdg min and max requests at once, but the
  compositor only sees them at the next wl_surface commit. So alloy
  requests a frame on `Forced`; its present commits the surface.
- On every resize event the platform loop calls `settle_forced_size`: once
  the window has the target size, the limits are lifted (0x0) and the next
  commit, the first frame at the new size, carries that. A resize to any
  other size leaves them. `set_fullscreen` lifts them first, because SDL
  re-applies the stored limits when a window leaves fullscreen.

It is opt-in because it is a contortion in ore's window contract, for one
compositor's choice. It costs this: while a forced size is pending, the
user cannot drag the window to another size. The docs name it (ore's
`Window::set_size`, `packages/cli/agents/debugging.md`, the `<window size>`
prop).

Tried on the machine and ruled out, 2026-11-13:

- `hl.dsp.window.fullscreen_state({internal=0, client=0})` on the window:
  the fullscreen states are not the maximized state; the resize still does
  not apply.
- Toggling the window between tiled and floating: Hyprland re-sends
  maximized to the floating window.
- `SDL_RestoreWindow` before the size (a temporary patch in ore): the flag
  never clears.
- `windowrule suppressevent maximize` suppresses the client's own maximize
  requests (hyprwm/Hyprland#16503 uses it to stop a client's
  `set_maximized` echo); it does not stop Hyprland's state.

## Upstream

Known and kept on purpose:

- hyprwm/Hyprland#3830 (2023-11): Firefox mis-rendered on main; closed by
  commit ae46fba "xdg: set state maximized for all tiled windows", which
  made tiled windows stop drawing their own decorations after wlroots
  stopped doing it for them. A follow-up comment there already reported a
  Firefox dialog opening "maximized" against its window rules.
- hyprwm/Hyprland#9171 (2025-01, "Have option to not tell windows they are
  maximized"): Dolphin and Godot lose their floating sizes. The maintainer
  (vaxerski): "window rule is enoguh"; "fwiw if you want to patch it out
  you can just remove what CXDGToplevelResource::setMaximized does"; the
  maximized state has to stay because the tiled state alone does not stop
  client-side decorations and Hyprland's renderer does not handle them.
  Closed 2025-03-08 as completed by PR #9422, which added the
  `persistentsize` window rule instead; the reporter objected that the
  request was unmet. No option came out of it.
- hyprwm/Hyprland discussion #11723 (2025-09, mpv does not follow its video
  size): the maintainer confirms the tiled and maximized states are sent
  "because many apps will draw non-window CSD", and that the only way for
  a window to resize itself is through min and max size. Closed as
  resolved 2026-10 without a native change.
- hyprwm/Hyprland#16503 (2026-10, a `size` rule overridden by a maximize
  echo): auto-closed, issues are discussion-only now.

Not filed: the position is settled upstream (a design choice, not an
oversight) and new issues are closed by a bot in favour of discussions. If
it is raised, the ask is a per-window rule that withholds the maximized and
tiled states, or honouring xdg-decoration's server-side mode as the way to
suppress client decorations; the report above is the write-up.
