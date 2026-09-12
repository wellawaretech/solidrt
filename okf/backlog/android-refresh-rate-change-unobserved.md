---
title: A display refresh-rate change is not observed by the client
description: A Pixel 7 forced from 90 to 60 Hz while the client ran kept reporting periodMs 11.11, so the jank accounting counted every frame against 90 Hz (149 missed presents in a window that presented on every vsync); the refresh-rate fact is read at init and by a polled safety net that did not pick the change up.
created: 2026-09-12
---

# A display refresh-rate change is not observed by the client

Seen 2026-09-12 while checking the cadence probe on a Pixel 7 (Android 17):
`settings put system peak_refresh_rate 60.0` (and min) switched the panel
to 60 Hz - the client presented 301 frames per 5 s - but `/stats` kept
`periodMs` 11.11 and `missedPresents` climbed to 149 per window. The main
loop derives its tick period and the raster thread its miss accounting
from `watch.refresh_rate()` (`PlatformWatch`, `alloy/src/app.rs`), which
SDL feeds through the display-mode event and a polled safety net; neither
saw the switch.

## Done looks like

After a mode switch on device, `periodMs` follows within a second and
`missedPresents` stays at zero for an app presenting every vsync.

## What it involves

Finding which of SDL's display events fires on Android for a mode change
(if any) and, failing that, polling `SDL_GetCurrentDisplayMode` in the
safety net at a rate that catches it.
