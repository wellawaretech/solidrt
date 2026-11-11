---
title: SDL AAudio never recovers a playback stream that was disconnected while paused
description: On Android, when AAudio disconnects a stream while the app is in the background, SDL's resume calls requestStart, ignores the error it returns, and its device thread then waits forever on a data callback that never comes, so playback (and recording) stays dead until the device is closed; alloy reopens the playback stream when it does not pull after a resume.
created: 2026-11-03
status: fixed-upstream
project: SDL (libsdl-org/SDL)
versions: SDL 3.4.18 (sdl3-src 3.4.18); same code on main as of 2026-11-03; fixed on main and release-3.4.x 2026-11-03, after the 3.4.20 tag
link: https://github.com/libsdl-org/SDL/issues/16640
---

# SDL AAudio never recovers a playback stream that was disconnected while paused

## Upstream outcome

SDL#16640 was closed as completed on 2026-11-09. The fix is main f7c5d8e
and release-3.4.x 8548caa, "android: Fix aaudio resume-from-freeze"
(2026-11-03). It is the suggested fix below, and it also covers recording:
a failed `requestStart` sets `error_callback_triggered` and signals the
semaphore, and `AAUDIO_RecordDevice` now calls `RecoverAAudioDevice` as
`AAUDIO_PlayDevice` does, instead of failing the read.

Not in a release yet as of 2026-11-10: the newest tag is 3.4.20
(2026-11-03), whose notes do not list it, and the 3.4.x commit sits above
3.4.20's float-recording fix. Our pin is `sdl3-src` 3.4.18. Check that the
3.4.x branch also carries 36864b6 (main-only in the file history seen), the
crash fix for a failed `RecoverAAudioDevice`, which the new recording path
relies on. When a 3.4.x release with 8548caa is pinned: set `resolved`,
remove the workaround listed under "Our workaround", and close
`okf/backlog/android-microphone-dead-after-background.md`.

## Upstream (as filed)

Filed by someone else on 2026-11-02 as libsdl-org/SDL#16640 (playback
only, milestone 3.6.0), with the same two-line fix as below. Their trigger
is Android's freezer: the audio server closes the streams of an app it
froze in the background (`closeStream close frozen AAudio stream`). They
also carry commit `36864b60a2a5` (on main since 2026-10-13, not in 3.4.18),
which stops a failed `RecoverAAudioDevice` from crashing; their fix sends
every such resume through that path. We added the recording case and the
`am freeze` repro on 2026-11-03:
https://github.com/libsdl-org/SDL/issues/16640#issuecomment-6600937853

## Summary

On Android, playback goes silent for good after the app comes back from
the background, if Android disconnected the AAudio stream while it was
paused. Every `SDL_AudioStream` bound to the device stops being pulled. No
error reaches the app. Closing the device and opening it again recovers.

Seen on a Pixel 7 (Android 17) after about 20 s in the background.
Logcat at resume:

```
AAudio  AAudioStream_requestStart(s#1) called
AAudioStream: systemStart() stream is disconnected
AAudio  AAudioStream_requestStart(s#1) returned -895
```

`-895` is `AAUDIO_ERROR_INVALID_STATE`, although AAudio's own line says the
stream is disconnected. A process frozen while in the background (see
Verify) fails the same restart with `-899`, `AAUDIO_ERROR_DISCONNECTED`:

```
AAudioStream: setDisconnected setting disconnected, current disconnected: 0, current state: 3
AAudio  AAudioStream_requestStart(s#1) returned -899
```

SDL handles every failed restart the same way, so the code does not matter.

## The chain (src/audio/aaudio/SDL_aaudio.c)

1. `Android_OnPause` -> `AAUDIO_PauseDevices` -> `PauseOneDevice` calls
   `AAudioStream_requestPause`.
2. While paused, Android disconnects the stream. AAudio does not call the
   error callback for a paused stream, so `error_callback_triggered` stays 0.
3. `Android_OnResume` -> `AAUDIO_ResumeDevices` -> `ResumeOneDevice` calls
   `AAudioStream_requestStart`. It fails (`AAUDIO_ERROR_DISCONNECTED` or
   `AAUDIO_ERROR_INVALID_STATE`).
   The failure goes only to `LOGI` (compiled out by default) and
   `SDL_SetError` on the Java thread. `AAUDIO_ResumeDevices` ignores it.
4. The stream never runs, so `AAUDIO_dataCallback` never signals
   `hidden->semaphore`. `AAUDIO_WaitDevice` loops on
   `SDL_WaitSemaphoreTimeout(..., 100)` until shutdown.
