---
title: The microphone open freezes the UI behind the Android prompt
description: SDL requests RECORD_AUDIO with a blocking callback inside the recording open, so the synchronous flux:microphone open() holds the UI thread while the system dialog is up and reports a denial as an error; the microphone gets the camera's permission flow, an open that is Pending until the grant, Denied or Ready, with open() a promise and createMicrophone carrying the state.
created: 2026-10-11
completed: 2026-10-12
---

# The microphone open freezes the UI behind the Android prompt

## Symptom

On Android, `SDL_OpenAudioDeviceStream` for a recording device calls
`SDL_RequestAndroidPermission("android.permission.RECORD_AUDIO", ..)` with
a blocking callback, inside the open (`SDL_aaudio.c`, `SDL_openslES.c` in
SDL 3.4). `flux:microphone`'s `open()` is synchronous and runs on the UI
thread, so:

- The app sits frozen behind the system dialog until the user answers:
  no frame, no input, however long they take.
- A denial comes back as the open's error ("This app doesn't have
  RECORD_AUDIO permission"), so `createMicrophone` reports it through
  `error()`, the same as a missing device, and the app cannot tell a
  user's no from a hardware fault, nor show "waiting for permission".

The camera has none of this: SDL's camera API is asynchronous, the session
is `Pending` until `SDL_GetCameraPermissionState` answers, then `Denied`
or `Ready`, polled per frame, and `open()` resolves a promise. Found while
building the microphone ring (okf/backlog/microphone-tap.md), where it was
left out as a contract change.

## Shape

The camera's flow, for audio:

- alloy: `sdl_utils` wraps the non-blocking `SDL_RequestAndroidPermission`
  (a callback that records the answer; our marshalling, not SDL's
  blocking helper). A session starts `Pending`; on Android the permission
  is requested first and the SDL open runs once granted, on every other
  platform the open runs at once (no prompt exists there). Status
  `Pending | Denied | Ready { sample_rate } | Failed(msg)`, read by a
  per-frame pump like `pump_cameras`; the ring exists from the open so a
  tap taken while Pending hears from the grant. The test source is
  approved at once.
- flux: `open()` returns a promise, settled from the pump, with a pending
  list like the camera plugin's; the session object is unchanged
  (`{ id, sampleRate, read, close }`).
- core: `createMicrophone` keeps its shape with the session resolving
  later: `id()` and `sampleRate()` undefined and 0 until Ready, `error()`
  set on Denied or Failed, and a `denied()` or the error naming the denial
  so an app can route the user to settings.
- flux-types and the docs, which today state that `open` is synchronous
  and that current platforms show no prompt.

## Done looks like

- On the Android tablet the app keeps rendering behind the RECORD_AUDIO
  dialog, a grant opens the microphone, a denial reaches the app as a
  denial.
- Linux and the test source resolve at once and the microphone tests
  (`packages/core/tests/microphone.test.tsx`) still pass unchanged apart
  from the await.

## Outcome

Built as shaped, with one change: there is no per-frame pump. The
camera's tick exists to create and refresh a texture each frame; a
microphone has no per-frame work, so each opener (the `flux:microphone`
open, the speech plugin's start) waits on its own session from a task on
the engine's executor, polling `pump_microphone` under an in-flight hold,
and settles its promise itself without a frame. Checked on the Android
tablet: the frame counter keeps advancing behind the RECORD_AUDIO dialog,
a grant opens capture at the requested rate, a denial reaches the app as
"microphone permission denied", and a later open after "Don't allow" is
answered denied at once, without a dialog. SDL's own blocking request
inside the recording open is harmless once the permission is held: the
Java side answers it synchronously.

## Not in scope

- A settings deep link for a permanently denied permission.
