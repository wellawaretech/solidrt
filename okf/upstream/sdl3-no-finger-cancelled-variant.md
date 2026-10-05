---
title: sdl3 crate has no Event variant for SDL_EVENT_FINGER_CANCELED
description: The sdl3 crate maps SDL_EVENT_FINGER_DOWN/UP/MOTION to Event variants but not SDL_EVENT_FINGER_CANCELED, which arrives as Event::Unknown { type_ } with the finger id and position dropped.
created: 2026-10-05
status: unfiled
project: sdl3 (Rust bindings)
versions: sdl3 0.18.4 over SDL 3.4.10
link:
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

Our workaround (okf/plans/touch-cancel.md): the pump loop in
`alloy/src/app.rs` polls raw `SDL_Event`s through wrappers in
`alloy/src/sdl_utils.rs` and reads the `tfinger` payload itself when the
type is `SDL_EVENT_FINGER_CANCELED`; everything else still goes through
`Event::from_ll`. When the variant lands, the wrappers go and
`translate_event` gets a `FingerCancelled` arm like `FingerUp`.
