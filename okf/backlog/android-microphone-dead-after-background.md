---
title: Reopen a microphone that SDL leaves dead after an Android resume
description: When Android disconnects the AAudio recording stream while the app is in the background, SDL's resume ignores the failed restart and the session delivers no samples ever again, with no error; alloy needs the same reopen-after-resume workaround it has for playback, per recording stream.
created: 2026-11-03
---

# Reopen a microphone that SDL leaves dead after an Android resume

## Problem

An open microphone session stops delivering samples for good after the app
comes back from the background, if Android disconnected the AAudio
recording stream meanwhile. `read()` returns empty arrays, `error()` stays
unset, and the session never recovers until it is closed and opened again.

The cause is in SDL, and it is the same bug as for playback:
`okf/upstream/sdl-android-aaudio-resume-disconnected-never-recovers.md`.
SDL stops the recording stream at pause, `ResumeOneDevice` ignores the
failed `requestStart`, and nothing reopens the device.

Seen on the Pixel 7 (Android 17), 2026-11-03, with a probe app that played a
1 kHz tone and read a 16 kHz session once a second:

| | samples/s | RMS | tone share |
|---|---|---|---|
| before backgrounding | 16000 | 0.12 | 0.999 |
| after background + `am freeze` 25 s + resume | 0 on every read | 0 | 0 |

```
AAudioStream_requestStart(s#2) returned -899   <- microphone
AAudioStream_requestStart(s#1) returned -899   <- playback
[alloy::audio] Playback device did not restart after resume; reopening it
AAudioStream_requestStart(s#3) returned 0      <- playback only
[probe] samples=0 rms=0.0000 tone=0.000        <- every read after
```

The microphone is dead, not muted: Android's background-capture silencing
delivers exact zeros, this delivers nothing.

## Done looks like

After the same freeze cycle, the session delivers samples again within
about a second of the resume, with the same session id and the same
`read()`/tap readers. The reopen is logged. A clean resume (the stream
restarts by itself) reopens nothing.

## What it involves

The playback workaround (`device_foregrounded` in
`alloy/src/audio/device.rs`) is the model: after a resume, a stream that
does not deliver within a window is destroyed and opened again on the same
state. What is different for the microphone:

- **Several streams.** Each session owns its own recording stream
  (`Source::Device` in `alloy/src/microphone/mod.rs`), each a logical device
  on a physical one. SDL closes the physical device, and so ends its wedged
  device thread, only when its last logical device closes. So the reopen
  must destroy every session's stream on that physical device before it
  opens any of them again. Reopening one session while another stays open
  on the same device keeps the dead device.
- **The thread.** The sessions live in a `RefCell` on the UI thread, while
  the resume is seen on the main thread (the event watch in
  `alloy/src/app.rs`). Either the main thread only marks the resume (an
  atomic, as `TRANSITIONS` does for playback) and the UI thread checks and
  reopens, for example from `pump_microphone`/`read_microphone`, or the
  session state moves behind a lock. The first keeps the registry as it is.
- **The sign of life.** The put callback pushes into the session's
  `SampleRing`, so a count of pushed frames that does not move after the
  window marks a dead stream. Exact zeros still count as delivered.
- **What survives.** The ring, the session's reader and every tap stay; only
  the stream and the callback's ring clone are replaced, in the order the
  `Session` drop already uses (destroy the stream, then release the clone).

Remove it with the playback workaround once the SDL fix is in our tree. With
that fix as suggested in the upstream note, a failed recording restart
becomes a read error and SDL drops the device, so the session would see a
device loss instead of silence; whether alloy then reopens or reports the
loss is its own decision.

## Verify

The probe and the freeze steps from the upstream note's Verify section:
before, after the freeze cycle, and after a plain background spell. Check
two sessions open on the default microphone at once as well.
