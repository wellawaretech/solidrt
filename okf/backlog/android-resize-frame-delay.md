---
title: Find the 0.5 s to 0.7 s between an Android surface resize and the engine's next frame
description: On the SM-T500 the engine presents its first frame at a new size 0.5 s to 0.7 s after SDL's surfaceChanged() on a rotation, while the system's own windows redraw in 50 ms to 300 ms; the frame itself builds in about 60 ms, so most of the wait comes before the UI thread starts it. Since held redraws, the screen stays frozen for that whole time.
created: 2026-11-01
---

# Find the 0.5 s to 0.7 s between an Android surface resize and the engine's next frame

What it looks like: rotate the SM-T500 tablet (Android 12) with an app at
rest (Player.dev on its launcher screen). The window manager keeps the
screen frozen 0.7 s to 1.2 s, "due to" our window. The status bar,
navigation bar and edge panel finish their orientation draws in 50 ms to
300 ms.

Since [[android-launch-home-flash]] the window's draw report waits for a
frame at the new size, so this delay is now the freeze. Before that it was
a stale, wrong-size frame on screen for the same time. The correct frame
lands at the same moment either way: five rotations measured without the
hold gave 517 ms to 705 ms from SDL's `surfaceChanged()` to alloy's
`[alloy] frame size A -> B` line.

Where the time is not: the frame. In one traced rotation, logcat shows
nothing from our process between the window's own redraw (51.33) and the
UI thread's "Slow frame: 62.8 ms" line (52.08), and the hold released 20 ms
after it. So about 0.7 s passes before the UI thread starts building the
frame. Candidates, none checked: SDL's delivery of the resize to the main
loop (its Android backend queues the size change and the pump may not see
it at once); the main loop's wait (the vsync release chain while the
display is frozen and reconfiguring, see [[android-vsync-release-chain]]);
the repaint latch reaching the UI thread (liveness.rs).

## Done looks like

- The time from `surfaceChanged()` to the first new-size frame is split
  into its steps (event delivery, frame signal, UI build, raster, swap)
  with a number on each, from one traced rotation.
- The step that holds most of it is named, and fixed if it is ours; or
  written up as a limit if it is the platform's.
- Rotation freeze on the SM-T500 measured again after any fix, with
  [[android-launch-handover]]'s recipe.

## What it involves

Log lines with timestamps at each step of one rotation (the SDL window
event, the Resize the loop emits, the frame signal, the UI frame start,
the raster's present), on the tablet, then the same on the TV to see if
the cost is per device.
