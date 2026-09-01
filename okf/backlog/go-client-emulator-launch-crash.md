---
title: Go client crashes at launch on the x86_64 emulator
description: The currently staged solidrt-go.apk aborts within a second of launch on the srt_pixel6 emulator (FORTIFY, destroyed mutex inside libhwui's CommonPool); an older build installed on the same AVD launches fine, so this is a regression in the artifact, not an emulator setup problem.
created: 2026-09-01
---

# Go client crashes at launch on the x86_64 emulator

Found while verifying `srt pack --apk` (2026-09-01): every launch of the
staged fat release APK (`dist/android/arm64-v8a/solidrt-go.apk`, arm64 +
x86_64) on the `srt_pixel6` emulator (API 36, x86_64, `-gpu host`) dies
within ~1 s of `am start`, before any window content. Reproduced 7/7 across
fresh installs; the APK that was already installed on the AVD (a ~June
build) launched fine in the same emulator boot, so the regression is in the
artifact somewhere between then and now. Patched (`srt pack --apk`) and
byte-original APKs crash identically, which is how it surfaced.

The abort:

```
FORTIFY: pthread_mutex_lock called on a destroyed mutex (0x73d2daa02c38)
Fatal signal 6 (SIGABRT) in tid (hwuiTask0/hwuiTask1/RenderThread), pid (SDLActivity)
```

Backtrace is entirely inside the platform: bionic `pthread_cond_wait` under
`libhwui.so` `android::uirenderer::CommonPool::CommonPool()`'s worker
thread; no frames in libmain/SDL. HWUI's static pool mutex being destroyed
under a waiting worker is the signature of the process tearing down (exit /
static destructors) while HWUI is still starting up - which points at the
native side exiting very early, with the hwui abort as the messenger rather
than the cause. Timeline fits: libs load ok, `SDL_main` starts, abort
~200 ms later. Rust panic output does not reach logcat (only `log` lines do,
tag `SDL/APP`), so whatever the native side said on the way out was lost.

Next steps when picking this up:

- Build the debug x86_64 client per the emulator setup notes
  (`make x-solidrt-go-android ANDROID_ABI=x86_64 PROFILE=debug`) and launch
  that: a debug build may panic visibly instead, and confirms or rules out
  release-only teardown timing.
- If the debug build runs, bisect release vs debug; if it also dies, bisect
  the June-to-now range (candidate areas: frame pacing, demand-driven
  rendering, raster-thread changes - all touched the startup path).
- CONFIRMED emulator-only (2026-09-01): the same artifact (as the
  `srt pack --apk`-patched console APK, byte-equivalent modulo id/label/
  payload) installs, launches and stays resumed on an arm64 device
  (SM-T500, Android 12) - SDL_main runs, alloy reports a frame size, no
  aborts. The crash is confined to the emulator's x86_64 GLES translator
  path, so the debug-x86_64 rebuild above is the reproduction vehicle.
