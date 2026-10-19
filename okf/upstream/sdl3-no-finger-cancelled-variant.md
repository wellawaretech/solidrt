---
title: sdl3 crate has no Event variant for SDL_EVENT_FINGER_CANCELED
description: The sdl3 crate maps SDL_EVENT_FINGER_DOWN/UP/MOTION to Event variants but not SDL_EVENT_FINGER_CANCELED, which arrives as Event::Unknown { type_ } with the finger id and position dropped.
created: 2026-10-05
status: resolved
project: sdl3 (Rust bindings, github.com/vhspace/sdl3-rs)
versions: sdl3 0.18.4 over SDL 3.4.10; fixed in 0.19.0 (2026-09-06), our pin since 2026-10-19
link: https://github.com/vhspace/sdl3-rs/pull/438
---

# sdl3 crate has no Event variant for SDL_EVENT_FINGER_CANCELED

SDL 3 sends `SDL_EVENT_FINGER_CANCELED` (type 0x703) when the system
takes a touch away from the window: Wayland's `wl_touch.cancel`, UIKit's
`touchesCancelled`, Cocoa's touch cancel, Android's `ACTION_CANCEL`. Its
payload is an `SDL_TouchFingerEvent` like the down, up and motion
events: `touchID`, `fingerID`, `x`, `y`, `dx`, `dy`, `pressure`,
`windowID`.

`Event::from_ll` in the sdl3 crate (src/sdl3/event.rs) maps
`SDL_EVENT_FINGER_DOWN`, `SDL_EVENT_FINGER_UP` and
`SDL_EVENT_FINGER_MOTION` to `Event::FingerDown`, `FingerUp` and
`FingerMotion`, and has no arm for `SDL_EVENT_FINGER_CANCELED`. It
falls to the default arm and becomes `Event::Unknown { timestamp,
type_ }`, which drops the finger id and position. A consumer cannot
tell which finger was cancelled, so it cannot end that pointer.

Suggested fix: an `Event::FingerCancelled { timestamp, touch_id,
finger_id, x, y, dx, dy, pressure, window_id }` variant with the same
fields as `FingerUp`, an `EventType::FingerCancelled` entry, and the
`from_ll` arm beside the other finger events.

Our workaround (okf/done/touch-cancel.md) was a pump loop in
`alloy/src/app.rs` that polled raw `SDL_Event`s through wrappers in
`alloy/src/sdl_utils.rs` and read the `tfinger` payload itself when the
type was `SDL_EVENT_FINGER_CANCELED`; everything else still went through
`Event::from_ll`.

## Outcome

Fixed upstream before we filed, then resolved here. sdl3-rs PR #438
(merged 2026-08-24, released in 0.19.0 on 2026-09-06) adds
`Event::FingerCanceled` with the full finger payload, `EventType::FingerCanceled`
and the `from_ll` arm. The bump to 0.19.0 (2026-10-19, which also moved
SDL from 3.4.10 to 3.4.18 through sdl3-sys 0.7) took the workaround out:
the pump is back on the crate's `EventPump`, and `translate_event` in
`alloy/src/event.rs` maps `FingerCanceled` to `PointerCancel` beside
`FingerUp`.
