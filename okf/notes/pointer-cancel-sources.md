---
title: What cancels a pointer, and what only looks like it does
description: The platform touch cancels are the only producers of a pointer cancel; a desktop focus loss and the dev-tool input mute both deliver the up, so neither strands a pointer and neither needs a synthesized cancel.
created: 2026-10-05
---

# What cancels a pointer, and what only looks like it does

Checked 2026-10-05 while building okf's touch-cancel item, which
translates the platform touch cancel (`SDL_EVENT_FINGER_CANCELED`,
Android's `ACTION_CANCEL`) into `PointerCancel`. The question was whether
the runtime should also synthesize cancels for pointers it knows will
never lift. It should not, because nothing today strands one:

- **Desktop focus loss mid-press.** SDL's Wayland backend releases every
  pressed mouse button on `wl_pointer.leave` (`pointer_handle_leave` in
  SDL_waylandevents.c sends `SDL_SendMouseButton(.., false)` per pressed
  button before clearing mouse focus), so the app gets an ordinary up.
  Touch on Wayland is the cancel case proper (`wl_touch.cancel`), which
  arrives as the finger cancel.
- **The dev-tool input mute** (alloy's run loop, `is_muted_input`) drops
  moves, downs, wheels and key downs but passes releases, so a button or
  finger held when the mute began lifts normally. Its up fires the press
  it belongs to; a cancel there would be more correct but the pointer is
  not stuck.
- **An engine swap mid-press** gives the new engine a router with no
  frozen path; the up hit-tests live and nothing in the old engine
  survives to be cancelled.

So the only producers are the platform touch cancels, which is why the
desk reproduction is the synthetic `{ "type": "pointer", "action":
"cancel" }` input action and the device reproduction is a back swipe
started inside a dragged element on Android.

## Reproducing a cancel on an Android device over adb

Tried 2026-10-05 on the SM-T500 (Android 12, three-button navigation),
with `adb shell input swipe` holding a 4 s drag in the app:

- Opening the recents screen (`input keyevent KEYCODE_APP_SWITCH`) or
  starting Settings on top mid-drag did NOT cancel it: an injected swipe
  runs to its up in the app regardless. Neither is a reproduction.
- `adb shell input motionevent DOWN x y`, `MOVE`, `MOVE`, `CANCEL x y`
  (Android 11+) injects a real `ACTION_CANCEL` into the app's touch
  stream, the same stream a back gesture or the notification shade
  produces, and it reaches the app through the surface's `onTouch` like
  the real thing. That is the device reproduction; a back swipe needs
  gesture navigation, which that tablet does not use.
- Physical coordinates: the tablet reports 1200x2000 but runs the app in
  landscape (2000x1200), so a swipe at y=1200 lands on the navigation
  bar and the app sees nothing. Density 240 scales physical to logical
  by 1.5. A backgrounded local `adb shell input` never injects anything;
  run it in the foreground and interrupt from a second shell.
