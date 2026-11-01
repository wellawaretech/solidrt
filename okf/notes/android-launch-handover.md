---
title: Recording and reading the Android launch hand-over
description: How to record what the screen shows between a tap and an app's first frame on Android (launcher launch for the icon splash, screenrecord, frame sheets, brightness for black), what decides the hand-over (the starting window leaves on the window's draw report, SurfaceView holds that report only through Callback2), and two measured facts nearby (0.5 s to 0.7 s from surfaceChanged to a new-size frame on the SM-T500; jni 0.22's with_env makes no local frame).
created: 2026-11-01
---

# Recording and reading the Android launch hand-over

Measured on the SM-T500 tablet (Android 12, API 31) and the Philips
TPM171E TV (Android 8, API 26) while building
[[android-launch-home-flash]].

## The starting window and what ends it

- API 31+: the icon splash shows only for a launch from a system surface
  (home screen, app drawer). `am start` and `monkey` get a plain splash
  without the icon, so a recording made that way does not show what users
  see. Tap the icon: `adb shell input tap x y` on the drawer, after
  `KEYCODE_HOME` twice and an upward swipe, with a second or two between
  steps so the launcher settles.
- Below API 31 the starting window is the theme's `windowBackground` for
  any launch (`Theme.NoTitleBar.Fullscreen`: the old dark grey gradient).
- Either one leaves when the window reports its first draw to the window
  manager. A SurfaceView whose holder callback is a plain
  `SurfaceHolder.Callback` finishes its redraw at once and punches its hole
  in the window on the next draw, before anything was drawn into it. As a
  `Callback2`, `surfaceRedrawNeededAsync` holds both the hole and the
  window's draw report until the continuation runs (SurfaceView.java,
  SurfaceCallbackHelper.java, Android 14 source).

## Recording

- `adb shell screenrecord --bit-rate 20000000 --time-limit 7
  /sdcard/x.mp4` in the background, then the launch, then `adb pull`. The
  SM-T500 encoder refuses 2000x1200 and falls back to 1280x720; the TV
  records 1920x1080. Frames come only when the screen changes, so a gap in
  the timestamps is a held frame, not a dropped one.
- A frame sheet to step through:
  `ffmpeg -i x.mp4 -vf "scale=240:-1,drawtext=text='%{n} %{pts\:flt}':x=4:y=4:fontsize=16:fontcolor=yellow:box=1,tile=6x10" -frames:v 1 sheet.png`.
- Dark frames are hard to judge by eye. Per-frame brightness settles it:
  `ffmpeg -i x.mp4 -vf "signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-" -f null -`.
  YAVG 16 is pure black (limited range); the TV's starting-window gradient
  reads about 34.
- Rotation timing without a recording: logcat's `WindowManager` lines
  `finishDrawing of orientation change: Window{... <package>} N ms` and
  `Screen frozen for N ms due to Window{...}`, against SDL's
  `surfaceChanged()` and alloy's `[alloy] frame size A -> B`.
  `settings put system accelerometer_rotation 0` and `user_rotation 0/1`
  force a rotation; put both back after.

## Measured nearby

- On the SM-T500 the engine presents its first frame at a new size 0.5 s
  to 0.7 s after SDL's `surfaceChanged()` (five rotations, Player.dev at
  rest). The window manager's own windows redraw in 50 ms to 300 ms. Where
  that time goes is not known: [[android-resize-frame-delay]].
- jni 0.22's `EnvUnowned::with_env` creates no JNI frame. On a thread that
  never returns to Java (SDL's), every local reference made inside it
  stays until the thread ends, unless the code makes its own frame
  (`with_local_frame`) or deletes it. `sdl_utils::with_android_activity`
  does both since 2026-11-01.
