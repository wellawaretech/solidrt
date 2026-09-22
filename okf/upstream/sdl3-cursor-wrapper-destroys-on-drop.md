---
title: sdl3 crate's Cursor destroys on drop while SDL keeps showing it, and its SystemCursor stops at 12 of 20 shapes
description: sdl3::mouse::Cursor::set hands the pointer to SDL for as long as it is shown, but the wrapper's Drop calls SDL_DestroyCursor, and SDL reverts to the default cursor when the current one is destroyed; the enum also lacks the eight directional resize shapes SDL 3.2 added.
created: 2026-09-22
status: unfiled
project: sdl3 (Rust bindings, github.com/revmischa/sdl3-rs)
versions: sdl3 0.18.4 and 0.20.0 over sdl3-sys 0.6.6 (SDL 3.4.10)
link:
---

# sdl3 crate's Cursor destroys on drop while SDL keeps showing it, and its SystemCursor stops at 12 of 20 shapes

Two gaps in `sdl3::mouse` (src/sdl3/mouse/mod.rs), both still present in
0.20.0.

## Cursor::set followed by drop reverts to the default cursor

`Cursor::set` calls `SDL_SetCursor(self.raw)`, which makes SDL show that
pointer until another is set. `impl Drop for Cursor` calls
`SDL_DestroyCursor`, and SDL's `SDL_DestroyCursor` (src/events/SDL_mouse.c)
does `if (cursor == mouse->cur_cursor) SDL_SetCursor(mouse->def_cursor)`
before freeing. So the natural use

```rust
Cursor::from_system(SystemCursor::Hand)?.set();
```

sets the hand and reverts to the arrow in the same statement. Nothing in
the API or docs says the `Cursor` must be kept alive; `set(&self)` reads
as a fire-and-forget call. A wrapper that owns the SDL semantics would
either document the lifetime requirement on `set` or keep the current
cursor alive itself (a thread-local of the last set cursor, released on
the next set).

## SystemCursor covers 12 of SDL_SystemCursor's 20 values

The enum maps DEFAULT through POINTER and stops. SDL 3.2 added
`SDL_SYSTEM_CURSOR_NW_RESIZE`, `N_RESIZE`, `NE_RESIZE`, `E_RESIZE`,
`SE_RESIZE`, `S_RESIZE`, `SW_RESIZE` and `W_RESIZE` (the CSS
single-direction resize cursors), which sdl3-sys exposes but the safe enum
cannot name.

## How we work around it

alloy creates cursors through `SDL_CreateSystemCursor` /
`SDL_CreateColorCursor` / `SDL_CreateAnimatedCursor` from sdl3-sys in
`alloy/src/sdl_utils.rs`, wrapped in an `OwnedCursor` that the loop's
cursor cache (`alloy/src/cursor.rs`) keeps alive for as long as SDL may
show it. See okf/done/element-cursor-prop.md.