5. `AAUDIO_PlayDevice` is the only caller of `RecoverAAudioDevice`, and it
   never runs again. The device is dead until it is closed.

A disconnect while the stream runs (headphones unplugged) is not affected:
the error callback fires, sets `error_callback_triggered`, signals the
semaphore, and `PlayDevice` rebuilds the stream.

## Suggested fix

In `ResumeOneDevice`, treat a failed `requestStart` like an error callback,
so the device thread wakes and `PlayDevice` runs the existing recovery:

```c
aaudio_result_t res = ctx.AAudioStream_requestStart(hidden->stream);
if (res != AAUDIO_OK) {
    LOGI("SDL Failed AAudioStream_requestStart %d", res);
    SDL_SetError("%s : %s", SDL_FUNCTION, ctx.AAudio_convertResultToText(res));
    SDL_SetAtomicInt(&hidden->error_callback_triggered, (int) res);
    SDL_SignalSemaphore(hidden->semaphore);
}
```

For playback devices only: `AAUDIO_RecordDevice` reads the flag as a read
error and does not recover.

## Our workaround

`alloy/src/audio/device.rs` (`device_foregrounded`, called from the event
watch in `alloy/src/app.rs` on `AppDidEnterForeground`, Android only):
after a return to the foreground, if the device callback has not pulled
within `RESUME_PULL_WINDOW`, the device stream is destroyed and opened
again on the same mixer state. Destroying the only stream on the device
closes the physical device, which ends the wedged device thread; the new
open builds a fresh AAudio stream.

To remove once the fix is in our SDL: delete `device_foregrounded`,
`device_backgrounded`, `RESUME_PULL_WINDOW`, `OPEN`, `TRANSITIONS`,
`Shared::reopen` and the `pulls` counter in `alloy/src/audio/device.rs`,
their re-export in `alloy/src/audio/mod.rs`, and the two calls in the event
watch in `alloy/src/app.rs`. The stream pointer can go back to an
`AtomicPtr` set once at open.

## Recording devices

The same resume path serves a recording device: `PauseOneDevice` stops it
(`requestStop`) and `ResumeOneDevice` starts it again and ignores the
result. A recording stream that Android disconnects while it is stopped
wedges in the same way.

Seen on the Pixel 7, 2026-11-03, with a probe app that played a 1 kHz tone
and read a 16 kHz recording stream once a second. Before backgrounding it
read 16000 samples a second with the tone clearly in them. After the freeze
cycle from Verify, both restarts failed:

```
AAudioStream_requestStart(s#2) returned -899   <- recording
AAudioStream_requestStart(s#1) returned -899   <- playback
[alloy::audio] Playback device did not restart after resume; reopening it
AAudioStream_requestStart(s#3) returned 0      <- playback only
```

From then on the recording stream delivered no samples at all, on every
read, and no error reached the app. This is a dead stream, not Android's
background-capture silencing, which delivers exact zeros.

alloy has no workaround for the microphone yet:
`okf/backlog/android-microphone-dead-after-background.md`. Each session
there owns its own logical device, and SDL closes the wedged physical
device only when its last logical device closes, so a reopen has to close
every session on that device first.

The suggested fix above does not cover recording. In `AAUDIO_RecordDevice`
a set `error_callback_triggered` makes the read fail, and SDL then drops
the device, so the app sees a device loss instead of silence. A report
upstream should name both cases. A full fix for recording could call
`RecoverAAudioDevice` from `AAUDIO_RecordDevice` as `AAUDIO_PlayDevice`
does (not tried).

## Verify

A plain background spell does not trigger the disconnect reliably: on the
Pixel 7, 30 s and 70 s in the background restarted the stream cleanly.
Freezing the process while it is in the background triggers it every time.
`am freeze` needs no root:

```
adb shell input keyevent KEYCODE_HOME
adb shell am freeze <pid>        # wait about 25 s
adb shell am unfreeze <pid>
adb shell am start -n <package>/com.solidrt.app.MainActivity
```

Run it with an app that plays a looped sound (`examples/audio`). Then check
logcat and `/stats` (`audioRenderMaxMs` resets on each read, so a value
above zero on every read means the device pulls).

- With the workaround: `requestStart` fails, 500 ms later
  `[alloy::audio] Playback device did not restart after resume; reopening
  it`, then a new stream (`s#2`) opens and starts with result 0, and
  `audioRenderMaxMs` stays above zero. Verified 2026-11-03 on the Pixel 7.
  The clean restarts above did not log the warning or reopen.
- With a fixed SDL and the workaround removed: SDL logs
  `aaudio: Audio device triggered error -899` and rebuilds the stream
  itself.
