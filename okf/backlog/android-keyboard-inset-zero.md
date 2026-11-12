---
title: The screen keyboard's inset reads 0 on the tablet
description: On the SM-T500 (Android 12) the docked screen keyboard covers the lower half of the window, Android reports the IME inset to the activity, yet the app's keyboardHeight stays 0 - the activity's inset listener never reaches ore's keyboard_inset_px; seen on the current build and on the older installed player alike, so it is not a regression of the pump move.
created: 2026-11-12
---

# The screen keyboard's inset reads 0 on the tablet

Seen 2026-11-12 while verifying step 2 of okf/done/ore-pump-and-window.md
on the maintainer's tablet (SM-T500, Android 12, display scale 1.5):

- A focused field with `onTextInput`, tapped, opens the docked Samsung
  keyboard; a screenshot shows it covering the window from y=639 of 1200
  physical pixels down. `dumpsys input_method` says `mInputShown=true`.
- logcat shows the system handing the IME inset to the activity's window
  (`InsetsSourceConsumer setRequestedVisible: visible=true, type=19`,
  `ITYPE_IME mFrame=[0,639][2000,1200]`).
- The app's raw `keyboardVisibility` event reads `{ shown: true, height:
  0 }` and then `{ shown: false, height: 0 }`: the pump's watch sees SDL's
  "shown" flip, but `ore::window::keyboard_inset_px()` stays 0, so
  `keyboardHeight()` never moves and content never lifts.
- The older release player installed on the same tablet (built before the
  pump move) reports the same 0, so the gap predates
  okf/done/ore-pump-and-window.md step 2.

So the Java side never calls `nativeKeyboardInset` with a nonzero value, or
calls it with 0: `SolidRTActivity` sets `setOnApplyWindowInsetsListener` on
`android.R.id.content` and reads `insets.getInsets(WindowInsets.Type.ime())
.bottom` (API 30 and up; the tablet qualifies). Suspects, to tell apart with
a log line in the listener: the listener is never invoked (another view in
the chain consumes the insets first, or the decor does not dispatch to
`content` under the activity's fullscreen flags), or it is invoked before
the IME animation and reads 0 and never again.

Done looks like: `keyboardHeight()` reads the keyboard's overlap in logical
points on this tablet when the docked keyboard opens, and 0 when it closes;
a probe that lifts a field by it keeps the field visible.

What it involves: a log line in the Java listener and a run on the tablet
(okf/notes has the measurement setup); then either moving the listener to
the decor view or the SDL surface, or reading the inset from
`ViewRootImpl`'s `WindowInsets` on each `onApplyWindowInsets` of the SDL
layout. The native side (`ore::window::set_keyboard_inset_px`, the watch's
`KeyboardVisibility`) is in place and verified for the `shown` half.
